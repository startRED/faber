//! Opens the system terminal running a CLI in its own interface, detached from Faber: the one-click way out when
//! Faber cannot follow a CLI's output.

use faber_engine::TerminalCommand;
use std::io;
use std::process::Command;

/// Characters a shell acts on even inside quotes; paths with accents or parentheses are fine.
const UNSAFE: &[char] = &['"', '%', '&', '|', '<', '>', '^', '`', '$', '!', '\r', '\n'];

pub fn is_safe(cmd: &TerminalCommand) -> bool {
    let cwd = cmd.cwd.to_string_lossy();
    std::iter::once(cmd.command.as_str()).chain(cmd.args.iter().map(String::as_str)).chain([cwd.as_ref()]).all(|p| !p.contains(UNSAFE))
}

use std::sync::Mutex;

static HIDDEN_CHILDREN: Mutex<Vec<std::process::Child>> = Mutex::new(Vec::new());

pub fn terminate_hidden() {
    if let Ok(mut children) = HIDDEN_CHILDREN.lock() {
        for child in children.iter_mut() {
            let _ = child.kill();
        }
        children.clear();
    }
}

pub fn open(cmd: &TerminalCommand) -> io::Result<()> {
    // Windows and macOS build a line for a shell, so every piece is checked first.
    if !is_safe(cmd) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "refusing to open a terminal with shell characters"));
    }
    let hidden = std::env::var("FABER_HIDDEN").is_ok_and(|v| v == "1");
    if hidden {
        let mut command = Command::new(&cmd.command);
        command.args(&cmd.args).current_dir(&cmd.cwd);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let child = command.spawn()?;
        if let Ok(mut children) = HIDDEN_CHILDREN.lock() {
            children.push(child);
        }
        return Ok(());
    }
    let quoted = std::iter::once(&cmd.command).chain(&cmd.args).map(|p| format!("\"{p}\"")).collect::<Vec<_>>().join(" ");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        // `start` gives the CLI a console of its own; raw_arg keeps cmd's quoting exactly as written.
        Command::new("cmd.exe")
            .raw_arg(format!("/c start \"Faber\" {quoted}"))
            .current_dir(&cmd.cwd)
            .creation_flags(DETACHED_PROCESS)
            .spawn()?;
    }
    #[cfg(target_os = "macos")]
    {
        let script = std::env::temp_dir().join(format!("faber-{}.command", std::process::id()));
        std::fs::write(&script, format!("#!/bin/sh\ncd \"{}\" && exec {quoted}\n", cmd.cwd.display()))?;
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))?;
        Command::new("open").args(["-a", "Terminal"]).arg(&script).spawn()?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = quoted;
        Command::new("x-terminal-emulator").arg("-e").arg(&cmd.command).args(&cmd.args).current_dir(&cmd.cwd).spawn()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn cmd(args: &[&str]) -> TerminalCommand {
        TerminalCommand {
            command: "claude".into(),
            args: args.iter().map(|s| s.to_string()).collect(),
            cwd: PathBuf::from("C:/Users/Ana/Projetos (novo)"),
        }
    }

    #[test]
    fn refuses_shell_characters_and_accepts_plain_paths() {
        assert!(is_safe(&cmd(&["--resume", "0b1c-22"])));
        assert!(!is_safe(&cmd(&["--resume", "x\" & calc"])));
        assert!(!is_safe(&cmd(&["%PATH%"])));
    }
}
