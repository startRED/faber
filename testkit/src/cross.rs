//! The macOS Clippy proof on a Windows PC. CI checks macOS only after a push, so code behind `cfg(unix)` or
//! `cfg(target_os = "macos")` that breaks there used to cost a whole CI round and a fix commit. Clippy needs no linker,
//! but `aws-lc-sys` compiles C, so zig stands in as the target's C compiler and archiver: Cargo calls this runner as
//! `CC` and `AR`, and [`zig_mode`] hands the call to zig. A PC without zig skips the proof; CI still runs it on a Mac.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const TARGET: &str = "aarch64-apple-darwin";
/// Zig's name for [`TARGET`].
const ZIG_TARGET: &str = "aarch64-macos";
/// Set only on the proof's Cargo command: the path of zig, which tells the runner it was called as `CC` or `AR`.
const ZIG_ENV: &str = "FABER_ZIG";

/// The proof. The shell (`faber`) is left out: its Objective-C needs Apple's SDK, which only a Mac has.
pub const PROOF: &[&str] =
    &["clippy", "-p", "faber-engine", "-p", "faber-testkit", "--all-targets", "--target", TARGET, "--", "-D", "warnings"];

/// Zig on `PATH`, if any.
pub fn find_zig() -> Option<PathBuf> {
    let name = if cfg!(windows) { "zig.exe" } else { "zig" };
    std::env::split_paths(&std::env::var_os("PATH")?).map(|dir| dir.join(name)).find(|path| path.is_file())
}

/// Makes `command` build the target's C with zig, through this runner.
pub fn use_zig(command: &mut Command, zig: &Path) -> std::io::Result<()> {
    let me = std::env::current_exe()?;
    command.env("CC_aarch64_apple_darwin", &me).env("AR_aarch64_apple_darwin", &me).env(ZIG_ENV, zig);
    Ok(())
}

/// When Cargo calls the runner as the target's C compiler or archiver: runs zig and gives its exit code.
pub fn zig_mode() -> Option<i32> {
    let zig = std::env::var_os(ZIG_ENV)?;
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let status = Command::new(zig).args(zig_args(&args)).status();
    Some(status.map_or(127, |s| s.code().unwrap_or(1)))
}

/// Zig's arguments for a call. A compiler call starts with a flag; an archiver call starts with an operation such as
/// `cq` or `s`. Zig takes its own `-target`, so the `cc` crate's `-arch arm64` and `--target=...` go.
fn zig_args(args: &[OsString]) -> Vec<OsString> {
    let archive = args.first().is_some_and(|first| !first.to_string_lossy().starts_with('-'));
    if archive {
        return std::iter::once(OsString::from("ar")).chain(args.iter().cloned()).collect();
    }
    let mut out: Vec<OsString> = ["cc", "-target", ZIG_TARGET].map(OsString::from).to_vec();
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.to_string_lossy() {
            a if a == "-arch" || a == "-target" => {
                rest.next();
            }
            a if a.starts_with("--target=") => {}
            _ => out.push(arg.clone()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    #[test]
    fn a_compiler_call_goes_to_zig_cc_with_zigs_target_instead_of_the_cc_crates() {
        let call = os(&["-O0", "-arch", "arm64", "--target=arm64-apple-macosx", "-c", "a.c", "-target", "x"]);
        assert_eq!(zig_args(&call), os(&["cc", "-target", "aarch64-macos", "-O0", "-c", "a.c"]));
    }

    #[test]
    fn an_archiver_call_goes_to_zig_ar_as_it_is() {
        assert_eq!(zig_args(&os(&["cq", "lib.a", "a.o"])), os(&["ar", "cq", "lib.a", "a.o"]));
        assert_eq!(zig_args(&os(&["s", "lib.a"])), os(&["ar", "s", "lib.a"]));
    }
}
