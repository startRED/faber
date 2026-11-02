//! The tray: a number on its icon while an agent needs the user, a tooltip with the words, and one notification per
//! question. What to show and when to notify is decided here by plain functions (tested without a tray); the shell only
//! draws it. `decide` holds the rules, `badge` the pictures, `front` the questions Windows answers. [`Deck`] puts them
//! together as orders for a shell that lives in another process (the sidecar); Tauri's tray calls the parts itself.

pub mod badge;
pub mod chime;
pub mod decide;
pub mod front;

use crate::wire::{Order, TrayMenu};
use base64::Engine as _;
use decide::{Attention, Counts, ShellTexts, Spoken, Watch, delivery, halted_notice, spoken, tooltip};
use faber_engine::model::{RunningItem, Settings, Vendor};
use std::path::Path;

/// The words the screen sent last, so a Faber started hidden (with the system) speaks the person's language; English until
/// a window has opened once.
pub fn load_texts(data_dir: &Path) -> ShellTexts {
    std::fs::read(data_dir.join("shell-texts.json")).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
}

pub fn save_texts(data_dir: &Path, texts: &ShellTexts) -> Result<(), String> {
    let json = serde_json::to_vec(texts).map_err(|_| "error.store.write")?;
    faber_engine::store::replace(&faber_engine::store::RealDisk, &data_dir.join("shell-texts.json"), &json).map_err(|e| e.key().into())
}

/// The badge pictures for one count, base64 as the orders carry them.
struct Pictures {
    needs_you: usize,
    large: String,
    small: String,
}

/// What the tray shows and what it has told the shell: the words, the running list's numbers and the questions already
/// notified. It never returns an order the shell already has.
pub struct Deck {
    texts: ShellTexts,
    watch: Watch,
    counts: Counts,
    pictures: Option<Pictures>,
    told: Option<Order>,
}

impl Deck {
    pub fn new(texts: ShellTexts) -> Self {
        Self { texts, watch: Watch::default(), counts: Counts::default(), pictures: None, told: None }
    }

    pub fn texts(&self) -> &ShellTexts {
        &self.texts
    }

    /// The tray as it is now, or None when the shell was told exactly this already. The pictures are drawn again only when the
    /// number on them changes.
    pub fn tray(&mut self) -> Option<Order> {
        let needs_you = self.counts.needs_you;
        if self.pictures.as_ref().is_none_or(|p| p.needs_you != needs_you) {
            let encode = |side| base64::engine::general_purpose::STANDARD.encode(badge::tray_png_with_badge(side, needs_you));
            self.pictures = Some(Pictures { needs_you, large: encode(32), small: encode(16) });
        }
        let pictures = self.pictures.as_ref()?;
        let order = Order::Tray {
            tooltip: tooltip(&self.texts, self.counts),
            needs_you,
            picture: pictures.large.clone(),
            picture16: pictures.small.clone(),
            menu: TrayMenu { show: self.texts.open.clone(), quit: self.texts.quit.clone() },
        };
        if self.told.as_ref() == Some(&order) {
            return None;
        }
        self.told = Some(order.clone());
        Some(order)
    }

    /// The screen's words changed (or arrived): the new tray, if any of what it shows differs.
    pub fn set_texts(&mut self, texts: ShellTexts) -> Option<Order> {
        self.texts = texts;
        self.tray()
    }

    /// A new running list from the engine. Lists can arrive out of order (each runs on its own thread), so the newest
    /// revision wins and an older one gives no orders. The tray first, then one notification for each question that is new,
    /// delivered as `attention` (asked once per notification, since a game may come to the front between two) and the
    /// settings allow. A question that comes while Faber's window is `focused` is seen there and never notified, later
    /// either; focus is read again as each notification is decided.
    pub fn running(&mut self, revision: u64, items: &[RunningItem], settings: &Settings, now: &Moment) -> Vec<Order> {
        let Some(update) = self.watch.take(revision, items, (now.focused)()) else { return Vec::new() };
        self.counts = update.counts;
        let mut orders: Vec<Order> = self.tray().into_iter().collect();
        orders.extend(notifications(spoken(&self.texts, &update), settings, now));
        orders
    }

    /// "Keep going" stopped by itself in a conversation (R199): one notice that says why, under R92's rules.
    pub fn halted(
        &self,
        conversation_id: &str,
        title: &str,
        why: &str,
        ai: Option<Vendor>,
        settings: &Settings,
        now: &Moment,
    ) -> Vec<Order> {
        notifications(vec![halted_notice(&self.texts, conversation_id, title, why, ai)], settings, now)
    }
}

/// What a notification depends on at the moment it is decided, read again for each one: how the system lets it through
/// (a game in front, quiet time), and whether Faber's window has the focus.
pub struct Moment<'a> {
    pub attention: &'a dyn Fn() -> Attention,
    pub focused: &'a dyn Fn() -> bool,
}

/// The notification orders for what was said: each only when it is shown or chimes.
fn notifications(said: Vec<Spoken>, settings: &Settings, now: &Moment) -> Vec<Order> {
    said.into_iter()
        .filter_map(|Spoken { text, conversation_id }| {
            let send = delivery((now.focused)(), (now.attention)(), settings);
            (send.toast || send.sound).then(|| Order::Notify {
                title: text,
                body: String::new(),
                conversation_id,
                toast: send.toast,
                sound: send.sound,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use faber_engine::model::{Area, Permission, RunState};
    use serde_json::json;

    #[test]
    fn the_words_the_screen_sent_last_are_there_after_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load_texts(dir.path()), ShellTexts::default(), "English before any window has opened");
        let texts: ShellTexts = serde_json::from_value(json!({
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

    fn item(conversation: &str, title: &str, requests: &[&str]) -> RunningItem {
        RunningItem {
            conversation_id: conversation.into(),
            turn_id: "t1".into(),
            title: title.into(),
            area: Area::Chat,
            project_id: None,
            vendor: None,
            model: None,
            state: if requests.is_empty() { RunState::Working } else { RunState::NeedsYou },
            since: 0,
            wait_until: None,
            permissions: requests
                .iter()
                .map(|r| Permission { request_id: (*r).into(), tool: "Write".into(), input: json!({}), epoch: None })
                .collect(),
            parked: vec![],
            parked_more: 0,
            stuck: vec![],
            stall: None,
            held: None,
            keep_going: false,
            keep_wait: None,
            keep_progress: None,
            weighing: false,
            settled: false,
            plan: None,
            now: None,
        }
    }

    fn no() -> bool {
        false
    }

    fn normal() -> Attention {
        Attention::Normal
    }

    fn tray(order: &Order) -> (&str, usize) {
        let Order::Tray { tooltip, needs_you, .. } = order else { panic!("not a tray order: {order:?}") };
        (tooltip, *needs_you)
    }

    #[test]
    fn the_tray_is_told_at_start_and_never_the_same_twice() {
        let mut deck = Deck::new(ShellTexts::default());
        let first = deck.tray().expect("the first tray is always told");
        assert_eq!(tray(&first), ("Faber", 0));
        let Order::Tray { picture, picture16, menu, .. } = &first else { unreachable!() };
        assert_eq!((menu.show.as_str(), menu.quit.as_str()), ("Open Faber", "Quit Faber"));
        let decoded = base64::engine::general_purpose::STANDARD.decode(picture).unwrap();
        assert_eq!(decoded, badge::tray_png(32), "nothing waits: the picture as shipped");
        assert_eq!(base64::engine::general_purpose::STANDARD.decode(picture16).unwrap(), badge::tray_png(16));
        assert_eq!(deck.tray(), None, "nothing changed");

        let settings = Settings::default();
        let list = [item("a", "A", &[]), item("b", "B", &["0"])];
        let orders = deck.running(1, &list, &settings, &Moment { attention: &normal, focused: &no });
        assert_eq!(tray(&orders[0]), ("Faber: 2 running, 1 needs you", 1));
        let Order::Tray { picture, .. } = &orders[0] else { unreachable!() };
        assert_ne!(base64::engine::general_purpose::STANDARD.decode(picture).unwrap(), badge::tray_png(32), "the badge is drawn");
        // The same numbers in a newer list change nothing on the tray (the question was told already).
        assert_eq!(deck.running(2, &list, &settings, &Moment { attention: &normal, focused: &no }), []);
        // An older list is ignored whatever it says.
        assert_eq!(deck.running(2, &[], &settings, &Moment { attention: &normal, focused: &no }), []);
        let orders = deck.running(3, &[], &settings, &Moment { attention: &normal, focused: &no });
        assert_eq!((orders.len(), tray(&orders[0])), (1, ("Faber", 0)));
    }

    #[test]
    fn a_language_change_is_told_once_and_only_when_it_differs() {
        let mut deck = Deck::new(ShellTexts::default());
        deck.tray().unwrap();
        assert_eq!(deck.set_texts(ShellTexts::default()), None, "the same words");
        let texts: ShellTexts = serde_json::from_value(json!({
            "open": "Abrir", "quit": "Sair",
            "running": { "one": "Faber: {n} em andamento", "other": "Faber: {n} em andamento" },
            "needsYou": { "one": ", {n} precisa de você", "other": ", {n} precisam de você" },
            "notify": "{title} precisa de você"
        }))
        .unwrap();
        let Some(Order::Tray { menu, .. }) = deck.set_texts(texts.clone()) else { panic!("new words must reach the menu") };
        assert_eq!((menu.show.as_str(), menu.quit.as_str()), ("Abrir", "Sair"));
        assert_eq!(deck.set_texts(texts), None);
        let orders = deck.running(1, &[item("a", "Relatório", &["0"])], &Settings::default(), &Moment { attention: &normal, focused: &no });
        assert_eq!(tray(&orders[0]), ("Faber: 1 em andamento, 1 precisa de você", 1));
        let Order::Notify { title, .. } = &orders[1] else { panic!("{orders:?}") };
        assert_eq!(title, "Relatório precisa de você", "the notification speaks the new language too");
    }

    #[test]
    fn a_question_notifies_once_in_the_conversation_it_belongs_to() {
        let mut deck = Deck::new(ShellTexts::default());
        let settings = Settings::default();
        let list = [item("conv-1", "Fix the build", &["0"])];
        let orders = deck.running(1, &list, &settings, &Moment { attention: &normal, focused: &no });
        assert_eq!(
            orders[1..],
            [Order::Notify {
                title: "Fix the build needs you".into(),
                body: String::new(),
                conversation_id: Some("conv-1".into()),
                toast: true,
                sound: true
            }]
        );
        assert_eq!(deck.running(2, &list, &settings, &Moment { attention: &normal, focused: &no }), [], "the same question again");
        let two = [item("conv-1", "Fix the build", &["0", "1"])];
        assert_eq!(
            deck.running(3, &two, &settings, &Moment { attention: &normal, focused: &no }).len(),
            1,
            "a new question of the same conversation: no tray change, one notice"
        );
    }

    #[test]
    fn a_notification_follows_the_settings_and_what_windows_allows_exactly_as_the_tray_delivers() {
        // (attention, notifications, sound) -> (toast, sound), the table the tray's `delivery` already pins.
        use Attention::{Normal, Silent, SoundOnly};
        for (attention, notifications, sound, expected) in [
            (Normal, true, true, Some((true, true))),
            (Normal, true, false, Some((true, false))),
            (Normal, false, true, Some((false, true))),
            (Normal, false, false, None),
            (SoundOnly, true, true, Some((false, true))),
            (SoundOnly, true, false, None),
            (Silent, true, true, None),
        ] {
            let settings = Settings { notifications: Some(notifications), notification_sound: Some(sound), ..Settings::default() };
            let mut deck = Deck::new(ShellTexts::default());
            let orders = deck.running(1, &[item("a", "A", &["0"])], &settings, &Moment { attention: &move || attention, focused: &no });
            let got = orders.iter().find_map(|o| match o {
                Order::Notify { toast, sound, .. } => Some((*toast, *sound)),
                _ => None,
            });
            assert_eq!(got, expected, "{attention:?} notifications {notifications} sound {sound}");
            assert!(matches!(orders[0], Order::Tray { .. }), "the tray follows the list whatever is delivered");
        }
    }

    #[test]
    fn a_game_that_comes_to_the_front_between_two_notices_is_noticed() {
        let calls = std::cell::Cell::new(0);
        let attention = || {
            calls.set(calls.get() + 1);
            if calls.get() == 1 { Attention::Normal } else { Attention::Silent }
        };
        let mut deck = Deck::new(ShellTexts::default());
        let orders = deck.running(
            1,
            &[item("a", "A", &["0"]), item("b", "B", &["0"])],
            &Settings::default(),
            &Moment { attention: &attention, focused: &no },
        );
        let notified: Vec<_> = orders.iter().filter(|o| matches!(o, Order::Notify { .. })).collect();
        assert_eq!(notified.len(), 1, "the second notice finds the game in front");
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn the_stop_of_keep_going_is_one_notice_under_the_same_rules() {
        let deck = Deck::new(ShellTexts::default());
        let orders = deck.halted(
            "c9",
            "Fix login",
            "reason.keepGoing.done",
            None,
            &Settings::default(),
            &Moment { attention: &normal, focused: &no },
        );
        assert_eq!(
            orders,
            [Order::Notify {
                title: "Fix login: kept going until nothing was left".into(),
                body: String::new(),
                conversation_id: Some("c9".into()),
                toast: true,
                sound: true
            }]
        );
        let off = Settings { notifications: Some(false), notification_sound: Some(false), ..Settings::default() };
        assert_eq!(deck.halted("c9", "Fix login", "reason.keepGoing.done", None, &off, &Moment { attention: &normal, focused: &no }), []);
        assert_eq!(
            deck.halted(
                "c9",
                "Fix login",
                "reason.keepGoing.done",
                None,
                &Settings::default(),
                &Moment { attention: &|| Attention::Silent, focused: &no }
            ),
            []
        );
    }
}
