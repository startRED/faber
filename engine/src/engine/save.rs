//! Only explicit engine entries and the transcript timer defer writes; direct Core locks stay synchronous.
use super::*;
use crate::store::Fault;
use std::sync::Condvar;

#[derive(Default)]
pub(super) struct Saves {
    pub(super) deferred: Option<Mutex<Vec<Requested>>>,
    pub(super) flights: Arc<Flights>,
    pub(super) pending: Mutex<HashMap<String, u64>>,
    pub(super) completed: Mutex<HashMap<String, (u64, bool)>>,
}

pub(super) struct Requested {
    pub(super) id: String,
    generation: u64,
    patient: bool,
}

pub(super) struct Pending {
    pub(super) id: String,
    generation: u64,
    snapshot: crate::store::Snapshot,
    patient: bool,
}

#[derive(Default)]
pub(super) struct Flights {
    state: Mutex<(usize, Vec<Finished>)>,
    pub(super) queue: Mutex<Queue>,
    done: Condvar,
}

#[derive(Default)]
pub(super) struct Queue {
    writing: bool,
    pub(super) pending: HashMap<String, Pending>,
}

struct Finished {
    id: String,
    generation: u64,
    result: Result<(), Fault>,
}

impl Flights {
    pub(super) fn wait(&self) {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        drop(self.done.wait_while(state, |s| s.0 != 0).unwrap_or_else(|e| e.into_inner()));
    }
}

pub(super) struct Batch {
    store: Arc<Store>,
    flights: Arc<Flights>,
    current: Option<Pending>,
    started: bool,
    writing: bool,
}

impl Batch {
    pub(super) fn write(mut self) {
        self.started = true;
        {
            let mut queue = self.flights.queue.lock().unwrap_or_else(|e| e.into_inner());
            if queue.writing {
                return;
            }
            queue.writing = true;
            self.writing = true;
        }
        loop {
            {
                let mut queue = self.flights.queue.lock().unwrap_or_else(|e| e.into_inner());
                let Some(id) = queue.pending.keys().next().cloned() else {
                    queue.writing = false;
                    self.writing = false;
                    return;
                };
                self.current = queue.pending.remove(&id);
            }
            let save = self.current.as_ref().unwrap();
            let result = self.store.save_prepared(&save.id, &save.snapshot, save.generation, save.patient);
            self.flights.state.lock().unwrap_or_else(|e| e.into_inner()).1.push(Finished {
                id: save.id.clone(),
                generation: save.generation,
                result,
            });
            self.current = None;
        }
    }
}

impl Drop for Batch {
    fn drop(&mut self) {
        let mut unfinished: Vec<_> = self.current.take().into_iter().collect();
        if self.writing || !self.started {
            let mut queue = self.flights.queue.lock().unwrap_or_else(|e| e.into_inner());
            if self.writing || !queue.writing {
                unfinished.extend(queue.pending.drain().map(|(_, save)| save));
                queue.writing = false;
            }
        }
        let mut state = self.flights.state.lock().unwrap_or_else(|e| e.into_inner());
        state.1.extend(unfinished.into_iter().map(|save| Finished { id: save.id, generation: save.generation, result: Err(Fault::Write) }));
        state.0 -= 1;
        self.flights.done.notify_all();
    }
}

impl Core {
    pub(super) fn take_saves(&mut self) -> Batch {
        let pending = self.saves.deferred.take().map(|pending| pending.into_inner().unwrap_or_else(|e| e.into_inner())).unwrap_or_default();
        let mut queue = self.saves.flights.queue.lock().unwrap_or_else(|e| e.into_inner());
        for save in pending {
            // Discard older bytes before serializing once, at the entry's final state; the writer cannot take them meanwhile.
            queue.pending.remove(&save.id);
            if let Some(snapshot) = self.saved_conversation(&save.id, Store::prepare_snapshot) {
                let generation = self
                    .dirty_transcripts
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .get(&save.id)
                    .copied()
                    .unwrap_or(save.generation)
                    .max(save.generation);
                self.saves.pending.lock().unwrap_or_else(|e| e.into_inner()).insert(save.id.clone(), generation);
                queue.pending.insert(save.id.clone(), Pending { id: save.id, generation, snapshot, patient: save.patient });
            }
        }
        // Register before releasing Core: shutdown also sees batches waiting for a blocking-pool worker.
        self.saves.flights.state.lock().unwrap_or_else(|e| e.into_inner()).0 += 1;
        Batch { store: self.store.clone(), flights: self.saves.flights.clone(), current: None, started: false, writing: false }
    }

    pub(super) fn finish_saves(&mut self) {
        let finished = std::mem::take(&mut self.saves.flights.state.lock().unwrap_or_else(|e| e.into_inner()).1);
        for save in finished {
            let _ = self.saved_result(&save.id, save.generation, save.result);
        }
        if !self.closing && !self.dirty_transcripts.lock().unwrap_or_else(|e| e.into_inner()).is_empty() {
            self.save_soon();
        }
    }

    pub(super) fn defer_save(&self, id: &str, patient: Option<bool>, changed: bool) -> bool {
        let Some(pending) = self.saves.deferred.as_ref().filter(|_| !self.closing) else { return false };
        if !self.convs.contains_key(id) {
            self.transcript_saved(id);
            return true;
        }
        if !changed {
            if pending.lock().unwrap_or_else(|e| e.into_inner()).iter().any(|save| save.id == id) {
                return true;
            }
            let Some(dirty) = self.dirty_transcripts.lock().unwrap_or_else(|e| e.into_inner()).get(id).copied() else { return true };
            let pending = self.saves.pending.lock().unwrap_or_else(|e| e.into_inner()).get(id).copied();
            let completed = self.saves.completed.lock().unwrap_or_else(|e| e.into_inner()).get(id).map(|(generation, _)| *generation);
            // A failed write keeps its pending mark for file readers, but must allow the timer to retry.
            if pending.is_some_and(|pending| pending >= dirty && completed.is_none_or(|completed| completed < pending)) {
                return true;
            }
        }
        let generation = self.store.next_generation(id);
        if changed && self.saves.pending.lock().unwrap_or_else(|e| e.into_inner()).contains_key(id) {
            self.dirty_transcripts.lock().unwrap_or_else(|e| e.into_inner()).insert(id.to_string(), generation);
        }
        self.saves.pending.lock().unwrap_or_else(|e| e.into_inner()).insert(id.to_string(), generation);
        let mut pending = pending.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(save) = pending.iter_mut().find(|save| save.id == id) {
            save.generation = generation;
        } else {
            let patient = patient.unwrap_or_else(|| !self.save_failed.lock().unwrap_or_else(|e| e.into_inner()).contains(id));
            pending.push(Requested { id: id.to_string(), generation, patient });
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{Disk, RealDisk};
    use std::io;
    use std::sync::mpsc::{Receiver, Sender, channel};

    struct SlowDisk {
        entered: Sender<()>,
        release: Mutex<Receiver<()>>,
        once: std::sync::atomic::AtomicBool,
        fail: bool,
    }

    impl Disk for SlowDisk {
        fn write(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
            if !self.once.swap(true, std::sync::atomic::Ordering::SeqCst) {
                self.entered.send(()).unwrap();
                self.release.lock().unwrap().recv_timeout(Duration::from_secs(20)).unwrap();
                if self.fail {
                    return Err(io::Error::from_raw_os_error(if cfg!(windows) { 112 } else { 28 }));
                }
            }
            RealDisk.write(path, bytes)
        }

        fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
            RealDisk.rename(from, to)
        }
    }

    fn engine(data: &Path, emit: Emit) -> Engine {
        Engine::new(
            EngineOptions {
                data_dir: data.into(),
                #[cfg(windows)]
                chat_dir: data.with_file_name("chat"),
                clis: BTreeMap::new(),
                spawn: Arc::new(|_, _, _| panic!("save checks never start a CLI")),
                windows: false,
                claude_env_billed: false,
                max_warm: 0,
                idle: Duration::from_secs(120),
                runtime: tokio::runtime::Handle::current(),
                ask: None,
            },
            emit,
        )
        .unwrap()
    }

    fn conversation() -> Conversation {
        let mut c: Conversation = serde_json::from_str(include_str!("../../../tests/fixtures/conversation-export.json")).unwrap();
        c.id = "save-check".into();
        c.messages.clear();
        c
    }

    fn slow(engine: &Engine, data: &Path, fail: bool) -> (Receiver<()>, Sender<()>) {
        let (entered, seen) = channel();
        let (release, wait) = channel();
        engine.with(|c| {
            c.store = Arc::new(Store::with_disk(data, Arc::new(SlowDisk { entered, release: Mutex::new(wait), once: false.into(), fail })));
            c.convs.insert("save-check".into(), conversation());
        });
        (seen, release)
    }

    #[tokio::test]
    async fn an_engine_call_answers_during_a_write_and_the_newer_snapshot_wins() {
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("data");
        let engine = engine(data.as_path(), Arc::new(|_| {}));
        let (entered, release) = slow(&engine, data.as_path(), false);
        let old = engine.clone();
        let first = std::thread::spawn(move || old.rename("save-check", "older"));
        entered.recv_timeout(Duration::from_secs(10)).unwrap();
        let (answered, answer) = channel();
        let reading = engine.clone();
        let read = std::thread::spawn(move || answered.send(reading.with(|c| c.closing)).unwrap());
        let responsive = answer.recv_timeout(Duration::from_secs(2));
        assert_eq!(
            engine.with(|c| c.listed().into_iter().find(|row| row.id == "save-check").unwrap().title),
            "older",
            "the list event reads the current row while its file is still being replaced"
        );
        let (staged, ready) = channel();
        let updating = engine.clone();
        let second = std::thread::spawn(move || {
            updating.with(|c| {
                c.convs.get_mut("save-check").unwrap().title = "newer".into();
                c.save("save-check");
                staged.send(()).unwrap();
            })
        });
        ready.recv_timeout(Duration::from_secs(10)).unwrap();
        release.send(()).unwrap();
        first.join().unwrap();
        second.join().unwrap();
        read.join().unwrap();
        assert!(!responsive.unwrap(), "an unrelated engine call answers before the disk is released");
        assert_eq!(Store::new(data.as_path()).get("save-check").unwrap().title, "newer");
        // Also exercise an older batch starting after a newer save has already returned.
        let stale = {
            let mut c = engine.core.lock().unwrap();
            c.saves.deferred = Some(Mutex::new(Vec::new()));
            c.convs.get_mut("save-check").unwrap().title = "stale".into();
            c.save("save-check");
            c.take_saves()
        };
        engine.with(|c| {
            c.convs.get_mut("save-check").unwrap().title = "newest".into();
            c.try_save("save-check").unwrap();
        });
        stale.write();
        assert_eq!(Store::new(data.as_path()).get("save-check").unwrap().title, "newest");
        engine.shutdown();
    }

    #[tokio::test]
    async fn a_timer_batch_keeps_new_dirty_activity_and_reports_a_late_failure_on_next_entry_once() {
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("data");
        let events = Arc::new(Mutex::new(Vec::new()));
        let heard = events.clone();
        let engine = engine(
            data.as_path(),
            Arc::new(move |e| {
                if let AppEvent::SaveFailed { key } = e {
                    heard.lock().unwrap().push(key)
                }
            }),
        );
        let (entered, release) = slow(&engine, data.as_path(), true);
        // The same registered batch the timer gives to spawn_blocking.
        let batch = {
            let mut c = engine.core.lock().unwrap();
            c.saves.deferred = Some(Mutex::new(Vec::new()));
            c.save("save-check");
            c.take_saves()
        };
        let write = std::thread::spawn(move || batch.write());
        entered.recv_timeout(Duration::from_secs(10)).unwrap();
        engine.with(|c| {
            c.dirty_transcripts.lock().unwrap().insert("save-check".into(), c.store.next_generation("save-check"));
        });
        release.send(()).unwrap();
        write.join().unwrap();
        assert!(events.lock().unwrap().is_empty(), "the writer never emits outside an engine entry");
        engine.with(|c| assert!(c.dirty_transcripts.lock().unwrap().contains_key("save-check")));
        assert_eq!(*events.lock().unwrap(), ["error.chat.saveFull"]);
        engine.with(|_| ());
        assert_eq!(events.lock().unwrap().len(), 1);
        engine.shutdown();
        assert_eq!(events.lock().unwrap().len(), 1);
        assert!(Store::new(data.as_path()).get("save-check").is_some());
    }

    #[tokio::test]
    async fn a_slow_timer_write_does_not_accumulate_more_helper_snapshots() {
        let root = crate::workspace::TestRoot::new();
        let engine = engine(root.path(), Arc::new(|_| {}));
        let (entered, release) = slow(&engine, root.path(), false);
        engine.with(|c| {
            c.dirty_transcripts.lock().unwrap().insert("save-check".into(), c.store.next_generation("save-check"));
            c.save_soon();
        });
        tokio::time::sleep(TRANSCRIPT_SAVE_EVERY + Duration::from_millis(100)).await;
        entered.recv_timeout(Duration::from_secs(10)).unwrap();
        let generation = engine.with(|c| {
            c.convs.get_mut("save-check").unwrap().title = "changed during the write".into();
            c.dirty_transcripts.lock().unwrap().insert("save-check".into(), c.store.next_generation("save-check"));
            c.save_soon();
            c.saves.pending.lock().unwrap()["save-check"]
        });
        tokio::time::sleep(TRANSCRIPT_SAVE_EVERY + Duration::from_millis(100)).await;
        let pending = engine.with(|c| c.saves.pending.lock().unwrap()["save-check"]);
        release.send(()).unwrap();
        engine.shutdown();
        assert_eq!(pending, generation, "a second timer must not allocate another full record while the first waits");
        assert_eq!(Store::new(root.path()).get("save-check").unwrap().title, "changed during the write");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn timer_retries_during_slow_shared_writes_settle_without_new_activity() {
        struct TickDisk(Sender<()>);
        impl Disk for TickDisk {
            fn write(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
                let _ = self.0.send(());
                std::thread::sleep(TRANSCRIPT_SAVE_EVERY * 4 + Duration::from_millis(100));
                RealDisk.write(path, bytes)
            }
            fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
                RealDisk.rename(from, to)
            }
        }
        let root = crate::workspace::TestRoot::new();
        let engine = engine(root.path(), Arc::new(|_| {}));
        let (entered, writing) = channel();
        engine.with(|c| {
            c.store = Arc::new(Store::with_disk(root.path(), Arc::new(TickDisk(entered))));
            c.convs.insert("save-check".into(), conversation());
        });
        let saving = engine.clone();
        let (done, finished) = channel();
        let first = std::thread::spawn(move || {
            saving.rename("save-check", "in flight");
            done.send(()).unwrap();
        });
        writing.recv_timeout(Duration::from_secs(10)).unwrap();
        engine.rename("save-check", "queued");
        let generation = engine.with(|c| {
            c.convs.get_mut("save-check").unwrap().title = "last activity".into();
            c.save("save-check");
            c.dirty_transcripts.lock().unwrap()["save-check"]
        });
        tokio::time::sleep(TRANSCRIPT_SAVE_EVERY * 3 + Duration::from_millis(100)).await;
        let after_ticks = engine.with(|c| c.dirty_transcripts.lock().unwrap().get("save-check").copied());
        let settled = tokio::time::timeout(Duration::from_secs(30), async {
            let mut answered = false;
            loop {
                answered |= finished.try_recv().is_ok();
                if answered && engine.with(|c| c.dirty_transcripts.lock().unwrap().is_empty()) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await;
        let writes = engine.conversation_writes();
        engine.shutdown();
        first.join().unwrap();
        assert_eq!(after_ticks, Some(generation), "three timer retries must preserve the existing dirty generation");
        assert!(settled.is_ok(), "the shared writer and dirty activity must settle before shutdown");
        assert!(writes <= 3, "idle timer retries wrote {writes} records");
        assert_eq!(Store::new(root.path()).get("save-check").unwrap().title, "last activity");
    }

    #[tokio::test]
    async fn shutdown_waits_for_a_taken_batch_and_reports_its_late_failure() {
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("data");
        let events = Arc::new(Mutex::new(Vec::new()));
        let heard = events.clone();
        let engine = engine(
            data.as_path(),
            Arc::new(move |e| {
                if let AppEvent::SaveFailed { key } = e {
                    heard.lock().unwrap().push(key)
                }
            }),
        );
        let (entered, release) = slow(&engine, data.as_path(), true);
        let batch = {
            let mut c = engine.core.lock().unwrap();
            c.saves.deferred = Some(Mutex::new(Vec::new()));
            c.save("save-check");
            c.take_saves()
        };
        let write = std::thread::spawn(move || batch.write());
        entered.recv_timeout(Duration::from_secs(10)).unwrap();
        let closing = engine.clone();
        let (done, ended) = channel();
        let close = std::thread::spawn(move || {
            closing.shutdown();
            done.send(()).unwrap();
        });
        let early = ended.recv_timeout(Duration::from_millis(100));
        release.send(()).unwrap();
        write.join().unwrap();
        ended.recv_timeout(Duration::from_secs(10)).unwrap();
        close.join().unwrap();
        assert!(matches!(early, Err(std::sync::mpsc::RecvTimeoutError::Timeout)));
        assert_eq!(*events.lock().unwrap(), ["error.chat.saveFull"]);
        assert!(Store::new(data.as_path()).get("save-check").is_some(), "shutdown retries the failed batch");
    }

    #[tokio::test]
    async fn saves_in_one_entry_publish_the_last_requested_state_before_returning() {
        let root = crate::workspace::TestRoot::new();
        let engine = engine(root.path(), Arc::new(|_| {}));
        engine.with(|c| {
            c.convs.insert("save-check".into(), conversation());
            c.save("save-check");
            c.convs.get_mut("save-check").unwrap().title = "final state".into();
            c.save("save-check");
            assert_eq!(c.listed().into_iter().find(|row| row.id == "save-check").unwrap().title, "final state");
        });
        assert_eq!(Store::new(root.path()).get("save-check").unwrap().title, "final state");
        engine.with(|c| assert!(c.dirty_transcripts.lock().unwrap().is_empty()));
        engine.shutdown();
    }

    #[tokio::test(start_paused = true)]
    async fn an_entry_replaces_queued_bytes_with_its_final_state_before_any_timer_retry() {
        let root = crate::workspace::TestRoot::new();
        let engine = engine(root.path(), Arc::new(|_| {}));
        // A has registered its bytes, but has not started its writer yet.
        let first = {
            let mut c = engine.core.lock().unwrap();
            let mut conv = conversation();
            conv.messages.push(Message::new("reply".into(), Role::Assistant, "".into(), 0));
            c.convs.insert(conv.id.clone(), conv);
            c.saves.deferred = Some(Mutex::new(Vec::new()));
            c.save("save-check");
            c.take_saves()
        };
        // B finishes while A's snapshot is still queued; even its last post-save mutation must be covered.
        let second = {
            let mut c = engine.core.lock().unwrap();
            c.saves.deferred = Some(Mutex::new(Vec::new()));
            c.save_failed.lock().unwrap().insert("save-check".into());
            c.save("save-check");
            c.fail_reply("save-check", "reply", "error.settings.saveFailed".into());
            c.convs.get_mut("save-check").unwrap().messages.last_mut().unwrap().halt = Some("reason.keepGoing.failed".into());
            let batch = c.take_saves();
            let generation = c.dirty_transcripts.lock().unwrap()["save-check"];
            assert_eq!(c.saves.pending.lock().unwrap()["save-check"], generation);
            assert_eq!(c.saves.flights.queue.lock().unwrap().pending["save-check"].generation, generation);
            assert!(c.save_failed.lock().unwrap().contains("save-check"), "replacement is not durable success");
            batch
        };
        // A publishes B's replacement before B's writer or any timer is allowed to run.
        first.write();
        let saved = Store::new(root.path()).get("save-check").unwrap();
        let reply = saved.messages.last().unwrap();
        assert_eq!(reply.status, Some(Status::Error));
        assert_eq!(reply.error.as_deref(), Some("error.settings.saveFailed"));
        assert_eq!(reply.halt.as_deref(), Some("reason.keepGoing.failed"));
        second.write();
        engine.with(|c| {
            assert!(c.dirty_transcripts.lock().unwrap().is_empty());
            assert!(c.save_failed.lock().unwrap().is_empty());
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn direct_core_saves_still_reach_disk_without_another_engine_entry() {
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("data");
        let engine = engine(data.as_path(), Arc::new(|_| {}));
        {
            let mut c = engine.core.lock().unwrap();
            c.convs.insert("save-check".into(), conversation());
            c.save("save-check");
        }
        assert!(Store::new(data.as_path()).get("save-check").is_some());
        engine.shutdown();
    }

    #[tokio::test]
    async fn an_older_success_keeps_new_dirty_activity_and_a_deleted_record_stays_deleted() {
        let root = crate::workspace::TestRoot::new();
        let engine = engine(root.path(), Arc::new(|_| {}));
        let batch = {
            let mut c = engine.core.lock().unwrap();
            c.convs.insert("save-check".into(), conversation());
            c.saves.deferred = Some(Mutex::new(Vec::new()));
            c.save("save-check");
            c.take_saves()
        };
        engine.with(|c| {
            c.dirty_transcripts.lock().unwrap().insert("save-check".into(), c.store.next_generation("save-check"));
        });
        batch.write();
        engine.with(|c| assert!(c.dirty_transcripts.lock().unwrap().contains_key("save-check")));
        let batch = {
            let mut c = engine.core.lock().unwrap();
            c.saves.deferred = Some(Mutex::new(Vec::new()));
            c.save("save-check");
            c.take_saves()
        };
        engine.with(|c| {
            c.store.remove_checked("save-check").unwrap();
            c.convs.remove("save-check");
        });
        batch.write();
        assert!(Store::new(root.path()).get("save-check").is_none());
        engine.shutdown();
    }

    #[tokio::test]
    async fn a_late_failure_is_reported_without_undoing_a_newer_success() {
        let root = crate::workspace::TestRoot::new();
        let events = Arc::new(Mutex::new(Vec::new()));
        let heard = events.clone();
        let engine = engine(
            root.path(),
            Arc::new(move |e| {
                if let AppEvent::SaveFailed { key } = e {
                    heard.lock().unwrap().push(key)
                }
            }),
        );
        engine.with(|c| {
            c.convs.insert("save-check".into(), conversation());
            let older = c.store.next_generation("save-check");
            c.try_save("save-check").unwrap();
            c.saves.flights.state.lock().unwrap().1.push(Finished {
                id: "save-check".into(),
                generation: older,
                result: Err(Fault::DiskFull),
            });
        });
        assert_eq!(*events.lock().unwrap(), ["error.chat.saveFull"]);
        engine.with(|c| assert!(!c.save_failed.lock().unwrap().contains("save-check")));
        engine.shutdown();
    }

    #[tokio::test]
    async fn deletion_retires_a_failure_paused_before_completion_delivery() {
        let root = crate::workspace::TestRoot::new();
        let events = Arc::new(Mutex::new(Vec::new()));
        let heard = events.clone();
        let engine = engine(
            root.path(),
            Arc::new(move |e| {
                if let AppEvent::SaveFailed { key } = e {
                    heard.lock().unwrap().push(key);
                }
            }),
        );
        let (entered, release) = slow(&engine, root.path(), true);
        let batch = {
            let mut c = engine.core.lock().unwrap();
            c.saves.deferred = Some(Mutex::new(Vec::new()));
            c.save("save-check");
            c.take_saves()
        };
        let (failed, failure) = channel();
        let (deliver, delivery) = channel();
        let write = std::thread::spawn(move || {
            let save = batch.flights.queue.lock().unwrap().pending.remove("save-check").unwrap();
            let result = batch.store.save_prepared(&save.id, &save.snapshot, save.generation, save.patient);
            assert_eq!(result, Err(Fault::DiskFull));
            failed.send(()).unwrap();
            delivery.recv_timeout(Duration::from_secs(10)).unwrap();
            batch.flights.state.lock().unwrap().1.push(Finished { id: save.id, generation: save.generation, result });
        });
        entered.recv_timeout(Duration::from_secs(10)).unwrap();
        release.send(()).unwrap();
        failure.recv_timeout(Duration::from_secs(10)).unwrap();
        // Store's write locks are now released, but Finished has not been queued.
        engine.with(|c| {
            c.dirty_transcripts.lock().unwrap().insert("save-check".into(), c.store.next_generation("save-check"));
            c.save_failed.lock().unwrap().insert("save-check".into());
            c.remove("save-check").unwrap();
            assert!(!c.save_failed.lock().unwrap().contains("save-check"));
            assert!(!c.saves.pending.lock().unwrap().contains_key("save-check"));
            assert!(!c.dirty_transcripts.lock().unwrap().contains_key("save-check"));
            assert!(!c.saves.completed.lock().unwrap().contains_key("save-check"));
        });
        deliver.send(()).unwrap();
        write.join().unwrap();
        engine.with(|c| {
            assert!(!c.save_failed.lock().unwrap().contains("save-check"));
            assert!(!c.saves.completed.lock().unwrap().contains_key("save-check"), "late delivery cannot recreate deleted metadata");
        });
        assert_eq!(*events.lock().unwrap(), ["error.chat.saveFull"]);
        engine.with(|c| {
            c.try_save("save-check").unwrap();
            let mut other = conversation();
            other.id = "other".into();
            c.convs.insert(other.id.clone(), other);
            assert!(c.saved_result("other", c.store.next_generation("other"), Err(Fault::Write)).is_err());
        });
        assert_eq!(*events.lock().unwrap(), ["error.chat.saveFull", "error.chat.saveFailed"]);
        assert!(Store::new(root.path()).get("save-check").is_none());
        engine.shutdown();
    }

    #[tokio::test]
    async fn deleting_saved_conversations_does_not_retain_completion_metadata() {
        let root = crate::workspace::TestRoot::new();
        let engine = engine(root.path(), Arc::new(|_| {}));
        for n in 0..32 {
            let id = format!("deleted-{n}");
            engine.with(|c| {
                let mut record = conversation();
                record.id = id.clone();
                c.convs.insert(id.clone(), record);
                c.save(&id);
            });
            engine.with(|c| {
                assert!(c.saves.completed.lock().unwrap().contains_key(&id));
                c.remove(&id).unwrap();
                assert!(c.saves.completed.lock().unwrap().is_empty(), "deleted conversation metadata must not accumulate");
            });
        }
        engine.shutdown();
    }

    #[tokio::test]
    async fn retrying_an_absent_conversation_clears_its_failure_and_pending_marks() {
        let root = crate::workspace::TestRoot::new();
        let engine = engine(root.path(), Arc::new(|_| {}));
        engine.with(|c| {
            c.save_failed.lock().unwrap().insert("absent".into());
            c.saves.pending.lock().unwrap().insert("absent".into(), 1);
            c.dirty_transcripts.lock().unwrap().insert("absent".into(), 1);
            c.try_save("absent").unwrap();
            assert!(c.save_failed.lock().unwrap().is_empty());
            assert!(c.saves.pending.lock().unwrap().is_empty());
            assert!(c.dirty_transcripts.lock().unwrap().is_empty());
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn concurrent_entries_retain_only_one_pending_snapshot_beside_the_stalled_write() {
        let root = crate::workspace::TestRoot::new();
        let engine = engine(root.path(), Arc::new(|_| {}));
        let (entered, release) = slow(&engine, root.path(), false);
        let saving = engine.clone();
        let first = std::thread::spawn(move || saving.rename("save-check", "in flight"));
        entered.recv_timeout(Duration::from_secs(10)).unwrap();
        engine.rename("save-check", "pending");
        let generation = engine.with(|c| c.saves.flights.queue.lock().unwrap().pending["save-check"].generation);
        let entries: Vec<_> = (0..32)
            .map(|n| {
                let engine = engine.clone();
                std::thread::spawn(move || {
                    engine.with(|c| {
                        c.convs.get_mut("save-check").unwrap().title = format!("entry {n}");
                        c.save("save-check");
                        assert_eq!(
                            c.saves.deferred.as_ref().unwrap().lock().unwrap().len(),
                            1,
                            "only a small save request until entry exit"
                        );
                        let queue = c.saves.flights.queue.lock().unwrap();
                        assert!(queue.writing);
                        assert_eq!(queue.pending.len(), 1, "one pending snapshot plus the stalled in-flight snapshot");
                        assert!(queue.pending["save-check"].generation >= generation);
                    })
                })
            })
            .collect();
        for entry in entries {
            entry.join().unwrap();
        }
        let newest = engine.with(|c| {
            assert!(c.dirty_transcripts.lock().unwrap()["save-check"] > generation);
            assert_eq!(
                c.saves.flights.queue.lock().unwrap().pending["save-check"].generation,
                c.dirty_transcripts.lock().unwrap()["save-check"]
            );
            c.convs["save-check"].title.clone()
        });
        release.send(()).unwrap();
        first.join().unwrap();
        assert_eq!(Store::new(root.path()).get("save-check").unwrap().title, newest);
        engine.shutdown();
    }

    #[tokio::test]
    async fn a_queued_snapshot_keeps_newer_dirty_activity_and_failures_until_a_durable_flush() {
        let root = crate::workspace::TestRoot::new();
        let engine = engine(root.path(), Arc::new(|_| {}));
        let batch = {
            let mut c = engine.core.lock().unwrap();
            c.convs.insert("save-check".into(), conversation());
            c.save_failed.lock().unwrap().insert("save-check".into());
            c.saves.deferred = Some(Mutex::new(Vec::new()));
            c.save("save-check");
            c.convs.get_mut("save-check").unwrap().title = "newer in the same entry".into();
            c.save("save-check");
            assert_eq!(c.saves.deferred.as_ref().unwrap().lock().unwrap().len(), 1);
            assert!(c.dirty_transcripts.lock().unwrap().contains_key("save-check"));
            assert!(c.save_failed.lock().unwrap().contains("save-check"));
            let generation = c.dirty_transcripts.lock().unwrap()["save-check"];
            for _ in 0..3 {
                c.save_dirty(false);
            }
            assert_eq!(c.dirty_transcripts.lock().unwrap()["save-check"], generation, "entry-local retries are not new activity");
            c.take_saves()
        };
        {
            let mut c = engine.core.lock().unwrap();
            c.saves.deferred = Some(Mutex::new(Vec::new()));
            c.convs.get_mut("save-check").unwrap().title = "latest, acknowledged".into();
            c.save("save-check");
            assert_eq!(c.saves.deferred.as_ref().unwrap().lock().unwrap().len(), 1);
            assert!(c.save_failed.lock().unwrap().contains("save-check"));
            c.flush_transcript("save-check").unwrap();
            assert!(c.saves.flights.queue.lock().unwrap().pending.is_empty());
            assert!(c.save_failed.lock().unwrap().is_empty());
            assert!(c.dirty_transcripts.lock().unwrap().is_empty());
            assert_eq!(c.store.get("save-check").unwrap().title, "latest, acknowledged");
            c.saves.deferred = None;
        }
        batch.write();
        engine.shutdown();
        assert_eq!(Store::new(root.path()).get("save-check").unwrap().title, "latest, acknowledged");
    }

    #[tokio::test]
    async fn dropping_a_batch_before_its_worker_starts_reports_failure_and_shutdown_retries() {
        let root = crate::workspace::TestRoot::new();
        let engine = engine(root.path(), Arc::new(|_| {}));
        let batch = {
            let mut c = engine.core.lock().unwrap();
            c.convs.insert("save-check".into(), conversation());
            c.saves.deferred = Some(Mutex::new(Vec::new()));
            c.save("save-check");
            c.take_saves()
        };
        drop(batch);
        {
            let mut c = engine.core.lock().unwrap();
            c.finish_saves();
            assert!(c.save_failed.lock().unwrap().contains("save-check"));
            assert!(c.saves.flights.queue.lock().unwrap().pending.is_empty());
        }
        engine.shutdown();
        assert!(Store::new(root.path()).get("save-check").is_some());
    }

    #[tokio::test]
    async fn an_entry_that_panics_still_publishes_the_saves_it_requested() {
        let root = crate::workspace::TestRoot::new();
        let engine = engine(root.path(), Arc::new(|_| {}));
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            engine.with(|c| {
                c.convs.insert("save-check".into(), conversation());
                c.save("save-check");
                panic!("after requesting the save");
            })
        }));
        assert!(panic.is_err());
        assert!(Store::new(root.path()).get("save-check").is_some());
        engine.shutdown();
    }

    #[tokio::test]
    async fn a_file_reader_flushes_an_explicit_save_without_helper_activity() {
        let root = crate::workspace::TestRoot::new();
        let engine = engine(root.path(), Arc::new(|_| {}));
        engine.with(|c| {
            c.convs.insert("save-check".into(), conversation());
            c.save("save-check");
            assert!(c.dirty_transcripts.lock().unwrap().is_empty());
            c.flush_transcript("save-check").unwrap();
            assert!(c.store.get("save-check").is_some(), "export and rewind read a durable current record");
            assert!(c.saves.pending.lock().unwrap().is_empty());
        });
        engine.shutdown();
    }

    #[test]
    fn prepared_snapshots_still_reject_an_id_that_does_not_match() {
        let root = crate::workspace::TestRoot::new();
        let store = Store::new(root.path());
        let snapshot = Store::prepare_snapshot(&conversation());
        assert_eq!(store.save_prepared("different", &snapshot, 1, false), Err(Fault::Corrupt));
        assert!(store.get("save-check").is_none());
    }

    #[tokio::test]
    async fn a_panicking_disk_keeps_the_entire_batch_for_shutdown_to_retry() {
        struct PanicOnce(std::sync::atomic::AtomicBool);
        impl Disk for PanicOnce {
            fn write(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
                assert!(self.0.swap(true, std::sync::atomic::Ordering::SeqCst), "disk panic");
                RealDisk.write(path, bytes)
            }
            fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
                RealDisk.rename(from, to)
            }
        }
        let root = crate::workspace::TestRoot::new();
        let engine = engine(root.path(), Arc::new(|_| {}));
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            engine.with(|c| {
                c.store = Arc::new(Store::with_disk(root.path(), Arc::new(PanicOnce(false.into()))));
                c.convs.insert("save-check".into(), conversation());
                let mut second = conversation();
                second.id = "second".into();
                c.convs.insert(second.id.clone(), second);
                c.save("save-check");
                c.save("second");
            })
        }));
        assert!(panic.is_err());
        engine.shutdown();
        assert!(Store::new(root.path()).get("save-check").is_some());
        assert!(Store::new(root.path()).get("second").is_some());
    }

    #[tokio::test]
    #[ignore]
    async fn measure_save_lock_before_and_after() {
        let pid = sysinfo::get_current_pid().unwrap();
        let mut system = sysinfo::System::new();
        for mb in [20, 100] {
            let root = tempfile::tempdir().unwrap();
            let data = root.path().join("data");
            let engine = engine(data.as_path(), Arc::new(|_| {}));
            engine.with(|c| {
                c.store.search().stop();
                let mut record = conversation();
                record.messages.push(Message::new("big".into(), Role::User, "x".repeat(mb * 1_000_000), 0));
                c.convs.insert("save-check".into(), record);
            });
            system.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), true);
            let cpu_before = system.process(pid).unwrap().accumulated_cpu_time();
            let before = engine.with(|c| {
                let started = std::time::Instant::now();
                c.try_save("save-check").unwrap();
                started.elapsed()
            });
            system.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), true);
            let cpu_inline = system.process(pid).unwrap().accumulated_cpu_time();
            let memory_inline = system.process(pid).unwrap().memory();
            let started = std::time::Instant::now();
            let (batch, after, bytes, capacity) = {
                let mut c = engine.core.lock().unwrap();
                c.finish_saves();
                c.saves.deferred = Some(Mutex::new(Vec::new()));
                let started = std::time::Instant::now();
                c.save("save-check");
                let batch = c.take_saves();
                let hold = started.elapsed();
                let queue = c.saves.flights.queue.lock().unwrap();
                let bytes = &queue.pending["save-check"].snapshot.bytes;
                (batch, hold, bytes.len(), bytes.capacity())
            };
            batch.write();
            let total = started.elapsed();
            system.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), true);
            let process = system.process(pid).unwrap();
            eprintln!(
                "{mb} MB: lock before {before:?}, lock after {after:?}, total after {total:?}; snapshot {bytes} bytes / {capacity} capacity; CPU inline {} ms, deferred {} ms; resident before {memory_inline}, after {} bytes",
                cpu_inline - cpu_before,
                process.accumulated_cpu_time() - cpu_inline,
                process.memory()
            );
            engine.shutdown();
        }
    }
}
