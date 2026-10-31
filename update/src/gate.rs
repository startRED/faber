//! Admission to an update install and setup work.

use std::sync::{Mutex, MutexGuard};

/// What decides, in one place, whether an install may begin: the CLI installs of the setup screen that are running, and
/// whether an install has already been decided. Starting one and deciding the other take the same lock, so a setup cannot
/// begin after the decision and an install cannot be decided in the middle of a setup.
#[derive(Debug, Default, PartialEq)]
pub struct Gate {
    setups: usize,
    installing: bool,
}

impl Gate {
    /// A CLI install begins; refused once an update install is decided.
    pub fn begin_setup(&mut self) -> bool {
        if self.installing {
            return false;
        }
        self.setups += 1;
        true
    }

    /// The install is decided unless a setup runs or `other_work` says something else does. From then on nothing new may
    /// start: a CLI install is refused (`begin_setup`), and `other_work` is where the engine's admission is closed.
    pub fn reserve(&mut self, other_work: impl FnOnce() -> bool) -> bool {
        if self.installing || self.setups > 0 || other_work() {
            return false;
        }
        self.installing = true;
        true
    }
}

static GATE: Mutex<Gate> = Mutex::new(Gate { setups: 0, installing: false });

pub fn gate() -> MutexGuard<'static, Gate> {
    GATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Held for the whole of a setup install (CLI downloads from the setup screen): they are work the engine does not know
/// about, and an update that ends the process in the middle of one leaves a half-extracted program and an unfinished PATH
/// change. While any is held no update installs.
pub struct SetupRunning;

impl SetupRunning {
    /// Refused with `error.update.closing` once an update install is decided.
    pub fn begin() -> Result<Self, String> {
        if gate().begin_setup() { Ok(Self) } else { Err("error.update.closing".into()) }
    }
}

impl Drop for SetupRunning {
    fn drop(&mut self) {
        let mut gate = gate();
        gate.setups = gate.setups.saturating_sub(1);
    }
}

/// Who asks for the install.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Who {
    /// Faber just opened and the update was downloaded.
    Start,
    /// The idle install, once an update is kept.
    Later,
    /// "Update ready, restart": the person's own choice. Work that runs does not hold it back: it is cut and goes on after.
    Owner,
}

/// The one admission of an install, all inside the gate's lock. `close` shuts the engine and the shell's work (None when
/// something runs). Then, still under the lock and as late as possible, an unattended install reads whether the person has
/// an unsent draft (`editing`, which the start also reads as any input since Faber opened) and whether its tried marker
/// reached the disk (`mark`): in either case it gives way and the engine reopens. The draft is read once more after the
/// marker is flushed, the last thing before the lock opens and the shutdown starts; a refusal there takes the marker back
/// (`unmark`), so a draft does not cost the update a day. The person's own restart ignores all of it.
pub fn admit<G>(
    gate: &mut Gate,
    who: Who,
    close: impl FnOnce() -> Option<G>,
    mut editing: impl FnMut() -> bool,
    mark: impl FnOnce() -> std::io::Result<()>,
    unmark: impl FnOnce(),
) -> Option<G> {
    let mut closed = None;
    gate.reserve(|| {
        let Some(guard) = close() else { return true };
        if who == Who::Owner {
            let _ = mark();
        } else if editing() || mark().is_err() {
            return true;
        } else if editing() {
            unmark();
            return true;
        }
        closed = Some(guard);
        false
    });
    closed
}

#[cfg(test)]
fn setups_running() -> bool {
    gate().setups > 0
}

#[cfg(test)]
mod tests {
    use super::{Gate, SetupRunning, Who, admit, setups_running};
    use std::cell::{Cell, RefCell};

    #[test]
    fn a_setup_install_counts_as_work_until_it_ends() {
        assert!(!setups_running());
        let first = SetupRunning::begin().unwrap();
        let second = SetupRunning::begin().unwrap();
        assert!(setups_running());
        drop(first);
        assert!(setups_running(), "one install is still running");
        drop(second);
        assert!(!setups_running());
    }

    #[test]
    fn the_install_is_decided_once_and_nothing_starts_after_it() {
        let mut gate = Gate::default();
        assert!(gate.begin_setup(), "a setup may start before the decision");
        assert!(!gate.reserve(|| false), "a setup that runs stops the install");
        gate.setups = 0;
        assert!(!gate.reserve(|| true), "work the shell or the engine reports stops it too");
        assert!(!gate.installing, "a refused decision reserves nothing");
        assert!(gate.reserve(|| false));
        assert!(!gate.begin_setup(), "no CLI install may begin once the install is decided");
        assert!(!gate.reserve(|| false), "and it is decided only once");
        assert_eq!(gate.setups, 0);
    }

    /// `admit` with the given answers; the guard is a plain string so a dropped one shows in `dropped`.
    fn admission(gate: &mut Gate, who: Who, closes: bool, editing: bool, marks: bool, seen: &RefCell<Vec<&'static str>>) -> bool {
        admission_late(gate, who, (closes, editing, false), marks, seen)
    }

    /// Like `admission`, with the draft read once more after the marker (`late`).
    fn admission_late(
        gate: &mut Gate,
        who: Who,
        (closes, editing, late): (bool, bool, bool),
        marks: bool,
        seen: &RefCell<Vec<&'static str>>,
    ) -> bool {
        let reads = Cell::new(0);
        admit(
            gate,
            who,
            || {
                seen.borrow_mut().push("close");
                closes.then_some("closed")
            },
            || {
                seen.borrow_mut().push("read draft");
                reads.set(reads.get() + 1);
                if reads.get() == 1 { editing } else { late }
            },
            || {
                seen.borrow_mut().push("mark");
                if marks { Ok(()) } else { Err(std::io::Error::other("disk")) }
            },
            || seen.borrow_mut().push("unmark"),
        )
        .is_some()
    }

    #[test]
    fn an_unattended_install_gives_way_to_an_unsent_draft_and_the_owner_does_not() {
        for who in [Who::Start, Who::Later] {
            let (mut gate, seen) = (Gate::default(), RefCell::new(vec![]));
            assert!(!admission(&mut gate, who, true, true, true, &seen), "{who:?}");
            assert!(!gate.installing, "a refused install decides nothing, so the window and the draft stay");
            assert!(admission(&mut gate, who, true, false, true, &seen), "{who:?}: an empty draft does not stop it");
        }
        let (mut gate, seen) = (Gate::default(), RefCell::new(vec![]));
        assert!(admission(&mut gate, Who::Owner, true, true, true, &seen), "Update ready, restart is the person's own choice");
    }

    #[test]
    fn the_draft_is_read_inside_the_decision_after_the_engine_closed() {
        let (mut gate, seen) = (Gate::default(), RefCell::new(vec![]));
        assert!(admission(&mut gate, Who::Later, true, false, true, &seen));
        assert_eq!(
            *seen.borrow(),
            ["close", "read draft", "mark", "read draft"],
            "the draft is read again after the marker, as the last thing under the lock"
        );
        let (mut busy, seen) = (Gate::default(), RefCell::new(vec![]));
        assert!(!admission(&mut busy, Who::Later, false, false, true, &seen));
        assert_eq!(*seen.borrow(), ["close"], "work that runs stops it before the draft or the marker are touched");
    }

    #[test]
    fn input_during_the_marker_sync_is_caught_by_the_second_read_and_takes_the_marker_back() {
        for who in [Who::Start, Who::Later] {
            let (mut gate, seen) = (Gate::default(), RefCell::new(vec![]));
            assert!(!admission_late(&mut gate, who, (true, false, true), true, &seen), "{who:?}");
            assert!(!gate.installing, "nothing is decided, so nothing shuts down");
            assert_eq!(*seen.borrow(), ["close", "read draft", "mark", "read draft", "unmark"], "{who:?}");
        }
        let (mut gate, seen) = (Gate::default(), RefCell::new(vec![]));
        assert!(admission_late(&mut gate, Who::Owner, (true, true, true), true, &seen), "the person's own restart still goes ahead");
    }

    #[test]
    fn a_marker_that_cannot_be_written_stops_every_unattended_install_before_anything_closes() {
        for who in [Who::Start, Who::Later] {
            let (mut gate, seen) = (Gate::default(), RefCell::new(vec![]));
            assert!(!admission(&mut gate, who, true, false, false, &seen), "{who:?}");
            assert!(!gate.installing, "nothing is decided, so nothing is shut down and no restart loop can begin");
        }
        let (mut gate, seen) = (Gate::default(), RefCell::new(vec![]));
        assert!(admission(&mut gate, Who::Owner, true, false, false, &seen), "the person's own restart cannot loop by itself");
    }

    #[test]
    fn a_cli_install_holds_the_admission_to_its_end_and_none_begins_after_the_decision() {
        let (mut gate, seen) = (Gate::default(), RefCell::new(vec![]));
        assert!(gate.begin_setup(), "a CLI install begins");
        assert!(!admission(&mut gate, Who::Owner, true, false, true, &seen), "even the owner's restart waits for it");
        gate.setups -= 1;
        assert!(admission(&mut gate, Who::Later, true, false, true, &seen));
        assert!(!gate.begin_setup(), "a CLI install asked for after the decision is refused");
    }
}
