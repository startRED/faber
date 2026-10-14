//! Which saved path "Open folder" may show. The path comes from a file on disk, so it is checked every time: a folder that
//! was swapped for a program, a web address and a network share are all refused, and what is shown is only revealed in the
//! file manager, never opened.

use std::path::{Component, Path, PathBuf, Prefix};

/// The existing local folder behind a saved project path, or the key of the sentence that says why not.
///
/// ponytail: the folder is checked here and revealed a moment later, so it can still be swapped in between. Revealing only
/// selects an item in the file manager and starts nothing, so the swap cannot run anything; no handle is held across.
pub fn local_folder(saved: &str) -> Result<PathBuf, &'static str> {
    // A web address or a network share written the way Windows writes them.
    if saved.contains("://") || saved.starts_with(r"\\") || saved.starts_with("//") {
        return Err("error.folder.notLocal");
    }
    let path = Path::new(saved);
    if !path.is_absolute() {
        return Err("error.folder.notLocal");
    }
    // Resolving links and mapped drives first: a mapped network drive resolves to its share, which is refused below.
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
}
