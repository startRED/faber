//! Start with the system (opt-in): the arguments a start at login carries, and what the setting's switch shows. Registering
//! Faber with the system (the registry on Windows, a login item in Electron) is the shell's own call. Faber started this way
//! begins in the tray, with no window.

/// The argument the system start passes, so Faber knows to begin in the tray.
pub const AUTOSTART_FLAG: &str = "--autostart";

/// Checks whether `--autostart` is in the arguments.
pub fn is_autostart_arg(args: impl IntoIterator<Item = String>) -> bool {
    args.into_iter().any(|a| a == AUTOSTART_FLAG)
}

/// A start at login makes no window; any other start does.
pub fn creates_window(args: impl IntoIterator<Item = String>) -> bool {
    !is_autostart_arg(args)
}

/// What the switch shows: whether the system will really start Faber. The registration can go without Faber (a cleanup tool,
/// another account tool), and the setting alone would then still say on. A registration that cannot be read leaves the setting
/// as saved.
pub fn shown(registered: Result<bool, String>, saved: bool) -> bool {
    registered.unwrap_or(saved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_switch_shows_the_registration_and_the_setting_only_when_the_registration_cannot_be_read() {
        assert!(!shown(Ok(false), true), "a registration removed outside Faber turns the switch off");
        assert!(shown(Ok(true), false));
        assert!(shown(Err("access denied".into()), true));
        assert!(!shown(Err("access denied".into()), false));
    }

    #[test]
    fn autostart_flag_detection() {
        assert!(is_autostart_arg(vec!["faber.exe".into(), "--autostart".into()]));
        assert!(!is_autostart_arg(vec!["faber.exe".into(), "--other".into()]));
        assert!(!is_autostart_arg(vec![]));
    }

    #[test]
    fn autostart_never_creates_a_window_even_in_hidden_tests() {
        assert!(!creates_window(vec!["faber.exe".into(), AUTOSTART_FLAG.into()]));
        assert!(creates_window(vec!["faber.exe".into()]));
    }
}
