//! Where the channel of one Faber lives. Each data folder has its own, so a test build with `FABER_DATA_DIR` never answers
//! for the installed Faber, and the client finds it from the same folder the app uses.

use std::path::{Path, PathBuf};

/// Faber's data folder: `FABER_DATA_DIR` when set, else `Faber` under the user's config folder (Roaming on Windows),
/// the same rule as the app.
pub fn data_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("FABER_DATA_DIR") {
        return Some(PathBuf::from(dir));
    }
    config_dir().map(|dir| dir.join("Faber"))
}

fn config_dir() -> Option<PathBuf> {
    let var = |name| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    if cfg!(windows) {
        var("APPDATA")
    } else if cfg!(target_os = "macos") {
        var("HOME").map(|home| home.join("Library").join("Application Support"))
    } else {
        var("XDG_CONFIG_HOME").or_else(|| var("HOME").map(|home| home.join(".config")))
    }
}

/// The channel of the Faber that keeps its data in one folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Address {
    /// `\\.\pipe\faber-<hash of the folder>`.
    Pipe(String),
    /// `<data>/channel/faber.sock`, in a folder only its owner can open.
    Socket(PathBuf),
}

impl Address {
    pub fn of(data: &Path) -> Address {
        if cfg!(windows) {
            Address::Pipe(format!(r"\\.\pipe\faber-{:016x}", fnv1a(&folder_key(data))))
        } else {
            Address::Socket(data.join("channel").join("faber.sock"))
        }
    }
}

/// Windows paths ignore case and either slash, so `C:\Users\A\Faber` and `c:/users/a/faber/` name one channel.
fn folder_key(data: &Path) -> String {
    let text = data.to_string_lossy().replace('/', "\\");
    let text = text.trim_end_matches('\\');
    if cfg!(windows) { text.to_lowercase() } else { text.to_string() }
}

/// FNV-1a: stable across Rust versions, unlike the standard hasher, so a client and an app of different builds agree.
fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hash_is_fnv1a_and_never_changes() {
        assert_eq!(fnv1a(""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a("a"), 0xaf63_dc4c_8601_ec8c);
    }

    #[cfg(windows)]
    #[test]
    fn one_folder_written_two_ways_is_one_pipe_and_two_folders_are_two() {
        let a = Address::of(Path::new(r"C:\Users\A\AppData\Roaming\Faber"));
        assert_eq!(a, Address::of(Path::new("c:/users/a/appdata/roaming/faber/")));
        assert_ne!(a, Address::of(Path::new(r"C:\Users\B\AppData\Roaming\Faber")));
        let Address::Pipe(name) = a else { panic!() };
        assert!(name.starts_with(r"\\.\pipe\faber-") && name.len() == r"\\.\pipe\faber-".len() + 16);
    }

    #[cfg(unix)]
    #[test]
    fn the_socket_sits_in_the_channel_folder_of_the_data() {
        assert_eq!(Address::of(Path::new("/d")), Address::Socket(PathBuf::from("/d/channel/faber.sock")));
    }
}
