use super::*;
use std::sync::{Arc, Mutex};

const BIG: usize = 300 * 1024;

struct Fixture {
    _tmp: tempfile::TempDir,
    data: PathBuf,
    target: PathBuf,
    pin: String,
}

impl Fixture {
    fn new() -> Self {
        // Unix temporary folders sit under /private/var, reached through the /var link, which no storage folder may have.
        let tmp = if cfg!(unix) { tempfile::tempdir_in(std::env::var_os("HOME").unwrap()) } else { tempfile::tempdir() }.unwrap();
        let base = fs::canonicalize(tmp.path()).unwrap();
        let base = PathBuf::from(crate::project::git::native_path(&base));
        let data = base.join("Faber");
        let tools = data.join("tools");
        let voice = tools.join("runtimes").join("voice");
        fs::create_dir_all(voice.join("instructions")).unwrap();
        fs::write(voice.join("instructions/SKILL.md"), b"voice").unwrap();
        fs::write(voice.join("source"), b"source").unwrap();
        fs::write(voice.join("model.bin"), vec![7u8; BIG]).unwrap();
        let pin_bytes = b"{\"pins\":1}";
        let pin = crate::hex(&Sha256::digest(pin_bytes));
        fs::create_dir_all(tools.join("runtime-pins")).unwrap();
        fs::write(tools.join("runtime-pins").join(&pin), pin_bytes).unwrap();
        fs::write(voice.join("interpreter.json"), &pin).unwrap();
        let notes = tools.join("plugin/skills/notes");
        fs::create_dir_all(&notes).unwrap();
        fs::write(notes.join("SKILL.md"), b"notes").unwrap();
        fs::write(tools.join("installed.json"), br#"{"notes":"c2","voice":"c1"}"#).unwrap();
        let target = base.join("Chosen").join("Faber-storage");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join(".faber-owner"), data.to_str().unwrap()).unwrap();
        Self { _tmp: tmp, data, target, pin }
    }

    fn tools(&self) -> PathBuf {
        self.data.join("tools")
    }

    fn to(&self) -> PathBuf {
        self.target.join("tools")
    }

    /// Where the store finds `id` now.
    fn live(&self, id: &str, folder: &str) -> PathBuf {
        under(&location::source_root(&self.tools(), id), folder)
    }

    /// `id` is whole where its record says and nowhere else a tool could be found.
    fn whole_once(&self, id: &str, folder: &str, moved: bool) {
        let live = self.live(id, folder);
        let (expected, other) = if moved {
            (under(&self.to(), folder), under(&self.tools(), folder))
        } else {
            (under(&self.tools(), folder), under(&self.to(), folder))
        };
        assert_eq!(live, expected, "{id} resolves to the {} folder", if moved { "new" } else { "old" });
        assert!(!other.exists(), "{id} is not also at {}", other.display());
        if id == "voice" {
            assert_eq!(fs::read(live.join("model.bin")).unwrap(), vec![7u8; BIG]);
            assert_eq!(fs::read(live.join("instructions/SKILL.md")).unwrap(), b"voice");
        } else {
            assert_eq!(fs::read(live.join("SKILL.md")).unwrap(), b"notes");
        }
        assert!(!self.target.join(STAGING).exists(), "no staging is left");
        assert!(!self.data.join(FOLDER).join(JOURNAL).exists(), "no journal is left");
    }
}

fn disk() -> Disk {
    Disk { free: Box::new(|_| Ok(u64::MAX)), sync_dir: Box::new(|_| Ok(())) }
}

fn quiet(_: Step<'_>) -> io::Result<()> {
    Ok(())
}

fn crash() -> io::Error {
    io::Error::other("crash")
}

/// A link to a folder as a person's account can make one: a junction on Windows, a symbolic link elsewhere.
fn link(to: &Path, at: &Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(to, at).unwrap();
    #[cfg(windows)]
    junction::create(to, at).unwrap();
}

/// Stops the run at `stop` of `voice`, as a crash there would.
fn crash_at(f: &Fixture, stop: &'static str) -> io::Result<Outcome> {
    let mut current = String::new();
    run(&f.data, &f.target, &disk(), &mut |step| {
        if let Step::Item { id, .. } = step {
            current = id.to_owned();
        }
        if current == "voice" && format!("{step:?}").starts_with(stop) { Err(crash()) } else { Ok(()) }
    })
}

#[test]
fn storage_move_moves_every_tool_and_sets_the_originals_aside() {
    let f = Fixture::new();
    let mut steps = Vec::new();
    let outcome = run(&f.data, &f.target, &disk(), &mut |step| {
        if !matches!(step, Step::Copied(_)) {
            steps.push(format!("{step:?}"));
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(outcome.moved, ["notes", "voice"]);
    assert!(outcome.left.is_empty() && outcome.stopped.is_none());
    assert!(outcome.bytes >= BIG as u64);
    assert!(
        steps.iter().position(|s| s.starts_with("Retired(\"voice\")")) < steps.iter().position(|s| s.starts_with("Switched(\"voice\")"))
    );
    f.whole_once("voice", "runtimes/voice", true);
    f.whole_once("notes", "plugin/skills/notes", true);
    assert_eq!(fs::read(f.to().join("runtime-pins").join(&f.pin)).unwrap(), b"{\"pins\":1}", "the runtime's pin goes with it");
    assert!(f.tools().join("runtime-pins").join(&f.pin).is_file(), "and stays where other runtimes may name it");
    assert_eq!(fs::read(f.tools().join("installed.json")).unwrap(), br#"{"notes":"c2","voice":"c1"}"#, "the ledger is untouched");
    let original = f.data.join(FOLDER).join(OLD);
    let attempt = fs::read_dir(&original).unwrap().next().unwrap().unwrap().path();
    assert_eq!(
        fs::metadata(f.to().join("runtimes/voice/source")).unwrap().modified().unwrap(),
        fs::metadata(attempt.join("runtimes/voice/source")).unwrap().modified().unwrap(),
        "files keep their times, so a runtime's bytecode stays valid"
    );
    assert!(old_bytes(&f.data).unwrap() >= BIG as u64, "the originals are kept for the person to remove");
    assert!(plan(&f.data, Some(&f.target)).unwrap().is_empty(), "nothing is left to move");
    remove_old(&f.data).unwrap();
    assert_eq!(old_bytes(&f.data), Some(0));
    f.whole_once("voice", "runtimes/voice", true);
}

#[test]
fn storage_move_interrupted_at_each_step_leaves_one_whole_location() {
    for (stop, moved) in
        [("Item", false), ("Copied", false), ("Staged", false), ("Published", false), ("Retired", false), ("Switched", true)]
    {
        let f = Fixture::new();
        let mut current = String::new();
        let result = run(&f.data, &f.target, &disk(), &mut |step| {
            if let Step::Item { id, .. } = step {
                current = id.to_owned();
            }
            let name = format!("{step:?}");
            if current == "voice" && name.starts_with(stop) { Err(crash()) } else { Ok(()) }
        });
        assert!(result.is_err(), "{stop}: the crash ends the run");
        assert!(f.data.join(FOLDER).join(JOURNAL).exists(), "{stop}: the journal stays for the next start");
        let recovered = recover(&f.data, &disk()).unwrap().expect("an interrupted move is found");
        assert_eq!(recovered.moved.contains(&"voice".to_owned()), moved, "{stop}");
        assert!(recovered.moved.contains(&"notes".to_owned()), "{stop}: the finished item stays moved");
        f.whole_once("voice", "runtimes/voice", moved);
        f.whole_once("notes", "plugin/skills/notes", true);
        assert!(recover(&f.data, &disk()).unwrap().is_none(), "{stop}: recovery runs once");
        // And a new move takes what is left.
        let again = run(&f.data, &f.target, &disk(), &mut quiet).unwrap();
        assert_eq!(again.moved, if moved { Vec::<String>::new() } else { vec!["voice".to_owned()] }, "{stop}");
        f.whole_once("voice", "runtimes/voice", true);
    }
}

#[test]
fn storage_move_retry_after_a_crash_needs_room_for_one_copy_only() {
    for stop in ["Staged", "Published"] {
        let f = Fixture::new();
        let mut current = String::new();
        let _ = run(&f.data, &f.target, &disk(), &mut |step| {
            if let Step::Item { id, .. } = step {
                current = id.to_owned();
            }
            if current == "voice" && format!("{step:?}").starts_with(stop) { Err(crash()) } else { Ok(()) }
        });
        recover(&f.data, &disk()).unwrap();
        // The drive has room for the item and the margin, less whatever lies under the destination right now.
        let need = plan(&f.data, Some(&f.target)).unwrap()[0].bytes + MARGIN + 4096;
        let watched = f.target.clone();
        let tight = Disk {
            free: Box::new(move |_| Ok(need.saturating_sub(old_tree_bytes(&watched).unwrap_or(0)))),
            sync_dir: Box::new(|_| Ok(())),
        };
        let outcome = run(&f.data, &f.target, &tight, &mut quiet).unwrap();
        assert_eq!(outcome.stopped, None, "{stop}: what the crash left never takes the room");
        assert_eq!(outcome.moved, ["voice"]);
        f.whole_once("voice", "runtimes/voice", true);
    }
}

#[test]
fn storage_move_stops_without_room_and_keeps_what_is_left() {
    let f = Fixture::new();
    let small = Disk { free: Box::new(|_| Ok(MARGIN + 100)), sync_dir: Box::new(|_| Ok(())) };
    let outcome = run(&f.data, &f.target, &small, &mut quiet).unwrap();
    assert_eq!(outcome.moved, ["notes"]);
    assert_eq!(outcome.stopped.as_deref(), Some(SPACE));
    f.whole_once("voice", "runtimes/voice", false);
}

#[test]
fn storage_move_notices_a_same_size_edit_whose_time_is_put_back() {
    let f = Fixture::new();
    let model = f.tools().join("runtimes/voice/model.bin");
    let outcome = run(&f.data, &f.target, &disk(), &mut |step| {
        if step == Step::Published("voice") {
            let before = fs::metadata(&model)?.modified()?;
            let mut bytes = fs::read(&model)?;
            bytes[BIG / 2] = 8;
            fs::write(&model, &bytes)?;
            File::options().write(true).open(&model)?.set_modified(before)?;
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(outcome.left, [Left { id: "voice".into(), reason: CHANGED.into() }]);
    let live = f.live("voice", "runtimes/voice");
    assert_eq!(live, f.tools().join("runtimes").join("voice"));
    assert_eq!(fs::read(live.join("model.bin")).unwrap()[BIG / 2], 8, "the edited original stays the truth");
    assert!(!f.to().join("runtimes/voice").exists(), "the stale copy is gone");
}

#[test]
fn storage_move_refuses_a_staged_folder_swapped_for_a_link() {
    let f = Fixture::new();
    let elsewhere = f.target.parent().unwrap().join("elsewhere");
    let staged = f.target.join(STAGING).join("voice");
    let outcome = run(&f.data, &f.target, &disk(), &mut |step| {
        if step == Step::Staged("voice") {
            // Same files, so only the link itself can give it away.
            fs::rename(&staged, &elsewhere)?;
            link(&elsewhere, &staged);
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(outcome.left, [Left { id: "voice".into(), reason: FAILED.into() }]);
    f.whole_once("voice", "runtimes/voice", false);
    assert!(elsewhere.join("model.bin").is_file(), "what the link pointed to is never deleted through it");
}

#[test]
fn storage_move_refuses_a_copy_swapped_for_a_link_after_it_was_read_back() {
    // The window between the last read and the switch: the original is already set aside when the swap happens.
    let f = Fixture::new();
    let elsewhere = f.target.parent().unwrap().join("elsewhere");
    let published = f.to().join("runtimes/voice");
    let outcome = run(&f.data, &f.target, &disk(), &mut |step| {
        if step == Step::Retired("voice") {
            fs::rename(&published, &elsewhere)?;
            link(&elsewhere, &published);
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(outcome.left, [Left { id: "voice".into(), reason: FAILED.into() }]);
    f.whole_once("voice", "runtimes/voice", false);
    assert!(elsewhere.join("model.bin").is_file());
}

#[test]
fn storage_move_recovery_keeps_a_switched_copy_whose_drive_cannot_be_confirmed() {
    // Cut right after the switch; at the next start the chosen folder's owner mark cannot be read (a drive half there).
    let f = Fixture::new();
    assert!(crash_at(&f, "Switched").is_err());
    let owner = f.target.join(".faber-owner");
    let mark = fs::read(&owner).unwrap();
    fs::write(&owner, b"unreadable now").unwrap();
    let recovered = recover(&f.data, &disk()).unwrap().unwrap();
    assert!(recovered.moved.contains(&"voice".to_owned()), "the record names the new folder: the move happened");
    assert!(f.to().join("runtimes/voice/model.bin").is_file(), "the copy the record names is never deleted");
    fs::write(&owner, mark).unwrap();
    assert_eq!(f.live("voice", "runtimes/voice"), f.to().join("runtimes/voice"));
    assert!(!f.tools().join("runtimes/voice").exists(), "and the original stays set aside");
    assert!(!f.data.join(FOLDER).join(JOURNAL).exists());
}

#[test]
fn storage_move_deletes_an_unswitched_copy_once_its_drive_is_back() {
    let f = Fixture::new();
    assert!(crash_at(&f, "Published").is_err());
    // The next start finds the chosen drive away: the original is the truth, and the copy waits.
    let chosen = f.target.parent().unwrap().to_path_buf();
    let away = chosen.with_file_name("Away");
    fs::rename(&chosen, &away).unwrap();
    recover(&f.data, &disk()).unwrap().unwrap();
    assert_eq!(f.live("voice", "runtimes/voice"), f.tools().join("runtimes/voice"));
    assert!(f.data.join(FOLDER).join(LEFTOVERS).is_file(), "the copy it could not reach is remembered");
    // Back again: a start with nothing to recover still deletes it, and the tool can move.
    fs::rename(&away, &chosen).unwrap();
    assert!(recover(&f.data, &disk()).unwrap().is_none());
    assert!(!f.to().join("runtimes/voice").exists(), "the copy is gone");
    assert!(!f.data.join(FOLDER).join(LEFTOVERS).exists());
    assert_eq!(run(&f.data, &f.target, &disk(), &mut quiet).unwrap().moved, ["voice"]);
    f.whole_once("voice", "runtimes/voice", true);
}

#[test]
fn storage_move_never_deletes_what_took_its_place_meanwhile() {
    let f = Fixture::new();
    let theirs = f.to().join("runtimes/voice");
    let outcome = run(&f.data, &f.target, &disk(), &mut |step| {
        if step == Step::Staged("voice") {
            fs::create_dir_all(&theirs)?;
            fs::write(theirs.join("theirs.txt"), b"not Faber's")?;
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(outcome.left, [Left { id: "voice".into(), reason: FAILED.into() }]);
    assert_eq!(fs::read(theirs.join("theirs.txt")).unwrap(), b"not Faber's");
    assert_eq!(f.live("voice", "runtimes/voice"), f.tools().join("runtimes/voice"));
    assert!(f.tools().join("runtimes/voice/model.bin").is_file());
}

#[test]
fn storage_move_flushes_every_folder_it_makes() {
    let f = Fixture::new();
    let flushes = Arc::new(Mutex::new(Vec::new()));
    let seen = flushes.clone();
    let watched = Disk {
        free: Box::new(|_| Ok(u64::MAX)),
        sync_dir: Box::new(move |path| {
            seen.lock().unwrap().push(path.to_path_buf());
            Ok(())
        }),
    };
    run(&f.data, &f.target, &watched, &mut quiet).unwrap();
    let flushed = flushes.lock().unwrap().clone();
    for folder in [
        f.target.join(STAGING).join("voice").join("instructions"),
        f.target.join(STAGING).join("voice"),
        f.to().join("runtimes"),
        f.data.join(FOLDER).join(OLD),
        f.tools(),
        f.tools().join("locations"),
    ] {
        assert!(flushed.contains(&folder), "{} is flushed", folder.display());
    }
}

#[test]
fn storage_move_takes_back_a_switch_whose_folder_flush_fails() {
    let f = Fixture::new();
    let locations = f.tools().join("locations");
    let flushes = Arc::new(Mutex::new(Vec::new()));
    let seen = flushes.clone();
    let failures = Arc::new(Mutex::new(1));
    let left = failures.clone();
    let failing = Disk {
        free: Box::new(|_| Ok(u64::MAX)),
        sync_dir: Box::new(move |path| {
            seen.lock().unwrap().push(path.to_path_buf());
            let mut left = left.lock().unwrap();
            if path == locations && *left > 0 {
                *left -= 1;
                return Err(io::Error::other("flush failed"));
            }
            Ok(())
        }),
    };
    // One failed flush: the first switch is taken back and that tool stays; the next one moves.
    let outcome = run(&f.data, &f.target, &failing, &mut quiet).unwrap();
    assert_eq!(outcome.moved, ["voice"], "a switch that may not survive a power cut is no switch");
    assert_eq!(outcome.left, [Left { id: "notes".into(), reason: FAILED.into() }]);
    f.whole_once("notes", "plugin/skills/notes", false);
    f.whole_once("voice", "runtimes/voice", true);
    assert!(flushes.lock().unwrap().contains(&f.tools().join("locations")), "the record's folder is flushed");
    // Flushes that keep failing: even the record's removal cannot be made durable, so nothing more is touched, and the
    // next start decides by the record as the disk has it.
    let f = Fixture::new();
    let locations = f.tools().join("locations");
    let broken = Disk {
        free: Box::new(|_| Ok(u64::MAX)),
        sync_dir: Box::new(move |path| if path == locations { Err(io::Error::other("flush failed")) } else { Ok(()) }),
    };
    assert!(run(&f.data, &f.target, &broken, &mut quiet).is_err());
    assert!(f.data.join(FOLDER).join(JOURNAL).exists(), "the journal stays");
    recover(&f.data, &disk()).unwrap().unwrap();
    f.whole_once("notes", "plugin/skills/notes", false);
    f.whole_once("voice", "runtimes/voice", false);
}

#[test]
fn storage_move_counts_a_removal_cut_halfway_as_kept() {
    let f = Fixture::new();
    run(&f.data, &f.target, &disk(), &mut quiet).unwrap();
    // A removal cut after its rename: what is left still shows, and the next removal finishes it.
    let cut = f.data.join(FOLDER).join(format!("{REMOVING}cut"));
    fs::rename(f.data.join(FOLDER).join(OLD), &cut).unwrap();
    assert!(old_bytes(&f.data).unwrap() >= BIG as u64);
    remove_old(&f.data).unwrap();
    assert_eq!(old_bytes(&f.data), Some(0));
    assert!(!cut.exists());
}

#[test]
fn storage_move_removal_needs_no_store_lock_and_never_reaches_a_live_tool() {
    let f = Fixture::new();
    run(&f.data, &f.target, &disk(), &mut quiet).unwrap();
    // The originals are out of every folder the store, a kit or a session resolves a tool from.
    for folder in ["runtimes/voice", "plugin/skills/notes"] {
        assert!(!under(&f.tools(), folder).exists(), "{folder} is set aside, not left where a tool is looked for");
    }
    let _held = super::super::lock();
    let (done, finished) = std::sync::mpsc::channel();
    let data = f.data.clone();
    std::thread::spawn(move || done.send(remove_old(&data).is_ok()).unwrap());
    assert_eq!(finished.recv_timeout(std::time::Duration::from_secs(20)), Ok(true), "removal never waits for the store lock");
    f.whole_once("voice", "runtimes/voice", true);
}

#[test]
fn storage_move_plans_only_whole_items_of_the_main_folder() {
    let f = Fixture::new();
    fs::create_dir_all(f.tools().join("replacements/notes")).unwrap();
    assert_eq!(
        plan(&f.data, Some(&f.target)).unwrap().iter().map(|item| item.id.as_str()).collect::<Vec<_>>(),
        ["voice"],
        "an update half done stays"
    );
    fs::remove_dir_all(f.tools().join("replacements")).unwrap();
    fs::create_dir_all(f.to().join("runtimes/voice")).unwrap();
    assert_eq!(
        plan(&f.data, Some(&f.target)).unwrap().iter().map(|item| item.id.as_str()).collect::<Vec<_>>(),
        ["notes"],
        "a folder already there is never replaced"
    );
    let outcome = run(&f.data, &f.target, &disk(), &mut quiet).unwrap();
    assert_eq!(outcome.moved, ["notes"]);
    assert!(f.to().join("runtimes/voice").is_dir(), "and never deleted");
    f.whole_once("notes", "plugin/skills/notes", true);
}

#[test]
fn storage_move_refuses_a_folder_this_profile_does_not_own() {
    let f = Fixture::new();
    fs::write(f.target.join(".faber-owner"), b"someone else").unwrap();
    let outcome = run(&f.data, &f.target, &disk(), &mut quiet).unwrap();
    assert_eq!(outcome.stopped.as_deref(), Some(UNREACHABLE));
    assert!(outcome.moved.is_empty());
    f.whole_once("voice", "runtimes/voice", false);
}

#[test]
fn storage_move_recovery_without_its_journal_still_puts_originals_back() {
    let f = Fixture::new();
    let _ = run(&f.data, &f.target, &disk(), &mut |step| if step == Step::Retired("voice") { Err(crash()) } else { Ok(()) });
    fs::write(f.data.join(FOLDER).join(JOURNAL), b"{").unwrap();
    recover(&f.data, &disk()).unwrap().unwrap();
    assert_eq!(f.live("voice", "runtimes/voice"), f.tools().join("runtimes/voice"));
    assert!(f.tools().join("runtimes/voice/model.bin").is_file(), "the original is back where its record says");
    assert!(!f.data.join(FOLDER).join(JOURNAL).exists());
}

#[test]
fn storage_move_recovery_deletes_and_puts_back_nothing_through_a_link_or_a_strange_journal() {
    // A crash left an unswitched copy; then the destination's `runtimes` became a link to a folder outside it.
    let f = Fixture::new();
    assert!(crash_at(&f, "Published").is_err());
    let runtimes = f.to().join("runtimes");
    let outside = f.target.parent().unwrap().join("outside-runtimes");
    fs::rename(&runtimes, &outside).unwrap();
    link(&outside, &runtimes);
    recover(&f.data, &disk()).unwrap().unwrap();
    assert!(outside.join("voice/model.bin").is_file(), "nothing is deleted through the link");
    assert_eq!(f.live("voice", "runtimes/voice"), f.tools().join("runtimes/voice"));
    // A journal whose attempt leaves `old` never moves a folder from elsewhere into the store.
    let f = Fixture::new();
    let foreign = f.data.parent().unwrap().join("Outside/runtimes/voice");
    fs::create_dir_all(&foreign).unwrap();
    fs::write(foreign.join("theirs.txt"), b"not Faber's").unwrap();
    fs::remove_dir_all(f.tools().join("runtimes/voice")).unwrap();
    let journal = serde_json::json!({ "attempt": "../../../Outside", "target": f.to(), "items": [] });
    // `old` exists, so the path through it resolves on every system.
    fs::create_dir_all(f.data.join(FOLDER).join(OLD)).unwrap();
    fs::write(f.data.join(FOLDER).join(JOURNAL), serde_json::to_vec(&journal).unwrap()).unwrap();
    recover(&f.data, &disk()).unwrap().unwrap();
    assert!(foreign.join("theirs.txt").is_file());
    assert!(!f.tools().join("runtimes/voice").exists());
}

#[test]
fn storage_move_recovery_never_deletes_a_folder_it_did_not_publish() {
    // Something took the place during the copy, and the move was cut before publishing: the next start leaves it alone.
    let f = Fixture::new();
    let theirs = f.to().join("runtimes/voice");
    let mut current = String::new();
    let result = run(&f.data, &f.target, &disk(), &mut |step| {
        if let Step::Item { id, .. } = step {
            current = id.to_owned();
        }
        if current == "voice" && step == Step::Staged("voice") {
            fs::create_dir_all(&theirs)?;
            fs::write(theirs.join("theirs.txt"), b"not Faber's")?;
            return Err(crash());
        }
        Ok(())
    });
    assert!(result.is_err());
    recover(&f.data, &disk()).unwrap().unwrap();
    assert_eq!(fs::read(theirs.join("theirs.txt")).unwrap(), b"not Faber's");
    assert_eq!(f.live("voice", "runtimes/voice"), f.tools().join("runtimes/voice"));
}
