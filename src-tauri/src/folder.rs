//! Which saved path "Open folder" may show. The path comes from a file on disk, so it is checked every time: a folder that
//! was swapped for a program, a web address, a network share and a link are all refused, and what is shown is only
//! revealed in the file manager, never opened. Nothing that could reach a network is touched before it is refused.

use std::path::{Component, Path, PathBuf, Prefix};

/// The existing local folder behind a saved project path, or the key of the sentence that says why not.
///
/// ponytail: the folder is checked here and revealed a moment later, so it can still be swapped in between. Revealing only
/// selects an item in the file manager and starts nothing, so the swap cannot run anything; no handle is held across.
pub fn local_folder(saved: &str) -> Result<PathBuf, &'static str> {
    local_folder_with(saved, &is_remote_drive)
}

/// `local_folder` with the question "is this drive a network drive?" supplied, so a test can say yes without a network.
fn local_folder_with(saved: &str, remote_drive: &dyn Fn(char) -> bool) -> Result<PathBuf, &'static str> {
    // A web address or a network share written the way Windows writes them.
    if saved.contains("://") || saved.starts_with(r"\\") || saved.starts_with("//") {
        return Err("error.folder.notLocal");
    }
    let path = Path::new(saved);
    if !path.is_absolute() || path.components().any(|c| matches!(c, Component::CurDir | Component::ParentDir)) {
        return Err("error.folder.notLocal");
    }
    // A mapped network drive is refused from its letter alone, before any call that would open the folder.
    if let Some(Component::Prefix(prefix)) = path.components().next()
        && let Prefix::Disk(letter) = prefix.kind()
        && remote_drive(letter as char)
    {
        return Err("error.folder.notLocal");
    }
    // Walking down from the top without following anything: a link or junction on the way may lead to a share, and reading
    // where it leads would already contact it, so it is refused as it is met.
    let mut walked = PathBuf::new();
    for part in path.components() {
        walked.push(part);
        if matches!(part, Component::Prefix(_) | Component::RootDir) {
            continue;
        }
        match std::fs::symlink_metadata(&walked) {
            Ok(meta) if meta.file_type().is_symlink() => return Err("error.folder.link"),
            Ok(_) => {}
            Err(_) => return Err("error.folder.missing"),
        }
    }
    // Nothing on the way is a link now; resolving only normalizes the form, and the result is checked again.
    let real = std::fs::canonicalize(path).map_err(|_| "error.folder.missing")?;
    if let Some(Component::Prefix(prefix)) = real.components().next()
        && !matches!(prefix.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_))
    {
        return Err("error.folder.notLocal");
    }
    if !real.is_dir() {
        return Err("error.folder.missing");
    }
    Ok(strip_verbatim(real))
}

/// Whether Windows says this drive letter is a network drive. Other systems have no drive letters.
#[cfg(windows)]
fn is_remote_drive(letter: char) -> bool {
    winsafe::GetDriveType(Some(&format!("{letter}:\\"))) == winsafe::co::DRIVE::REMOTE
}

#[cfg(not(windows))]
fn is_remote_drive(_: char) -> bool {
    false
}

/// `\\?\C:\work` is how Windows resolves `C:\work`; the file manager wants the short form.
fn strip_verbatim(path: PathBuf) -> PathBuf {
    match path.to_str().and_then(|s| s.strip_prefix(r"\\?\")) {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => PathBuf::from(rest),
        _ => path,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_real_local_folder_is_shown() {
        let dir = tempfile::tempdir().unwrap();
        let shown = local_folder(dir.path().to_str().unwrap()).unwrap();
        assert!(shown.is_dir());
        assert!(!shown.to_string_lossy().starts_with(r"\\?\"), "{shown:?}");
    }

    #[test]
    fn a_folder_replaced_by_a_program_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let swapped = dir.path().join("demo.exe");
        std::fs::write(&swapped, b"MZ").unwrap();
        assert_eq!(local_folder(swapped.to_str().unwrap()), Err("error.folder.missing"));
    }

    #[test]
    fn a_folder_that_is_gone_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(local_folder(dir.path().join("gone").to_str().unwrap()), Err("error.folder.missing"));
    }

    #[test]
    fn addresses_shares_and_relative_paths_are_refused() {
        for saved in [
            "https://example.com/demo",
            "file:///C:/Windows",
            r"\\server\share\demo",
            r"\\?\UNC\server\share\demo",
            "//server/share/demo",
            "demo",
            r"..\demo",
            "",
        ] {
            assert_eq!(local_folder(saved), Err("error.folder.notLocal"), "{saved:?}");
        }
    }

    #[test]
    fn a_path_with_dot_parts_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let sneaky = dir.path().join("a").join("..").join("b");
        assert_eq!(local_folder(sneaky.to_str().unwrap()), Err("error.folder.notLocal"));
    }

    /// A folder, and a link to it made where the system lets a test make one (Windows needs a privilege for that).
    fn linked() -> Option<(tempfile::TempDir, PathBuf)> {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real");
        std::fs::create_dir(&real).unwrap();
        let link = dir.path().join("link");
        #[cfg(unix)]
        let made = std::os::unix::fs::symlink(&real, &link);
        #[cfg(windows)]
        let made = std::os::windows::fs::symlink_dir(&real, &link);
        made.ok().map(|()| (dir, link))
    }

    #[test]
    fn a_link_is_refused_where_it_stands_and_so_is_anything_below_it() {
        let Some((_dir, link)) = linked() else { return };
        assert_eq!(local_folder(link.to_str().unwrap()), Err("error.folder.link"));
        assert_eq!(local_folder(link.join("deeper").to_str().unwrap()), Err("error.folder.link"));
    }

    #[cfg(windows)]
    #[test]
    fn a_network_drive_is_refused_before_anything_is_looked_up() {
        // The drive does not exist, so a lookup would say "missing": the answer "not local" proves nothing was touched.
        assert_eq!(local_folder_with(r"Q:\projects\demo", &|letter| letter == 'Q'), Err("error.folder.notLocal"));
        assert_eq!(local_folder_with(r"Q:\projects\demo", &|_| false), Err("error.folder.missing"));
    }
}
