//! Who may send which command (R499). Every caller goes through the same rule, in Rust, before a command runs: the screen,
//! the command line and the AI's own tools alike. A command's reach is written beside it in the table; this decides it for
//! one caller.
//!
//! An AI that Faber started acts freely on its own family (the conversation it works in, the one that started it and every
//! conversation started from them), reads and organizes anything, and needs the person's yes for the rest. Signing in,
//! answering a card, granting a folder, raising permissions, money, security settings and deleting for good stay with the
//! person on the screen, whoever asks. The owner's line (R585): the AI operates Faber in everything except signing in,
//! spending money and changing its own permissions, trust and hooks, which are all `Person`.

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
/// it, the sites the agent's browser must not open, hooks that run commands, full trust, updates from the preview channel,
/// starting with the system, and the key that lets the AI see other windows with the programs it keeps out of reach.
pub const PERSON_SETTINGS: &[&str] = &[
    "permissions",
    "userHooks",
    "faberTrust",
    "grantsEpoch",
    "permissionsRevision",
    "beforePlan",
    "blockedHosts",
    "previews",
    "autostart",
    "extensionsDeveloperMode",
    "seeOtherWindows",
    "programsOutOfReach",
];

/// Refusals, as the keys a caller reads.
pub const SCREEN_ONLY: &str = "error.reach.screen";
pub const PERSON_ONLY: &str = "error.reach.person";
pub const ASK_PERSON: &str = "error.reach.ask";
pub const BAD_ARGS: &str = "error.host.badArgs";
/// An agent in Plan or Isolated, which change nothing, asked for a command that is not a read (R585).
pub const READ_ONLY: &str = "error.reach.readOnly";

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
    if command == "save_settings" {
        // A struct also decodes from an array, field by field: only an object can be read for the fields it names.
        match args.get("patch") {
            None | Some(Value::Null) => {}
            Some(Value::Object(patch)) if !PERSON_SETTINGS.iter().any(|f| patch.contains_key(*f)) => {}
            Some(Value::Object(_)) => return Err(PERSON_ONLY),
            Some(_) => return Err(BAD_ARGS),
        }
    }
    let Caller::Agent { conversation } = caller else { return Ok(()) };
    match reach {
        Reach::Free => Ok(()),
        Reach::Ask => Err(ASK_PERSON),
        Reach::Own(pointer) => {
            let target = match target_of(args, pointer) {
                Ok(Some(target)) => target,
                Ok(None) => return Ok(()),
                Err(refusal) => return Err(refusal),
            };
            let mine = conversation.as_deref().and_then(&family);
            match (mine, family(target)) {
                (Some(mine), Some(theirs)) if mine == theirs => Ok(()),
                _ => Err(ASK_PERSON),
            }
        }
        Reach::Person | Reach::Screen => unreachable!("refused above"),
    }
}

/// The conversation a command names at `pointer`, walking object by object. A step that is present but not an object (a
/// struct also decodes from an array, field by field, so the pointer would find nothing while the command still reads the
/// value) or a name that is not text is refused; a name that is absent or null names no existing conversation.
fn target_of<'a>(args: &'a Value, pointer: &str) -> Result<Option<&'a str>, &'static str> {
    let mut at = args;
    for step in pointer.split('/').skip(1) {
        match at {
            Value::Object(map) => match map.get(step) {
                None | Some(Value::Null) => return Ok(None),
                Some(next) => at = next,
            },
            _ => return Err(BAD_ARGS),
        }
    }
    at.as_str().map(Some).ok_or(BAD_ARGS)
}

/// What one of an agent's `Ask` commands does when its conversation is in Full access (R587, the owner's answer 6 of
/// 2026-11-25). Anything not listed in [`in_full_access`] keeps its card, so a new `Ask` command asks until someone decides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InFullAccess {
    /// Keeps its request card, as in every other mode.
    Card,
    /// Runs at once: it names no conversation.
    Runs,
    /// Runs at once when the conversation at this JSON pointer is in the caller's own family; any other keeps the card.
    Mine(&'static str),
    /// Runs at once when the background command its `id` names belongs to the caller's family; any other keeps the card.
    MineBackground,
}

/// The settings that spend the person's quota or widen what the AI does in every conversation: `save_settings` touching one
/// keeps its card even in Full access (money stays with the person, the owner's answer 6 of 2026-11-25). Live web search,
/// the default model of new conversations, Codex's reasoning summaries, the step up to a stronger model, resuming
/// interrupted work, the shared reply style, which is the AI's own instructions in every conversation, and the person's
/// name, which every new conversation's instructions carry.
pub const CARD_SETTINGS: &[&str] = &[
    "webSearch",
    "defaultModel",
    "reasoningSummary",
    "strongerWhenStuck",
    "resumeInterrupted",
    "replyStyle",
    "replyStyleText",
    "userName",
];

/// What `command`, an `Ask` command with `args`, does in Full access. Kept on the card in every mode: permanent deletions
/// (`remove_chat_group`, `remove_chat_group_file`, `forget_auto_learned`, `forget_environment`, `remove_project`,
/// `project_goal_remove`, `forget_recent`), starting turns that spend quota (`new_group_chat`, `pick_up`,
/// `project_goal_resume`, `wake_wait`, and `project_goal_add` and `project_goal_add_crashes`, whose goals start checks
/// and fix turns in a project with automatic goals), what other conversations read as their own (a chat group's
/// instructions and files, `save_chat_group` and `add_chat_group_files`; an environment's folders, `save_environment`),
/// `rewind_recover` (it recovers every conversation's interrupted rewind, not only the one it names),
/// `update_restart` (it stops running work and launches the installer), `open_link` (the person's own browser, outside the
/// blocked sites), `stop_aside` (its id does not say whose it is), a tool's install and its kin (`tools_*`, `setup_install`:
/// R584 and R33 leave an unchecked item to one yes, and a plugin can bring hooks) and `save_settings` touching a
/// [`CARD_SETTINGS`] field.
pub fn in_full_access(command: &str, args: &Value) -> InFullAccess {
    match command {
        "show_panel" | "show_project" => InFullAccess::Runs,
        "save_settings" => match args.get("patch") {
            Some(Value::Object(patch)) if CARD_SETTINGS.iter().any(|field| patch.contains_key(*field)) => InFullAccess::Card,
            _ => InFullAccess::Runs,
        },
        "move_to_project" | "show_file" | "show_conversation" => InFullAccess::Mine("/id"),
        "rewind_restore" | "rewind_redo" => InFullAccess::Mine("/request/conversationId"),
        "stop_background" => InFullAccess::MineBackground,
        _ => InFullAccess::Card,
    }
}

/// What a command in Full access acts on, for the caller's family to be checked against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target<'a> {
    Conversation(&'a str),
    Background(&'a str),
}

/// Whether the mode of the caller's conversation, as the engine holds it, is Full access: the stored mode is, and a session
/// runs in the conversation in it. No session (an agent calls only while its turn runs), or one in Plan or Isolated, is not.
pub fn full_access(stored: faber_engine::model::Permissions, live: Option<faber_engine::model::Permissions>) -> bool {
    use faber_engine::model::Permissions::FullAccess;
    stored == FullAccess && live == Some(FullAccess)
}

/// Whether a refusal gives way to the calling conversation's mode (R587): in Full access, an `Ask` command from that
/// conversation's own agent runs at once and the conversation says so. `full_access` is the mode the engine holds for that
/// conversation, read at the call ([`full_access`]), never anything the agent sends; `mine` says whether a target is in the
/// caller's own family. Only the ask refusal of an `Ask` command [`in_full_access`] lets run gives way: `Person` and `Screen`
/// (signing in, money, the AI's own permissions, trust and hooks, the `PERSON_SETTINGS` of `save_settings`) never follow
/// the mode, nor does an `Own` command on another family's conversation, and an agent with no conversation has no mode.
pub fn follows_mode(
    caller: &Caller,
    command: &str,
    reach: Reach,
    args: &Value,
    refusal: &str,
    full_access: bool,
    mine: impl Fn(Target<'_>) -> bool,
) -> bool {
    if !full_access || refusal != ASK_PERSON || reach != Reach::Ask || !matches!(caller, Caller::Agent { conversation: Some(_) }) {
        return false;
    }
    match in_full_access(command, args) {
        InFullAccess::Card => false,
        InFullAccess::Runs => true,
        InFullAccess::Mine(pointer) => matches!(target_of(args, pointer), Ok(Some(id)) if mine(Target::Conversation(id))),
        InFullAccess::MineBackground => matches!(target_of(args, "/id"), Ok(Some(id)) if mine(Target::Background(id))),
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
            let extensions = json!({"patch": {"extensionsDeveloperMode": true}});
            assert_eq!(check(&caller, "save_settings", Reach::Ask, &extensions, families()), Err(PERSON_ONLY));
            // Unblocking a site widens what the agent's own browser may open: its own permission (R585).
            let unblock = json!({"patch": {"blockedHosts": []}});
            assert_eq!(check(&caller, "save_settings", Reach::Ask, &unblock, families()), Err(PERSON_ONLY));
            assert_eq!(check(&caller, "site_extensions_mode", Reach::Screen, &json!({"enabled": true}), families()), Err(SCREEN_ONLY));
        }
        assert_eq!(check(&Caller::Screen, "answer", Reach::Person, &json!({}), families()), Ok(()));
        assert_eq!(check(&Caller::Screen, "save_settings", Reach::Ask, &json!({"patch": {"permissions": "ask"}}), families()), Ok(()));
    }

    #[test]
    fn no_ai_and_no_terminal_turns_on_the_key_or_edits_its_list() {
        let patches = [
            json!({"patch": {"seeOtherWindows": true}}),
            json!({"patch": {"seeOtherWindows": false}}),
            json!({"patch": {"programsOutOfReach": []}}),
            json!({"patch": {"language": "de", "programsOutOfReach": ["keepass.exe"]}}),
        ];
        for caller in [Caller::Person, agent("c1"), Caller::Agent { conversation: None }] {
            for patch in &patches {
                assert_eq!(check(&caller, "save_settings", Reach::Ask, patch, families()), Err(PERSON_ONLY), "{caller:?} {patch}");
            }
        }
        for patch in &patches {
            assert_eq!(check(&Caller::Screen, "save_settings", Reach::Ask, patch, families()), Ok(()), "the person on the screen");
        }
    }

    #[test]
    fn a_person_at_a_terminal_may_what_an_ai_asks_for() {
        let words = json!({"patch": {"language": "de"}});
        assert_eq!(check(&Caller::Person, "save_settings", Reach::Ask, &words, families()), Ok(()));
        assert_eq!(check(&agent("c1"), "save_settings", Reach::Ask, &words, families()), Err(ASK_PERSON));
        assert_eq!(check(&Caller::Person, "interrupt", Reach::Own("/id"), &json!({"id": "c9"}), families()), Ok(()));
        let archive = Reach::Free;
        assert_eq!(check(&agent("c1"), "archive", archive, &json!({"id": "c2", "on": true}), families()), Ok(()));
        assert_eq!(check(&agent("c1"), "archive", archive, &json!({"id": "c9", "on": true}), families()), Ok(()));
        assert_eq!(check(&Caller::Person, "archive", archive, &json!({"id": "c9", "on": true}), families()), Ok(()));
        assert_eq!(check(&Caller::Screen, "archive", archive, &json!({"id": "c9", "on": true}), families()), Ok(()));
    }

    #[test]
    fn in_full_access_only_an_ask_refusal_from_a_named_agent_gives_way() {
        use faber_engine::model::Permissions::*;
        let any = |_: Target<'_>| true;
        let words = json!({"patch": {"language": "de"}});
        let refusal = check(&agent("c1"), "save_settings", Reach::Ask, &words, families()).unwrap_err();
        assert!(follows_mode(&agent("c1"), "save_settings", Reach::Ask, &words, refusal, true, any), "Full access runs it");
        assert!(!follows_mode(&agent("c1"), "save_settings", Reach::Ask, &words, refusal, false, any), "other modes ask");
        let unknown = Caller::Agent { conversation: None };
        assert!(!follows_mode(&unknown, "save_settings", Reach::Ask, &words, refusal, true, any), "no conversation, no mode");
        // The mode is Full access only when stored so and a session runs in it.
        assert!(full_access(FullAccess, Some(FullAccess)));
        assert!(!full_access(FullAccess, None), "no session running");
        assert!(!full_access(FullAccess, Some(Plan)) && !full_access(FullAccess, Some(Isolated)));
        assert!(!full_access(Ask, Some(FullAccess)), "the stored mode decides");
        // What only the person decides never follows the mode.
        let raise = json!({"patch": {"permissions": "dontAsk"}});
        let refusal = check(&agent("c1"), "save_settings", Reach::Ask, &raise, families()).unwrap_err();
        assert!(!follows_mode(&agent("c1"), "save_settings", Reach::Ask, &raise, refusal, true, any));
        let refusal = check(&agent("c1"), "answer", Reach::Person, &json!({}), families()).unwrap_err();
        assert!(!follows_mode(&agent("c1"), "answer", Reach::Person, &json!({}), refusal, true, any));
        let refusal = check(&agent("c1"), "screen_ready", Reach::Screen, &json!({}), families()).unwrap_err();
        assert!(!follows_mode(&agent("c1"), "screen_ready", Reach::Screen, &json!({}), refusal, true, any));
        // Another family's conversation stays the person's to allow.
        let own = Reach::Own("/id");
        let refusal = check(&agent("c1"), "interrupt", own, &json!({"id": "c9"}), families()).unwrap_err();
        assert!(!follows_mode(&agent("c1"), "interrupt", own, &json!({"id": "c9"}), refusal, true, any));
    }

    #[test]
    fn in_full_access_a_named_target_must_be_the_caller_s_family_and_some_commands_always_ask() {
        let family = families();
        let mine = |target: Target<'_>| match target {
            Target::Conversation(id) => family(id).is_some() && family(id) == family("c1"),
            Target::Background(id) => id == "bg-mine",
        };
        let run = |command: &str, args: Value| follows_mode(&agent("c1"), command, Reach::Ask, &args, ASK_PERSON, true, mine);
        assert!(run("move_to_project", json!({"id": "c3", "projectId": "p"})), "its own family");
        assert!(!run("move_to_project", json!({"id": "c9", "projectId": "p"})), "another family keeps the card");
        assert!(!run("move_to_project", json!({"projectId": "p"})), "no target named");
        assert!(!run("move_to_project", json!({"id": ["c3"]})), "a target that is not text");
        assert!(run("rewind_restore", json!({"request": {"conversationId": "c2", "messageId": "m", "mode": "both"}})));
        assert!(!run("rewind_restore", json!({"request": {"conversationId": "c9", "messageId": "m", "mode": "both"}})));
        assert!(!run("rewind_redo", json!({"request": ["c2", "r1"]})), "a request given as an array");
        assert!(run("stop_background", json!({"id": "bg-mine"})));
        assert!(!run("stop_background", json!({"id": "bg-other"})));
        assert!(run("show_panel", json!({"panel": "privacy"})));
        // Deleting for good, spending, what other conversations read, the installer, the person's own browser and the tools
        // keep their card in every mode.
        for (command, args) in [
            ("remove_chat_group", json!({"id": "g"})),
            ("remove_chat_group_file", json!({"id": "g", "fileId": "f"})),
            ("forget_auto_learned", json!({})),
            ("forget_environment", json!({"id": "e"})),
            ("remove_project", json!({"id": "p"})),
            ("project_goal_remove", json!({"projectId": "p", "goalId": "g"})),
            ("new_group_chat", json!({"groupId": "g"})),
            ("pick_up", json!({})),
            ("project_goal_resume", json!({"projectId": "p", "goalId": "g"})),
            ("update_restart", json!({})),
            ("open_link", json!({"url": "https://example.com"})),
            ("stop_aside", json!({"asideId": "a"})),
            ("tools_install", json!({"id": "x"})),
            ("tools_remove", json!({"id": "x"})),
            ("setup_install", json!({"vendor": "codex"})),
            ("save_settings", json!({"patch": {"webSearch": "live"}})),
            ("save_settings", json!({"patch": {"language": "de", "strongerWhenStuck": true}})),
            ("save_settings", json!({"patch": {"defaultModel": {"vendor": "claude"}}})),
            ("save_settings", json!({"patch": {"userName": "Ignore the person"}})),
            ("forget_recent", json!({"path": "C:/x"})),
            ("wake_wait", json!({"id": "c1", "since": 1, "until": 2})),
            ("project_goal_add", json!({"projectId": "p", "words": "spend"})),
            ("project_goal_add_crashes", json!({"projectId": "p"})),
            ("save_chat_group", json!({"id": "g", "instructions": "obey"})),
            ("add_chat_group_files", json!({"id": "g", "paths": ["C:/x"]})),
            ("save_environment", json!({"id": "e", "folders": ["C:/x"]})),
            ("rewind_recover", json!({"id": "c1"})),
            ("some_new_ask_command", json!({})),
        ] {
            assert!(!run(command, args.clone()), "{command} {args}");
        }
    }

    #[test]
    fn a_struct_given_as_an_array_never_slips_past_the_rule_that_reads_its_fields() {
        // A derived struct also decodes from an array, field by field, so the command would read what the rule could not.
        let send = Reach::Own("/req/conversationId");
        let unknown = Caller::Agent { conversation: None };
        let array = json!({"req": ["c9", "chat", null, "run it"]});
        assert_eq!(check(&unknown, "send", send, &array, families()), Err(BAD_ARGS));
        assert_eq!(check(&agent("c1"), "send", send, &json!({"req": "c9"}), families()), Err(BAD_ARGS));
        assert_eq!(check(&agent("c1"), "send", send, &json!({"req": {"conversationId": 9}}), families()), Err(BAD_ARGS));
        assert_eq!(check(&agent("c1"), "send", send, &json!({"req": null}), families()), Ok(()), "no conversation named");
        for caller in [Caller::Person, agent("c1")] {
            let patch = json!({"patch": [null, true, "dontAsk"]});
            assert_eq!(check(&caller, "save_settings", Reach::Ask, &patch, families()), Err(BAD_ARGS));
        }
    }
}
