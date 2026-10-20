//! Identity-bearing snapshots within a container Faber already owns.

use std::io;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProcessIdentity {
    pub pid: u32,
    pub created: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct ProcessMember {
    pub identity: ProcessIdentity,
    pub parent: u32,
}

#[cfg(any(unix, test))]
#[derive(Clone, Copy)]
struct ProcessEntry {
    member: ProcessMember,
    group: u32,
    running: bool,
}

/// Descendants are observed only for blocking writes, never as authority to signal a process.
#[cfg(any(unix, test))]
#[derive(Default)]
pub struct ExternalDescendants {
    observed: std::collections::HashSet<ProcessIdentity>,
}

#[cfg(any(unix, test))]
impl ExternalDescendants {
    #[cfg(unix)]
    pub fn sample(&mut self, root: ProcessIdentity, group: u32) -> io::Result<Vec<ProcessIdentity>> {
        Ok(self.observe(root, group, &platform::process_table(root, &self.observed)?))
    }

    fn observe(&mut self, root: ProcessIdentity, group: u32, entries: &[ProcessEntry]) -> Vec<ProcessIdentity> {
        let table: std::collections::HashMap<_, _> = entries.iter().map(|entry| (entry.member.identity.pid, entry)).collect();
        self.observed.retain(|identity| table.get(&identity.pid).is_some_and(|entry| entry.member.identity == *identity));
        let mut parents = self.observed.clone();
        if table.get(&root.pid).is_some_and(|entry| entry.member.identity == root) {
            parents.insert(root);
        }
        loop {
            let mut changed = false;
            for entry in entries {
                if entry.member.identity == root || parents.contains(&entry.member.identity) {
                    continue;
                }
                if table.get(&entry.member.parent).is_some_and(|parent| {
                    parents.contains(&parent.member.identity) && parent.member.identity.created <= entry.member.identity.created
                }) {
                    parents.insert(entry.member.identity);
                    self.observed.insert(entry.member.identity);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        let mut external: Vec<_> = entries
            .iter()
            .filter(|entry| entry.running && entry.group != group && self.observed.contains(&entry.member.identity))
            .map(|entry| entry.member.identity)
            .collect();
        external.sort_unstable();
        external
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::Foundation::{ERROR_MORE_DATA, FILETIME};
    use windows_sys::Win32::System::JobObjects::{
        IsProcessInJob, JOBOBJECT_BASIC_PROCESS_ID_LIST, JobObjectBasicProcessIdList, QueryInformationJobObject,
    };
    use windows_sys::Win32::System::Threading::{GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

    fn open(pid: u32, access: u32) -> io::Result<OwnedHandle> {
        let handle = unsafe { OpenProcess(access, 0, pid) };
        if handle.is_null() { Err(io::Error::last_os_error()) } else { Ok(unsafe { OwnedHandle::from_raw_handle(handle) }) }
    }

    fn identity(handle: &OwnedHandle, pid: u32) -> io::Result<ProcessIdentity> {
        let (mut created, mut exit, mut kernel, mut user) =
            (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
        if unsafe { GetProcessTimes(handle.as_raw_handle(), &mut created, &mut exit, &mut kernel, &mut user) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(ProcessIdentity { pid, created: (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime) })
    }

    fn validate(handle: &OwnedHandle, job: isize, expected: ProcessIdentity) -> io::Result<()> {
        let mut belongs = 0;
        if unsafe { IsProcessInJob(handle.as_raw_handle(), job as _, &mut belongs) } == 0 {
            return Err(io::Error::last_os_error());
        }
        if belongs == 0 || identity(handle, expected.pid)? != expected {
            return Err(io::Error::other("process identity is outside the owned job"));
        }
        Ok(())
    }

    pub fn job_process_identity(job: isize, pid: u32) -> io::Result<ProcessIdentity> {
        let process = open(pid, PROCESS_QUERY_LIMITED_INFORMATION)?;
        let stable = identity(&process, pid)?;
        validate(&process, job, stable)?;
        Ok(stable)
    }

    pub fn job_members(job: isize) -> io::Result<Vec<ProcessMember>> {
        let mut capacity = 32usize;
        let ids = loop {
            if capacity > 1_048_576 {
                return Err(io::Error::other("owned job membership is too large"));
            }
            let mut buffer = vec![0usize; capacity + 2];
            let list = buffer.as_mut_ptr().cast::<JOBOBJECT_BASIC_PROCESS_ID_LIST>();
            let bytes = u32::try_from(buffer.len() * std::mem::size_of::<usize>()).map_err(io::Error::other)?;
            let ok = unsafe { QueryInformationJobObject(job as _, JobObjectBasicProcessIdList, list.cast(), bytes, std::ptr::null_mut()) };
            if ok == 0 {
                let error = io::Error::last_os_error();
                if error.raw_os_error() == Some(ERROR_MORE_DATA as i32) {
                    capacity *= 2;
                    continue;
                }
                return Err(error);
            }
            let count = unsafe { (*list).NumberOfProcessIdsInList as usize };
            if count > capacity {
                capacity = count;
                continue;
            }
            let first = unsafe { std::ptr::addr_of!((*list).ProcessIdList).cast::<usize>() };
            break unsafe { std::slice::from_raw_parts(first, count) }.to_vec();
        };
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let snapshot = crate::processes()?;
        let mut members = Vec::with_capacity(ids.len());
        for id in ids {
            let pid = u32::try_from(id).map_err(io::Error::other)?;
            let process = open(pid, PROCESS_QUERY_LIMITED_INFORMATION)?;
            let stable = identity(&process, pid)?;
            validate(&process, job, stable)?;
            let entry = snapshot
                .iter()
                .find(|entry| entry.pid == pid && entry.created == stable.created)
                .ok_or_else(|| io::Error::other("owned job membership changed during the snapshot"))?;
            members.push(ProcessMember { identity: stable, parent: entry.parent });
        }
        Ok(members)
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::*;
    use rustix::process::Pid;

    fn read_member(pid: u32) -> io::Result<(ProcessMember, u32)> {
        parse_stat(&std::fs::read_to_string(format!("/proc/{pid}/stat"))?, pid)
    }

    pub fn group_process_identity(pgid: u32, pid: u32) -> io::Result<ProcessIdentity> {
        let (member, group) = read_member(pid)?;
        if group != pgid {
            return Err(io::Error::other("process identity is outside the owned group"));
        }
        Ok(member.identity)
    }

    pub(super) fn parse_stat(stat: &str, pid: u32) -> io::Result<(ProcessMember, u32)> {
        let end = stat.rfind(')').ok_or_else(|| io::Error::other("invalid process identity"))?;
        let fields: Vec<_> = stat[end + 1..].split_whitespace().collect();
        let number = |at: usize| {
            fields.get(at).ok_or_else(|| io::Error::other("incomplete process identity"))?.parse::<u64>().map_err(io::Error::other)
        };
        let created = number(19)?;
        let parent = u32::try_from(number(1)?).map_err(io::Error::other)?;
        let group = u32::try_from(number(2)?).map_err(io::Error::other)?;
        Ok((ProcessMember { identity: ProcessIdentity { pid, created }, parent }, group))
    }

    pub(super) fn process_table(
        _root: ProcessIdentity,
        _observed: &std::collections::HashSet<ProcessIdentity>,
    ) -> io::Result<Vec<ProcessEntry>> {
        let mut entries = Vec::new();
        for entry in std::fs::read_dir("/proc")? {
            let entry = entry?;
            let Some(pid) = entry.file_name().to_str().and_then(|name| name.parse::<u32>().ok()) else { continue };
            let stat = match std::fs::read_to_string(entry.path().join("stat")) {
                Ok(stat) => stat,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error),
            };
            let (member, group) = parse_stat(&stat, pid)?;
            let running = stat.rsplit_once(')').is_some_and(|(_, fields)| !fields.trim_start().starts_with(['Z', 'X']));
            entries.push(ProcessEntry { member, group, running });
        }
        Ok(entries)
    }

    pub fn group_members(pgid: u32) -> io::Result<Vec<ProcessMember>> {
        if crate::process_group_empty(pgid)? {
            return Ok(Vec::new());
        }
        let mut members = Vec::new();
        for entry in std::fs::read_dir("/proc")? {
            let entry = entry?;
            let Some(pid) = entry.file_name().to_str().and_then(|name| name.parse::<u32>().ok()) else { continue };
            match read_member(pid) {
                Ok((member, group)) if group == pgid => members.push(member),
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => {
                    let Some(pid) = i32::try_from(pid).ok().and_then(Pid::from_raw) else { continue };
                    match rustix::process::getpgid(Some(pid)) {
                        Ok(group) if group.as_raw_pid() as u32 != pgid => {}
                        Err(rustix::io::Errno::SRCH) => {}
                        _ => return Err(error),
                    }
                }
            }
        }
        if members.is_empty() && !crate::process_group_empty(pgid)? {
            return Err(io::Error::other("owned process group could not be enumerated"));
        }
        Ok(members)
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;

    pub fn group_process_identity(pgid: u32, pid: u32) -> io::Result<ProcessIdentity> {
        group_members(pgid)?
            .into_iter()
            .find(|member| member.identity.pid == pid)
            .map(|member| member.identity)
            .ok_or_else(|| io::Error::other("process identity is outside the owned group"))
    }

    fn process_ids(id: u32, children: bool) -> io::Result<Vec<i32>> {
        let mut pids = vec![0i32; 256];
        loop {
            let bytes = i32::try_from(pids.len() * std::mem::size_of::<i32>()).map_err(io::Error::other)?;
            let id = i32::try_from(id).map_err(io::Error::other)?;
            // libproc returns zero for both an empty list and an error; do not reuse a stale errno.
            let used = unsafe {
                *libc::__error() = 0;
                if children {
                    libc::proc_listchildpids(id, pids.as_mut_ptr().cast(), bytes)
                } else {
                    libc::proc_listpgrppids(id, pids.as_mut_ptr().cast(), bytes)
                }
            };
            let error = io::Error::last_os_error();
            if used < 0 || (used == 0 && error.raw_os_error() != Some(0)) {
                if error.raw_os_error() == Some(libc::ESRCH) {
                    return Ok(Vec::new());
                }
                return Err(error);
            }
            if used as usize >= pids.len() {
                pids.resize(pids.len() * 2, 0);
                continue;
            }
            pids.truncate(used as usize);
            return Ok(pids);
        }
    }

    fn process_entry(pid: i32) -> io::Result<Option<ProcessEntry>> {
        let mut info = std::mem::MaybeUninit::<libc::proc_bsdinfo>::zeroed();
        let bytes = std::mem::size_of::<libc::proc_bsdinfo>() as i32;
        // Include zombies so their still-live children retain the complete parent chain.
        let used = unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDTBSDINFO, 1, info.as_mut_ptr().cast(), bytes) };
        if used != bytes {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(libc::ESRCH) {
                return Ok(None);
            }
            return Err(error);
        }
        let info = unsafe { info.assume_init() };
        Ok(Some(ProcessEntry {
            member: ProcessMember {
                identity: ProcessIdentity {
                    pid: pid as u32,
                    created: info.pbi_start_tvsec.saturating_mul(1_000_000).saturating_add(info.pbi_start_tvusec),
                },
                parent: info.pbi_ppid,
            },
            group: info.pbi_pgid,
            running: info.pbi_status != libc::SZOMB,
        }))
    }

    pub(super) fn process_table(
        root: ProcessIdentity,
        observed: &std::collections::HashSet<ProcessIdentity>,
    ) -> io::Result<Vec<ProcessEntry>> {
        // Full BSD info is restricted to the same user. Walk only this tree and its retained orphans,
        // rather than making an unrelated system process's EPERM block every restore.
        let mut pending = vec![(root.pid, Some(root))];
        pending.extend(observed.iter().map(|identity| (identity.pid, Some(*identity))));
        let mut visited = std::collections::HashSet::new();
        let mut entries = Vec::new();
        while let Some((pid, expected)) = pending.pop() {
            if !visited.insert(pid) {
                continue;
            }
            let Some(entry) = process_entry(i32::try_from(pid).map_err(io::Error::other)?)? else { continue };
            if expected.is_some_and(|identity| entry.member.identity != identity) {
                continue;
            }
            for child in process_ids(pid, true)? {
                if child > 0 {
                    pending.push((child as u32, None));
                }
            }
            entries.push(entry);
        }
        Ok(entries)
    }

    pub fn group_members(pgid: u32) -> io::Result<Vec<ProcessMember>> {
        if crate::process_group_empty(pgid)? {
            return Ok(Vec::new());
        }
        let mut members = Vec::new();
        for pid in process_ids(pgid, false)? {
            if pid <= 0 {
                continue;
            }
            let Some(entry) = process_entry(pid)? else {
                // A listed member may exit and be reaped before its identity is queried.
                continue;
            };
            if entry.group != pgid {
                return Err(io::Error::other("owned group changed during the snapshot"));
            }
            if entry.running {
                members.push(entry.member);
            }
        }
        Ok(members)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn rewind_macos_exited_members_and_empty_lists_do_not_reuse_errno() {
            // Darwin's PID range cannot reach i32::MAX; no process needs to be launched or killed.
            assert!(process_entry(i32::MAX).unwrap().is_none());
            for children in [false, true] {
                unsafe { *libc::__error() = libc::EPERM };
                assert!(process_ids(i32::MAX as u32, children).unwrap().is_empty());
            }
        }
    }
}

#[cfg(unix)]
pub use platform::{group_members, group_process_identity};
#[cfg(windows)]
pub use platform::{job_members, job_process_identity};

#[cfg(test)]
mod tests {
    use super::*;

    fn process(pid: u32, parent: u32, created: u64, group: u32) -> ProcessEntry {
        ProcessEntry { member: ProcessMember { identity: ProcessIdentity { pid, created }, parent }, group, running: true }
    }

    #[test]
    fn an_exited_command_does_not_release_its_observed_child_in_another_group() {
        let root = process(10, 1, 1, 10);
        let command = process(20, 10, 2, 20);
        let child = process(30, 20, 3, 20);
        let mut tracked = ExternalDescendants::default();
        assert_eq!(
            tracked.observe(root.member.identity, 10, &[child, root, command]),
            vec![command.member.identity, child.member.identity]
        );
        let reparented = process(30, 1, 3, 20);
        assert_eq!(tracked.observe(root.member.identity, 10, &[root, reparented]), vec![child.member.identity]);
        assert!(tracked.observe(root.member.identity, 10, &[root]).is_empty());
    }

    #[test]
    fn observed_children_remain_known_after_group_and_parent_changes_but_not_pid_reuse() {
        let root = process(10, 1, 1, 10);
        let child = process(20, 10, 2, 10);
        let mut tracked = ExternalDescendants::default();
        assert!(tracked.observe(root.member.identity, 10, &[root, child]).is_empty());
        let moved = process(20, 1, 2, 20);
        assert_eq!(tracked.observe(root.member.identity, 10, &[moved]), vec![child.member.identity]);
        assert!(tracked.observe(root.member.identity, 10, &[process(20, 1, 4, 20)]).is_empty());
        assert!(tracked.observe(root.member.identity, 10, &[process(10, 1, 5, 10), process(30, 10, 6, 30)]).is_empty());
    }

    #[test]
    fn zombies_do_not_block_but_their_live_children_do() {
        let root = process(10, 1, 1, 10);
        let mut command = process(20, 10, 2, 20);
        command.running = false;
        let child = process(30, 20, 3, 20);
        let mut tracked = ExternalDescendants::default();
        assert_eq!(tracked.observe(root.member.identity, 10, &[root, command, child]), vec![child.member.identity]);
        let mut zombie = process(30, 1, 3, 20);
        zombie.running = false;
        assert!(tracked.observe(root.member.identity, 10, &[root, zombie]).is_empty());
        assert!(tracked.observe(root.member.identity, 10, &[root, process(40, 10, 0, 40)]).is_empty());
    }
}
