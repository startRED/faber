//! The self-updater inside the sidecar (`faber-update`): what only this process knows, given to it as `faber_update::App`.
//!
//! Updates are on only when the app that started this process says so: `FABER_UPDATES=1` (Electron sets it for a packaged
//! app that is not a hidden test run) and `FABER_INSTALL_DIR`, the folder of the app main program. Otherwise nothing here
//! runs, and the screen sees no update. The status of an update reaches the screen as the event `{"type":"update",...}`
//! (the shell gives it to the screen as `faber:update`), and the end of the app as `Order::Quit`.

use faber_host::Host;
use faber_host::sidecar::SidecarShell;
use faber_host::wire::{Event, Order, Ordered, line};
use faber_update::{App, Status, Updater, Who};
use serde_json::{Value, json};
use std::any::Any;
use std::path::PathBuf;
use std::sync::{Arc, Weak};

/// Writes one line to the shell: the process one writer.
pub type Write = Arc<dyn Fn(String) + Send + Sync>;

/// Where the app says its main program is, when it says updates are on.
pub fn install_dir(updates: Option<&str>, dir: Option<std::ffi::OsString>) -> Option<PathBuf> {
    (updates == Some("1")).then_some(dir).flatten().filter(|dir| !dir.is_empty()).map(PathBuf::from)
}

/// The event the screen hears when the update changes: the status, tagged so the shell can route it.
fn event(status: Status) -> Value {
    let mut event = serde_json::to_value(status).unwrap_or_default();
    event["type"] = json!("update");
    event
}

/// Holds the host and the shell weakly: they own the updater, and a cycle would keep them alive for nothing.
struct Carrier {
    host: Weak<Host>,
    shell: Weak<SidecarShell>,
    write: Write,
    previews: Box<dyn Fn() -> bool + Send + Sync>,
}

impl App for Carrier {
    fn previews(&self) -> bool {
        (self.previews)()
    }

    fn window_visible(&self) -> bool {
        self.shell.upgrade().is_none_or(|shell| shell.window_visible())
    }

    fn close_engine(&self, who: Who) -> Option<Box<dyn Any + Send>> {
        let host = self.host.upgrade()?;
        // The person's own restart cuts what runs (and `shut_down` notes it, so it goes on after); every other install waits for
        // an engine at rest.
        if who == Who::Owner {
            return host.engine.install_admission_stopping().ok().map(|guard| Box::new(guard) as Box<dyn Any + Send>);
        }
        let guard = host.engine.install_admission().ok()?;
        (!host.engine.is_working()).then(|| Box::new(guard) as Box<dyn Any + Send>)
    }

    fn shut_down(&self, resume: bool) {
        if let Some(host) = self.host.upgrade() {
            if resume {
                host.engine.note_resume();
            }
            host.shut_down();
        }
    }

    fn quit(&self) {
        (self.write)(line(&Ordered { order: Order::Quit }));
    }

    fn emit(&self, status: Status) {
        (self.write)(line(&Event { event: event(status) }));
    }
}

/// Starts the updater when updates are on. The shell hears of it first, so a status asked for at once is answered.
pub fn start(host: &Arc<Host>, shell: &Arc<SidecarShell>, write: Write, data_dir: PathBuf, version: &str, dir: PathBuf) {
    let engine = host.engine.clone();
    let carrier = Carrier {
        host: Arc::downgrade(host),
        shell: Arc::downgrade(shell),
        write,
        previews: Box::new(move || engine.settings().previews == Some(true)),
    };
    // A refusal is already in `update.log`; Faber then stays on this version.
    if let Ok(updater) = Updater::new(Arc::new(carrier), version, data_dir, &dir) {
        shell.attach_updater(updater.clone());
        updater.start();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use faber_update::Phase;
    use std::ffi::OsString;

    #[test]
    fn updates_are_on_only_when_the_app_says_so_and_names_its_folder() {
        let dir = || Some(OsString::from(r"E:\Apps\Faber"));
        assert_eq!(install_dir(Some("1"), dir()), Some(PathBuf::from(r"E:\Apps\Faber")));
        assert_eq!(install_dir(None, dir()), None, "an unpackaged app never says so");
        assert_eq!(install_dir(Some("0"), dir()), None);
        assert_eq!(install_dir(Some("1"), None), None, "no folder, no install");
        assert_eq!(install_dir(Some("1"), Some(OsString::new())), None);
    }

    #[test]
    fn the_screen_hears_the_status_as_the_tauri_shell_sent_it_plus_the_tag_that_routes_it() {
        assert_eq!(event(Status::of(Phase::Ready)), json!({"type":"update","phase":"ready"}));
        let downloading = Status { percent: Some(42), ..Status::of(Phase::Downloading) };
        assert_eq!(event(downloading), json!({"type":"update","phase":"downloading","percent":42}));
    }

    #[test]
    fn the_end_of_the_app_is_one_order_line() {
        assert_eq!(line(&Ordered { order: Order::Quit }), "{\"order\":{\"kind\":\"quit\"}}\n");
    }
}
