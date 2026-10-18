//! RAM admission (the verifier spec's part 4): a Cargo build starts when the memory it is expected to take fits in what
//! the machine has free, so two builds run together only when both fit. A build whose peak is not known yet runs alone,
//! the way every build did before. Idle it holds nothing: a waiting build reads the free memory every `EVERY`.

use std::sync::Mutex;
use std::time::Duration;
use tokio::sync::Notify;

/// What stays free for the person, past every admitted build.
pub const RESERVE: u64 = 2 << 30;

/// How often a waiting build reads the free memory again, besides when a build ends.
const EVERY: Duration = Duration::from_secs(2);

/// The free memory now, in bytes, or none when it cannot be told.
pub type Free = std::sync::Arc<dyn Fn() -> Option<u64> + Send + Sync>;

/// The machine's available memory, the way the system reports it.
pub fn free_memory() -> Option<u64> {
    let mut system = sysinfo::System::new();
    system.refresh_memory();
    Some(system.available_memory()).filter(|&m| m > 0)
}

#[derive(Default)]
struct Held {
    builds: usize,
    /// Builds running with no known peak: while one runs, no other starts.
    unknown: usize,
    reserved: u64,
}

#[derive(Default)]
pub struct Admission {
    held: Mutex<Held>,
    ended: Notify,
}

/// A running build's place, given back when it is dropped.
pub struct Admitted<'a> {
    admission: &'a Admission,
    need: Option<u64>,
}

impl Admission {
    /// Waits until a build expected to take `need` bytes at most fits.
    pub async fn admit(&self, need: Option<u64>, free: &Free) -> Admitted<'_> {
        loop {
            let ended = self.ended.notified();
            {
                let mut held = self.held.lock().unwrap_or_else(|e| e.into_inner());
                // ponytail: the free memory already counts what the running builds use now, so their reserve is counted
                // twice while they run; conservative (less overlap, never more). A per-build reading would refine it.
                let fits = held.builds == 0
                    || held.unknown == 0 && need.zip(free()).is_some_and(|(need, free)| held.reserved + need + RESERVE <= free);
                if fits {
                    held.builds += 1;
                    match need {
                        Some(need) => held.reserved += need,
                        None => held.unknown += 1,
                    }
                    return Admitted { admission: self, need };
                }
            }
            tokio::select! {
                () = ended => {}
                () = tokio::time::sleep(EVERY) => {}
            }
        }
    }
}

impl Drop for Admitted<'_> {
    fn drop(&mut self) {
        let mut held = self.admission.held.lock().unwrap_or_else(|e| e.into_inner());
        held.builds -= 1;
        match self.need {
            Some(need) => held.reserved -= need,
            None => held.unknown -= 1,
        }
        self.admission.ended.notify_waiters();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    const GB: u64 = 1 << 30;

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread().enable_time().start_paused(true).build().unwrap()
    }

    #[test]
    fn builds_run_together_only_when_their_known_peaks_fit_and_an_unknown_one_runs_alone() {
        rt().block_on(async {
            let a = Admission::default();
            let soon = |need, free: Free| {
                let a = &a;
                async move { tokio::time::timeout(Duration::from_secs(10), a.admit(need, &free)).await.ok() }
            };
            let free: Free = Arc::new(|| Some(10 * GB));
            let first = soon(Some(3 * GB), free.clone()).await.expect("the first always starts");
            let second = soon(Some(4 * GB), free.clone()).await.expect("3 + 4 + the reserve fit in 10");
            assert!(soon(Some(2 * GB), free.clone()).await.is_none(), "3 + 4 + 2 + the reserve do not");
            drop((first, second));
            let unknown = soon(None, free.clone()).await.expect("alone it starts");
            assert!(soon(Some(GB), free.clone()).await.is_none(), "nothing joins a build of unknown size");
            drop(unknown);
            let blind: Free = Arc::new(|| None);
            let _one = soon(Some(GB), blind.clone()).await.unwrap();
            assert!(soon(Some(GB), blind).await.is_none(), "with no reading, one at a time");
        });
    }

    #[test]
    fn a_waiting_build_starts_when_the_running_one_ends() {
        rt().block_on(async {
            let a = Arc::new(Admission::default());
            let free: Free = Arc::new(|| Some(GB));
            let first = a.admit(None, &free).await;
            let waiting = {
                let (a, free) = (a.clone(), free.clone());
                tokio::spawn(async move { drop(a.admit(Some(GB), &free).await) })
            };
            tokio::task::yield_now().await;
            assert!(!waiting.is_finished());
            drop(first);
            tokio::time::timeout(Duration::from_millis(10), waiting).await.expect("woken by the end").unwrap();
        });
    }
}
