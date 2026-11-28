//! How long each part of a start took, for `startup.log`: the process that starts the engine calls [`begin`] first, each
//! part calls [`mark`] when it ends, and [`line`] gives them as `name=12ms`. Without [`begin`] (tests, other hosts)
//! nothing is kept.

use std::sync::{Mutex, OnceLock};
use std::time::Instant;

/// More than any start has; a mark past it is dropped, so a part called again and again never grows the list.
const MAX_MARKS: usize = 48;

static BEGIN: OnceLock<Instant> = OnceLock::new();
static MARKS: Mutex<Vec<(&'static str, u128)>> = Mutex::new(Vec::new());

/// The start begins now.
pub fn begin() {
    BEGIN.get_or_init(Instant::now);
}

/// `stage` ended now.
pub fn mark(stage: &'static str) {
    let Some(begun) = BEGIN.get() else { return };
    let mut marks = MARKS.lock().unwrap_or_else(|e| e.into_inner());
    if marks.len() < MAX_MARKS {
        marks.push((stage, begun.elapsed().as_millis()));
    }
}

/// The marks so far, `name=12ms` each, in the order they ended; they are forgotten, so the next line has only newer ones.
pub fn line() -> String {
    joined(&std::mem::take(&mut *MARKS.lock().unwrap_or_else(|e| e.into_inner())))
}

fn joined(marks: &[(&'static str, u128)]) -> String {
    marks.iter().map(|(stage, ms)| format!("{stage}={ms}ms")).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    // The marks themselves are global to the process, which other tests share: only their line is checked here.
    #[test]
    fn marks_read_as_name_and_milliseconds_in_order() {
        assert_eq!(super::joined(&[("engine", 28), ("started", 29)]), "engine=28ms started=29ms");
        assert_eq!(super::joined(&[]), "");
    }
}
