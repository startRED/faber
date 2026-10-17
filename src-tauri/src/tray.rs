//! The tray icon: a number on it while an agent needs the user, a tooltip with the words, and one notification per
//! question. What to show and when to notify is decided by plain functions (tested without a tray); the rest hands
//! those answers to Tauri. The deciding lives in `decide`, the drawing in `badge`.

mod badge;
mod decide;
#[cfg(windows)]
mod native;
mod sound;

use badge::{glyph, tray_png, tray_side, with_badge};
pub(crate) use decide::ShellTexts;
use decide::{Counts, Shown, Watch, decode_seen, delivery, notice_text, parked_notice_text, stopped_text, tooltip};
use faber_engine::model::{RunningItem, Vendor};
use faber_engine::project::wire::MissionView;
use std::collections::HashSet;
use std::path::Path;
use std::sync::Mutex;
use tauri::image::Image;
use tauri::menu::{IsMenuItem, Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};
#[cfg(not(target_os = "linux"))]
use tauri_plugin_notification::NotificationExt;

#[derive(Default)]
struct State {
    /// None in a hidden run, which has no tray and so notifies nobody.
    icon: Option<TrayIcon>,
    menu: Option<[MenuItem<Wry>; 2]>,
    /// Linux only: a disabled first menu item that repeats the tooltip, because that backend cannot show a tooltip.
    /// ponytail: a menu line stands in for the tooltip there; the number is still on the icon. Verified on no real desktop yet.
    status: Option<MenuItem<Wry>>,
    texts: ShellTexts,
    watch: Watch,
    counts: Counts,
    shown: Shown,
    /// Corrupt notification receipts suppress mission delivery, rather than notifying old choices again.
    mission_notices: bool,
}

/// The tray's state. Managed by Tauri from the start of `setup`, so the engine's first event finds it.
#[derive(Default)]
pub(crate) struct Tray(Mutex<State>);

fn lock(tray: &Tray) -> std::sync::MutexGuard<'_, State> {
    tray.0.lock().unwrap_or_else(|e| e.into_inner())
}

/// The side of the tray picture that fits this display. Read at each running list, so a scale that changes is noticed
/// with the next list (and the picture is redrawn then, see `Shown`).
fn side(app: &AppHandle) -> u32 {
    tray_side(app.primary_monitor().ok().flatten().map_or(1.0, |m| m.scale_factor()))
}

/// The picture for `needs_you` waiting items on a tray picture of `side` px. A tray picture that does not decode falls back
/// to the window icon; a badge that cannot be drawn leaves the plain picture.
fn picture(app: &AppHandle, side: u32, needs_you: usize) -> Option<Image<'static>> {
    let plain = match Image::from_bytes(tray_png(side)) {
        Ok(image) => image,
        Err(e) => {
            eprintln!("faber: tray-{side}.png did not decode ({e}); using the window icon");
            return app.default_window_icon().map(|i| i.clone().to_owned());
        }
    };
    let Some(c) = glyph(needs_you) else { return Some(plain) };
    match with_badge(plain.rgba(), plain.width(), plain.height(), c) {
        Some(rgba) => Some(Image::new_owned(rgba, plain.width(), plain.height())),
        None => {
            eprintln!("faber: no badge could be drawn on tray-{side}.png; showing the plain icon");
            Some(plain)
        }
    }
}

/// Puts the tooltip (and its menu stand-in on Linux) of the current counts on the tray.
fn say(s: &State) -> Result<(), String> {
    let text = tooltip(&s.texts, s.counts);
    if let Some(status) = &s.status {
        status.set_text(&text).map_err(|e| e.to_string())?;
    }
    match &s.icon {
        Some(icon) => icon.set_tooltip(Some(text)).map_err(|e| e.to_string()),
        None => Ok(()),
    }
}

/// Puts the tooltip and the picture of the current counts on the tray.
fn show_counts(app: &AppHandle, s: &State, side: u32) {
    let Some(icon) = &s.icon else { return };
    if let Err(e) = say(s) {
        eprintln!("faber: the tray tooltip could not change ({e})");
    }
    if let Some(image) = picture(app, side, s.counts.needs_you)
        && let Err(e) = icon.set_icon(Some(image))
    {
        eprintln!("faber: the tray icon could not change ({e})");
    }
}

fn load_seen_decisions(data_dir: &Path) -> Result<HashSet<(String, String)>, String> {
    let path = data_dir.join("seen_decisions.json");
    match std::fs::read(path) {
        Ok(bytes) => decode_seen(&bytes).map_err(str::to_string),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(HashSet::new()),
        Err(_) => Err("error.store.read".into()),
    }
}

fn save_seen_decisions(data_dir: &Path, seen: &HashSet<(String, String)>) -> Result<(), String> {
    let path = data_dir.join("seen_decisions.json");
    let json = serde_json::to_vec(seen).map_err(|_| "error.store.write")?;
    faber_engine::store::replace(&faber_engine::store::RealDisk, &path, &json).map_err(|e| e.key().into())
}

/// The words the screen sent last, so a Faber started hidden (with Windows) speaks the person's language; English until
/// a window has opened once.
fn load_texts(data_dir: &Path) -> ShellTexts {
    std::fs::read(data_dir.join("shell-texts.json")).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
}

fn save_texts(data_dir: &Path, texts: &ShellTexts) -> Result<(), String> {
    let json = serde_json::to_vec(texts).map_err(|_| "error.store.write")?;
    faber_engine::store::replace(&faber_engine::store::RealDisk, &data_dir.join("shell-texts.json"), &json).map_err(|e| e.key().into())
}

pub(crate) fn create(app: &tauri::App) -> tauri::Result<()> {
    let handle = app.handle();
    let data_dir = handle.state::<crate::Faber>().data_dir.clone();
    #[cfg(not(windows))]
    if let Err(e) = sound::prepare(&data_dir) {
        eprintln!("faber: the attention chime could not be prepared ({e})");
    }
    let tray = handle.state::<Tray>();
    let mut s = lock(&tray);
    s.texts = load_texts(&data_dir);
    match load_seen_decisions(&data_dir) {
        Ok(seen) => {
            s.watch = Watch::with_seen_decisions(seen);
            s.mission_notices = true;
        }
        Err(error) => eprintln!("faber: mission notification receipts could not load ({error})"),
    }
    let open = MenuItem::with_id(app, "open", &s.texts.open, true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", &s.texts.quit, true, None::<&str>)?;
    let status = if cfg!(target_os = "linux") {
        Some(MenuItem::with_id(app, "status", tooltip(&s.texts, s.counts), false, None::<&str>)?)
    } else {
        None
    };
    let mut items: Vec<&dyn IsMenuItem<Wry>> = status.iter().map(|i| i as &dyn IsMenuItem<Wry>).collect();
    items.extend([&open as &dyn IsMenuItem<Wry>, &quit]);
    let mut builder = TrayIconBuilder::new()
        .tooltip(tooltip(&s.texts, s.counts))
        .menu(&Menu::with_items(app, &items)?)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, e| match e.id().as_ref() {
            "open" => crate::show(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, e| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                crate::show(tray.app_handle());
            }
        });
    if let Some(image) = picture(handle, side(handle), 0) {
        builder = builder.icon(image);
    }
    s.icon = Some(builder.build(app)?);
    s.menu = Some([open, quit]);
    s.status = status;
    drop(s);
    let app = handle.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if let Err(error) = crate::restore_mission_notices(&app) {
            eprintln!("faber: saved mission attention could not restore ({error})");
        }
    });
    Ok(())
}

/// The screen's words for the menu, the tooltip and the notification. A language change reaches the tray at once.
pub(crate) fn set_texts(app: &AppHandle, texts: ShellTexts) -> Result<(), String> {
    let tray = app.state::<Tray>();
    let mut s = lock(&tray);
    if s.texts != texts {
        if let Err(error) = save_texts(&app.state::<crate::Faber>().data_dir, &texts) {
            eprintln!("faber: the tray's words could not be kept ({error})");
        }
        s.texts = texts;
    }
    if let Some([open, quit]) = &s.menu {
        open.set_text(&s.texts.open).map_err(|e| e.to_string())?;
        quit.set_text(&s.texts.quit).map_err(|e| e.to_string())?;
    }
    say(&s)
}

/// Whether a game or another full-screen app is in front (R191). Only Windows can tell today.
pub(crate) fn game_in_front() -> bool {
    #[cfg(windows)]
    return native::game();
    // shortcut: no global macOS/Linux full-screen probe; add native detection when those backends are implemented (R92).
    #[cfg(not(windows))]
    false
}

/// What the notifications of one update say: the needs-you titles, then the requests held back for want of an answer.
fn words(texts: &ShellTexts, update: &decide::Update) -> Vec<String> {
    update
        .notices
        .iter()
        .map(|title| notice_text(texts, title))
        .chain(update.parked_notices.iter().map(|(title, n)| parked_notice_text(texts, title, *n)))
        .collect()
}

fn deliver_notices(app: &AppHandle, settings: &faber_engine::model::Settings, words: &[String]) {
    for text in words {
        // Recheck focus at delivery, in case the user returned while the tray picture was updating.
        let focused = crate::main_window(app).is_some_and(|w| w.is_focused().unwrap_or(false));
        #[cfg(windows)]
        let attention = native::attention();
        // shortcut: no global macOS/Linux full-screen probe; add native detection when those backends are implemented (R92).
        #[cfg(not(windows))]
        let attention = decide::Attention::Normal;
        let send = delivery(focused, attention, settings);
        if send.sound {
            #[cfg(windows)]
            if let Err(e) = native::play() {
                eprintln!("faber: the attention chime could not play ({e})");
            }
            #[cfg(not(windows))]
            {
                let dir = app.state::<crate::Faber>().data_dir.clone();
                tauri::async_runtime::spawn_blocking(move || {
                    if let Err(e) = sound::play(&dir) {
                        eprintln!("faber: the attention chime could not play ({e})");
                    }
                });
            }
        }
        if !send.toast {
            continue;
        }
        // Faber does not rely on the user clicking a notification, which each system handles in its own way.
        // ponytail: tauri-plugin-notification 2.6 sends in the background and drops the system's answer, so `show` fails
        // only when the notification cannot be built; a refused or unavailable service leaves no line here. Replace the
        // plugin by notify-rust (already in Cargo.lock) if hand checks show a silent failure.
        #[cfg(not(target_os = "linux"))]
        // With no sound name, these backends use WinRT audio silent=true and macOS sound=None.
        let result = app.notification().builder().title(text).show().map_err(|e| e.to_string());
        // The Linux plugin cannot pass SuppressSound; keep desktop sounds off so the sound switch also means silence.
        #[cfg(target_os = "linux")]
        let result = notify_rust::Notification::new()
            .summary(text)
            .hint(notify_rust::Hint::SuppressSound(true))
            .show()
            .map(|_| ())
            .map_err(|e| e.to_string());
        if let Err(e) = result {
            eprintln!("faber: the notification could not be sent ({e})");
        }
    }
}

/// A new running list from the engine. Lists can arrive out of order (each runs on its own thread), so the newest
/// revision wins.
pub(crate) fn on_running(app: &AppHandle, revision: u64, items: &[RunningItem]) {
    let faber = app.state::<crate::Faber>();
    if faber.hidden {
        return;
    }
    let settings = faber.engine.settings();
    let tray = app.state::<Tray>();
    let mut s = lock(&tray);
    if s.icon.is_none() {
        return;
    }
    // A closed window is not focused either.
    let focused = crate::main_window(app).is_some_and(|w| w.is_focused().unwrap_or(false));
    let Some(update) = s.watch.take(revision, items, focused) else { return };
    let side = side(app);
    if s.shown.changed(update.counts, side) {
        s.counts = update.counts;
        show_counts(app, &s, side);
    }
    deliver_notices(app, &settings, &words(&s.texts, &update));
}

/// "Keep going" stopped by itself in a conversation (R199): one notice that says why, under R92's rules.
pub(crate) fn on_halted(app: &AppHandle, title: &str, why: &str, ai: Option<Vendor>) {
    let faber = app.state::<crate::Faber>();
    if faber.hidden {
        return;
    }
    let settings = faber.engine.settings();
    let tray = app.state::<Tray>();
    let s = lock(&tray);
    if s.icon.is_none() {
        return;
    }
    let text = stopped_text(&s.texts, title, why, ai);
    drop(s);
    deliver_notices(app, &settings, &[text]);
}

/// A mission view update from the engine: non-technical decisions become needs-you items.
pub(crate) fn on_mission(app: &AppHandle, view: &MissionView) {
    let faber = app.state::<crate::Faber>();
    if faber.hidden {
        return;
    }
    let settings = faber.engine.settings();
    let tray = app.state::<Tray>();
    let mut s = lock(&tray);
    if s.icon.is_none() {
        return;
    }
    let focused = crate::main_window(app).is_some_and(|w| w.is_focused().unwrap_or(false));
    let before = s.watch.seen_decisions().len();
    let Some(update) = s.watch.take_mission(view, focused) else { return };
    let saved = s.mission_notices
        && (before == s.watch.seen_decisions().len()
            || match save_seen_decisions(&faber.data_dir, s.watch.seen_decisions()) {
                Ok(()) => true,
                Err(error) => {
                    eprintln!("faber: mission notification receipts could not save ({error})");
                    false
                }
            });
    let side = side(app);
    if s.shown.changed(update.counts, side) {
        s.counts = update.counts;
        show_counts(app, &s, side);
    }
    if saved {
        deliver_notices(app, &settings, &words(&s.texts, &update));
    }
}

/// Exercise the same receipt store and policy in a hidden build, without playing a sound or submitting a popup.
#[cfg(feature = "test-hooks")]
pub(crate) fn probe_mission(app: &AppHandle, view: &MissionView, focused: bool, attention: &str) -> Result<serde_json::Value, String> {
    let faber = app.state::<crate::Faber>();
    if !faber.hidden {
        return Err("error.wire.malformed".into());
    }
    let attention = match attention {
        "normal" => decide::Attention::Normal,
        "soundOnly" => decide::Attention::SoundOnly,
        "uncertain" => decide::windows_attention(None, false),
        _ => return Err("error.wire.malformed".into()),
    };
    let mut watch = Watch::with_seen_decisions(load_seen_decisions(&faber.data_dir)?);
    let update = watch.take_mission(view, focused).ok_or("error.wire.stale")?;
    save_seen_decisions(&faber.data_dir, watch.seen_decisions())?;
    let policy = delivery(focused, attention, &faber.engine.settings());
    Ok(
        serde_json::json!({"toast": policy.toast && !update.notices.is_empty(), "sound":policy.sound && !update.notices.is_empty(), "needsYou":update.counts.needs_you}),
    )
}

#[cfg(test)]
mod tests {
    use super::{ShellTexts, load_texts, save_texts};

    #[test]
    fn the_words_the_screen_sent_last_are_there_after_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load_texts(dir.path()), ShellTexts::default(), "English before any window has opened");
        let texts: ShellTexts = serde_json::from_value(serde_json::json!({
            "open": "Abrir", "quit": "Sair",
            "running": { "one": "Faber: {n} em andamento", "other": "Faber: {n} em andamento" },
            "needsYou": { "one": ", {n} precisa de você", "other": ", {n} precisam de você" },
            "notify": "{title} precisa de você", "notifyMany": "{title}: {n} pedidos precisam de você"
        }))
        .unwrap();
        save_texts(dir.path(), &texts).unwrap();
        assert_eq!(load_texts(dir.path()), texts);
        std::fs::write(dir.path().join("shell-texts.json"), b"{ damaged").unwrap();
        assert_eq!(load_texts(dir.path()), ShellTexts::default(), "damaged words fall back to English");
    }
}
