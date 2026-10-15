//! The processes of a launched app, for the audit and for memory numbers: who is in its tree, which ones are still running,
//! and (on Windows) how much memory they use. Nothing here ends a process: a process id is not a safe handle (the system
//! hands it to the next program that starts), so ending is done through what the runner owns (`contain`).

/// One process, identified by its id *and* the moment it was created. An id alone is not an identity: the system reuses
/// ids, and on Windows it can do so within the same second.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcRow {
    pub pid: u32,
    pub parent: u32,
    pub name: String,
    /// Creation time in microseconds since 1970. Windows reads it from the system's own creation date, which is finer
    /// than a second. macOS and Linux only report whole seconds, so there it is that many seconds times a million.
    pub born: u64,
}

/// The app and everything it started. A child is born after its parent, or at the same moment: the system reuses process
/// ids, so a process whose parent id now belongs to a younger process (csrss.exe and winlogon.exe outlive the smss.exe
/// that started them) is not part of the tree. A pure function over the list.
pub fn descendants(rows: &[ProcRow], pid: u32) -> Vec<ProcRow> {
    let mut born: std::collections::BTreeMap<u32, u64> =
        std::collections::BTreeMap::from([(pid, rows.iter().find(|r| r.pid == pid).map_or(0, |r| r.born))]);
    let mut changed = true;
    while changed {
        changed = false;
        for row in rows {
            if let Some(&parent) = born.get(&row.parent)
                && !born.contains_key(&row.pid)
                && row.born >= parent
            {
                born.insert(row.pid, row.born);
                changed = true;
            }
        }
    }
    rows.iter().filter(|r| born.contains_key(&r.pid)).cloned().collect()
}

/// The tree of a process that is running now: empty when `pid` is not in the list, or is there without a creation time.
/// A root with no creation time to compare with would take in everything that names its id as its parent. When that id
/// once belonged to a program that is long gone, those are csrss.exe, services.exe, explorer.exe and the rest of the
/// system, and the cleanup would try to end them.
pub fn live_tree(rows: &[ProcRow], pid: u32) -> Vec<ProcRow> {
    if rows.iter().any(|r| r.pid == pid && r.born > 0) { descendants(rows, pid) } else { Vec::new() }
}

/// What the app started that is not in its job: in the tree of `root` as the first list shows it, still the same process in
/// the later list (so it did not simply end in between), and not among the ids the job named after the first list was
/// taken. The runner never ends these; the audit fails on them.
pub fn escaped(first: &[ProcRow], later: &[ProcRow], root: u32, members: &[u32]) -> Vec<ProcRow> {
    let alive = |r: &ProcRow| same_process(r, later);
    live_tree(first, root).into_iter().filter(|r| alive(r) && !members.contains(&r.pid)).collect()
}

/// Whether `row` is one of the processes recorded earlier: the same id, the same creation time and the same name. On
/// macOS and Linux the creation time is a whole second, so the name is a second witness against a reused id.
pub fn same_process(row: &ProcRow, seen: &[ProcRow]) -> bool {
    seen.iter().any(|old| old.pid == row.pid && old.born == row.born && old.name == row.name)
}

/// Every process on this computer right now. Windows asks WMI (`Win32_Process`), whose creation date has microseconds.
#[cfg(windows)]
pub fn snapshot() -> Result<Vec<ProcRow>, String> {
    use serde::Deserialize;
    #[derive(Deserialize)]
    #[serde(rename = "Win32_Process", rename_all = "PascalCase")]
    struct Row {
        process_id: u32,
        parent_process_id: u32,
        name: String,
        creation_date: Option<wmi::WMIDateTime>,
    }
    let wmi = wmi::WMIConnection::new().map_err(|e| format!("WMI: {e}"))?;
    let rows = wmi.query::<Row>().map_err(|e| format!("WMI process query: {e}"))?;
    Ok(rows
        .into_iter()
        .map(|r| ProcRow {
            pid: r.process_id,
            parent: r.parent_process_id,
            name: r.name,
            // The idle process has no creation date; 0 sorts before everything, as it should.
            born: r.creation_date.map_or(0, |d| u64::try_from(d.0.timestamp_micros()).unwrap_or(0)),
        })
        .collect())
}

/// Every process on this computer right now, from the `sysinfo` crate (whole-second start times).
#[cfg(not(windows))]
pub fn snapshot() -> Result<Vec<ProcRow>, String> {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
    let mut system = System::new();
    system.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
    Ok(system
        .processes()
        .iter()
        .map(|(pid, p)| ProcRow {
            pid: pid.as_u32(),
            parent: p.parent().map_or(0, Pid::as_u32),
            name: p.name().to_string_lossy().into_owned(),
            born: p.start_time() * 1_000_000,
        })
        .collect())
}

/// The private working set, in MiB, of the app and its whole tree, as Task Manager counts it. Windows only: the number
/// comes from `Win32_PerfRawData_PerfProc_Process`, and only the memory scenarios of `screen` read it.
#[cfg(windows)]
pub fn private_mb(root: u32) -> Result<f64, String> {
    use serde::Deserialize;
    #[derive(Deserialize)]
    #[serde(rename = "Win32_PerfRawData_PerfProc_Process", rename_all = "PascalCase")]
    struct Perf {
        #[serde(rename = "IDProcess")]
        id_process: u32,
        working_set_private: u64,
    }
    let ids: Vec<u32> = live_tree(&snapshot()?, root).iter().map(|r| r.pid).collect();
    let wmi = wmi::WMIConnection::new().map_err(|e| format!("WMI: {e}"))?;
    let rows = wmi.query::<Perf>().map_err(|e| format!("WMI memory query: {e}"))?;
    let bytes: u64 = rows.iter().filter(|p| ids.contains(&p.id_process)).map(|p| p.working_set_private).sum();
    Ok(bytes as f64 / (1024.0 * 1024.0))
}

#[cfg(not(windows))]
pub fn private_mb(_root: u32) -> Result<f64, String> {
    Err("private working sets are read through WMI, which only Windows has".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(pid: u32, parent: u32, born: u64) -> ProcRow {
        ProcRow { pid, parent, name: format!("p{pid}"), born }
    }
    fn pids(rows: &[ProcRow]) -> Vec<u32> {
        let mut pids: Vec<u32> = rows.iter().map(|r| r.pid).collect();
        pids.sort();
        pids
    }
    // 2026-10-11 12:00:00 UTC, in microseconds.
    const SECOND: u64 = 1_791_720_000_000_000;

    #[test]
    fn the_tree_follows_children_born_after_their_parent_or_at_the_same_moment() {
        let rows = [row(10, 1, 100), row(11, 10, 110), row(12, 11, 110), row(13, 10, 130), row(99, 1, 50)];
        assert_eq!(pids(&descendants(&rows, 10)), [10, 11, 12, 13]);
    }

    // The system reuses process ids: csrss.exe still names the smss.exe that started it, though that id now belongs to a
    // process started later, so csrss.exe is not a child of it.
    #[test]
    fn a_process_older_than_the_one_that_now_owns_its_parent_id_is_not_in_the_tree() {
        let rows = [row(500, 4, 1000), row(600, 500, 2000), row(700, 500, 900)];
        assert_eq!(pids(&descendants(&rows, 500)), [500, 600]);
        let twin = [row(10, 1, 100), row(11, 10, 90)];
        assert_eq!(pids(&descendants(&twin, 10)), [10]);
    }

    #[test]
    fn a_root_that_is_gone_still_collects_what_names_it_as_parent() {
        let rows = [row(11, 10, 110), row(12, 11, 120), row(13, 7, 130)];
        assert_eq!(pids(&descendants(&rows, 10)), [11, 12]);
    }

    // The app's id can be one that a program long gone left behind as the parent of the system's own processes.
    #[test]
    fn a_root_that_is_not_running_has_no_live_tree() {
        // 5 and 6 name the id 4 as their parent, as the system's processes name a program that exited long ago.
        let rows = [row(5, 4, 10), row(6, 5, 20), row(10, 1, 100), row(11, 10, 110)];
        assert_eq!(pids(&descendants(&rows, 4)), [5, 6], "a gone root collects what names it, which `leaked` relies on");
        assert!(live_tree(&rows, 4).is_empty(), "nothing is known about a process that is not in the list");
        assert_eq!(pids(&live_tree(&rows, 10)), [10, 11]);
        let undated = [row(10, 1, 0), row(5, 10, 10)];
        assert!(live_tree(&undated, 10).is_empty(), "a root with no creation time cannot tell its children from older processes");
    }

    #[test]
    fn a_process_of_the_tree_that_the_job_does_not_hold_has_escaped_unless_it_ended_meanwhile() {
        let first = [row(10, 1, 100), row(11, 10, 110), row(12, 10, 120), row(13, 10, 130)];
        let later = [row(10, 1, 100), row(11, 10, 110), row(12, 10, 120)];
        // 13 ended between the two lists; 12 is alive and the job does not name it; 11 is in the job.
        assert_eq!(pids(&escaped(&first, &later, 10, &[10, 11])), [12]);
        assert!(escaped(&first, &later, 10, &[10, 11, 12]).is_empty());
        assert!(escaped(&first, &later, 99, &[]).is_empty(), "a root that is not running has no tree");
    }

    #[test]
    fn a_recorded_process_is_the_same_id_and_the_same_creation_time() {
        let seen = [row(10, 1, 100)];
        assert!(same_process(&row(10, 1, 100), &seen));
        assert!(!same_process(&row(10, 1, 200), &seen), "the id was reused by a younger process");
        assert!(!same_process(&row(11, 1, 100), &seen));
    }

    // Ids come back quickly on Windows: a process that died and a new one that took its id can start in the same whole
    // second. Only a finer creation time tells them apart, so the audit must use it.
    #[test]
    fn an_id_reused_within_the_same_second_is_not_the_recorded_process() {
        let recorded = [row(4242, 100, SECOND + 120_000)];
        let reused = row(4242, 100, SECOND + 870_000);
        assert_eq!(reused.born / 1_000_000, recorded[0].born / 1_000_000, "both were created in the same whole second");
        assert!(!same_process(&reused, &recorded));
        // The finest difference the system gives is 5 ms on Windows; it is enough.
        assert!(!same_process(&row(4242, 100, SECOND + 125_000), &recorded));
        assert!(same_process(&row(4242, 7, SECOND + 120_000), &recorded), "the same process, even if the parent column differs");
    }

    // macOS and Linux times are whole seconds, so an id handed out again in the same second can only be told apart by
    // the program's name.
    #[test]
    fn an_equal_id_and_second_with_a_different_name_is_a_different_process() {
        let recorded = [ProcRow { pid: 31, parent: 1, name: "faber".into(), born: SECOND }];
        assert!(same_process(&ProcRow { pid: 31, parent: 1, name: "faber".into(), born: SECOND }, &recorded));
        assert!(!same_process(&ProcRow { pid: 31, parent: 1, name: "bash".into(), born: SECOND }, &recorded));
    }

    #[test]
    fn a_parent_id_reused_within_the_same_second_does_not_pull_in_an_older_process() {
        // 500 is the app. 800 names 500 as its parent, but was created 40 ms before 500 existed: it belongs to the
        // process that had the id earlier.
        let rows = [row(500, 4, SECOND + 500_000), row(600, 500, SECOND + 510_000), row(800, 500, SECOND + 460_000)];
        assert_eq!(pids(&descendants(&rows, 500)), [500, 600]);
    }

    #[test]
    fn the_snapshot_sees_this_very_test_process_with_its_parent_and_creation_time() {
        let me = snapshot().unwrap().into_iter().find(|r| r.pid == std::process::id()).expect("this process is in the snapshot");
        assert!(me.parent != 0 && me.born > SECOND / 2 && !me.name.is_empty(), "{me:?}");
    }

    #[cfg(windows)]
    #[test]
    fn on_windows_creation_times_are_finer_than_a_second() {
        let rows = snapshot().unwrap();
        let fine = rows.iter().filter(|r| r.born % 1_000_000 != 0).count();
        assert!(fine > rows.len() / 2, "{fine} of {} processes have a creation time with microseconds", rows.len());
    }

    #[cfg(windows)]
    #[test]
    fn the_private_working_set_of_this_process_is_a_real_number() {
        let mb = private_mb(std::process::id()).unwrap();
        assert!(mb > 1.0 && mb < 4096.0, "{mb}");
    }
}
