//! Who may send which command (R-next). Every caller goes through the same rule, in Rust, before a command runs: the screen,
//! the command line and the AI's own tools alike. A command's reach is written beside it in the table; this decides it for
//! one caller.
//!
//! An AI that Faber started acts freely on its own family (the conversation it works in, the one that started it and every
//! conversation started from them), reads and organizes anything, and needs the person's yes for the rest. Signing in,
//! answering a card, granting a folder, raising permissions, money, security settings and deleting for good stay with the
//! person on the screen, whoever asks.

use serde::Serialize;
use serde_json::Value;

/// Who sends a command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Caller {
    /// The window's own screen: the person, with everything Faber shows them.
    Screen,
    /// A person at a terminal, or a program of theirs that Faber did not start: everything but what only the screen may do.
    Person,
    /// An AI that Faber started, with the conversation it works in when that is known. One whose conversation is not known
    /// has no family of its own.
    Agent { conversation: Option<String> },
}

/// Who may send a command, as the table writes it beside the command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    /// Anyone: reading, and organizing that is undone as easily (archive, pin, rename).
    Free,
    /// Acts on the conversation the JSON pointer names in the arguments: an AI may in its own family, and asks the person
    /// for any other. A missing conversation means a new one, which any caller may start.
    Own(&'static str),
    /// An AI needs the person's yes; a person at a terminal may.
    Ask,
    /// Only the person, on the screen.
    Person,
    /// The screen's own plumbing (what it shows, its first paint): never sent from anywhere else.
    Screen,
}

impl Reach {
    /// The word the help and the AI's tools show.
    pub fn word(self) -> &'static str {
        match self {
            Reach::Free => "free",
            Reach::Own(_) => "own",
            Reach::Ask => "ask",
            Reach::Person => "person",
            Reach::Screen => "screen",
        }
    }

    /// Whether anything but the screen may ever send it: the command line's help and the AI's tools list only these.
    pub fn exported(self) -> bool {
        !matches!(self, Reach::Screen)
    }
}

/// The settings only the person changes on the screen, even through `save_settings`: the permission mode and what decides
/// it, hooks that run commands, full trust, updates from the preview channel and starting with the system.
pub const PERSON_SETTINGS: &[&str] =
    &["permissions", "userHooks", "faberTrust", "grantsEpoch", "permissionsRevision", "beforePlan", "previews", "autostart"];

/// Refusals, as the keys a caller reads.
pub const SCREEN_ONLY: &str = "error.reach.screen";
pub const PERSON_ONLY: &str = "error.reach.person";
pub const ASK_PERSON: &str = "error.reach.ask";

/// Whether `caller` may send `command` with `args`. `family` gives the root of a conversation's family (None for one that
/// does not exist), so an AI's own family is told apart from the others.
pub fn check(
    caller: &Caller,
    command: &str,
    reach: Reach,
    args: &Value,
    family: impl Fn(&str) -> Option<String>,
) -> Result<(), &'static str> {
    if *caller == Caller::Screen {
        return Ok(());
    }
    match reach {
        Reach::Screen => return Err(SCREEN_ONLY),
        Reach::Person => return Err(PERSON_ONLY),
        _ => {}
    }
    if command == "save_settings" && args["patch"].as_object().is_some_and(|patch| PERSON_SETTINGS.iter().any(|f| patch.contains_key(*f))) {
        return Err(PERSON_ONLY);
    }
    let Caller::Agent { conversation } = caller else { return Ok(()) };
    match reach {
        Reach::Free => Ok(()),
        Reach::Ask => Err(ASK_PERSON),
        Reach::Own(pointer) => {
            let Some(target) = args.pointer(pointer).and_then(Value::as_str) else { return Ok(()) };
            let mine = conversation.as_deref().and_then(&family);
            match (mine, family(target)) {
                (Some(mine), Some(theirs)) if mine == theirs => Ok(()),
                _ => Err(ASK_PERSON),
            }
        }
        Reach::Person | Reach::Screen => unreachable!("refused above"),
    }
}

/// The root of `id`'s family: following who started each conversation up to the first one, given each conversation's
/// starter. A loop or a missing starter ends the walk where it is.
pub fn root(id: &str, started_by: impl Fn(&str) -> Option<Option<String>>) -> Option<String> {
    let mut at = id.to_string();
    started_by(&at)?;
    for _ in 0..64 {
        match started_by(&at).flatten() {
            Some(parent) if started_by(&parent).is_some() => at = parent,
            _ => break,
        }
    }
    Some(at)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;

    /// c1 started c2, which started c3; c9 stands alone.
    fn families() -> impl Fn(&str) -> Option<String> {
        let starters: HashMap<&str, Option<&str>> =
            [("c1", None), ("c2", Some("c1")), ("c3", Some("c2")), ("c9", None)].into_iter().collect();
        move |id| root(id, |at| starters.get(at).map(|s| s.map(String::from)))
    }

    fn agent(of: &str) -> Caller {
        Caller::Agent { conversation: Some(of.into()) }
    }

    #[test]
    fn a_family_is_everything_under_its_first_conversation() {
        let family = families();
        assert_eq!(family("c3").as_deref(), Some("c1"));
        assert_eq!(family("c9").as_deref(), Some("c9"));
        assert_eq!(family("gone"), None);
        let looped: HashMap<&str, Option<&str>> = [("a", Some("b")), ("b", Some("a"))].into_iter().collect();
        assert!(root("a", |at| looped.get(at).map(|s| s.map(String::from))).is_some(), "a loop ends");
    }

    #[test]
    fn an_ai_drives_its_own_family_and_asks_for_the_others() {
        let own = Reach::Own("/id");
        assert_eq!(check(&agent("c3"), "interrupt", own, &json!({"id": "c1"}), families()), Ok(()));
        assert_eq!(check(&agent("c1"), "interrupt", own, &json!({"id": "c2"}), families()), Ok(()));
        assert_eq!(check(&agent("c1"), "interrupt", own, &json!({"id": "c9"}), families()), Err(ASK_PERSON));
        assert_eq!(check(&agent("c1"), "interrupt", own, &json!({"id": "gone"}), families()), Err(ASK_PERSON));
        let unknown = Caller::Agent { conversation: None };
        assert_eq!(check(&unknown, "interrupt", own, &json!({"id": "c1"}), families()), Err(ASK_PERSON), "no family of its own");
        let send = Reach::Own("/req/conversationId");
        assert_eq!(check(&unknown, "send", send, &json!({"req": {"text": "hi"}}), families()), Ok(()), "a new conversation");
        assert_eq!(check(&agent("c2"), "send", send, &json!({"req": {"conversationId": "c3"}}), families()), Ok(()));
    }

    #[test]
    fn what_only_the_person_decides_stays_on_the_screen_whoever_asks() {
        for caller in [Caller::Person, agent("c1")] {
            assert_eq!(check(&caller, "answer", Reach::Person, &json!({}), families()), Err(PERSON_ONLY));
            assert_eq!(check(&caller, "screen_ready", Reach::Screen, &json!({}), families()), Err(SCREEN_ONLY));
            let raise = json!({"patch": {"language": "de", "permissions": "dontAsk"}});
            assert_eq!(check(&caller, "save_settings", Reach::Ask, &raise, families()), Err(PERSON_ONLY));
        }
        assert_eq!(check(&Caller::Screen, "answer", Reach::Person, &json!({}), families()), Ok(()));
        assert_eq!(check(&Caller::Screen, "save_settings", Reach::Ask, &json!({"patch": {"permissions": "ask"}}), families()), Ok(()));
    }

    #[test]
    fn a_person_at_a_terminal_may_what_an_ai_asks_for() {
        let words = json!({"patch": {"language": "de"}});
        assert_eq!(check(&Caller::Person, "save_settings", Reach::Ask, &words, families()), Ok(()));
        assert_eq!(check(&agent("c1"), "save_settings", Reach::Ask, &words, families()), Err(ASK_PERSON));
        assert_eq!(check(&Caller::Person, "interrupt", Reach::Own("/id"), &json!({"id": "c9"}), families()), Ok(()));
        let archive = Reach::Own("/id");
        assert_eq!(check(&agent("c1"), "archive", archive, &json!({"id": "c2", "on": true}), families()), Ok(()));
        assert_eq!(check(&agent("c1"), "archive", archive, &json!({"id": "c9", "on": true}), families()), Err(ASK_PERSON));
        assert_eq!(check(&Caller::Person, "archive", archive, &json!({"id": "c9", "on": true}), families()), Ok(()));
        assert_eq!(check(&Caller::Screen, "archive", archive, &json!({"id": "c9", "on": true}), families()), Ok(()));
    }
}
