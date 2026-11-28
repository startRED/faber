//! What Windows says about the window in front, put to the tray's rules (`decide`): may a notification show, and is a game
//! in front. The reads are `faber-winproc`'s; the rules are Faber's, so Tauri and the sidecar answer alike. Only Windows can
//! tell today.

use super::decide::Attention;

/// How a notification is delivered right now (R92): in full, as a chime only, or not at all. `own_process` is the process
/// whose windows are Faber's: this one in Tauri, the Electron shell's main process for the sidecar.
pub fn attention(own_process: u32) -> Attention {
    #[cfg(windows)]
    return windows::attention(own_process);
    // shortcut: no global macOS/Linux full-screen probe; add native detection when those backends are implemented (R92).
    #[cfg(not(windows))]
    {
        let _ = own_process;
        Attention::Normal
    }
}

/// Whether a game or another full-screen app is in front (R191). The screen asks every few seconds while its window is
/// shown without the focus, so a failed query says nothing in the log and only means "no". Faber's own window
/// (`own_process`, as for [`attention`]) is never a game, however much of the screen it covers.
pub fn game_in_front(own_process: u32) -> bool {
    #[cfg(windows)]
    return windows::game(own_process);
    // shortcut: no global macOS/Linux full-screen probe; add native detection when those backends are implemented (R92).
    #[cfg(not(windows))]
    {
        let _ = own_process;
        false
    }
}

#[cfg(windows)]
mod windows {
    use super::super::decide::{Attention, Foreground, covers_monitor, game_in_front, windows_attention};
    use faber_winproc::front::{self, Front};

    pub(super) fn attention(own_process: u32) -> Attention {
        let state = front::notification_state();
        if let Err(result) = state {
            eprintln!("faber: the notification state could not be read (HRESULT {result:#x}); suppressing the toast");
        }
        let attention = windows_attention(state.ok(), false);
        if attention != Attention::Normal {
            return attention;
        }
        match foreground(own_process) {
            Some(Foreground { own: true, .. }) => Attention::Silent,
            Some(front) => windows_attention(Some(5), front.covers),
            None => {
                eprintln!("faber: foreground window bounds could not be read; suppressing the toast");
                Attention::SoundOnly
            }
        }
    }

    pub(super) fn game(own_process: u32) -> bool {
        let state = front::notification_state().ok();
        // A locked screen, quiet time or an unknown state is never a game, so the window in front is read only for the others.
        let front = if matches!(state, Some(2..=5 | 7)) { foreground(own_process) } else { None };
        game_in_front(state, front)
    }

    fn foreground(own_process: u32) -> Option<Foreground> {
        Some(match front::foreground(own_process)? {
            Front::Own => Foreground { own: true, covers: false },
            Front::Other { window, monitor } => Foreground { own: false, covers: covers_monitor(window, monitor) },
        })
    }
}
