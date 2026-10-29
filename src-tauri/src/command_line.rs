//! Faber's command line (`faber`, in `cli/`), as the app sees it: the installer puts it at `bin\faber.exe` beside the app
//! (`src-tauri/tauri.cli.conf.json`), and the app puts that folder on the person's PATH, so a new terminal finds `faber`.

use std::path::{Path, PathBuf};

/// The folder of the shipped command line, when this install has one.
fn shipped(exe: &Path) -> Option<PathBuf> {
    let bin = exe.parent()?.join("bin");
    bin.join(if cfg!(windows) { "faber.exe" } else { "faber" }).is_file().then_some(bin)
}

/// Puts the command line's folder on the user's PATH, the same way the CLIs Faber installs get there. A build without it (a
/// dev or test build) changes nothing. A folder already there is left as it is.
pub fn put_on_path() {
    if !cfg!(windows) {
        return;
    }
    let Some(bin) = std::env::current_exe().ok().and_then(|exe| shipped(&exe)) else { return };
    if let Err(error) = faber_engine::install::default_register_path(&bin) {
        eprintln!("faber: the command line could not be put on PATH: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_install_that_ships_the_command_line_names_its_folder() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("faber.exe");
        assert_eq!(shipped(&exe), None, "a dev build has no bin folder");
        std::fs::create_dir(dir.path().join("bin")).unwrap();
        std::fs::write(dir.path().join("bin").join(if cfg!(windows) { "faber.exe" } else { "faber" }), b"").unwrap();
        assert_eq!(shipped(&exe), Some(dir.path().join("bin")));
    }
}
