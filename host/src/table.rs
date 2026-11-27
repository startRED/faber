//! Every command Faber answers, with its arguments and what it does to the person's work (`Kind`). The names and the
//! camelCase keys are the ones `ui/api.ts` sends.

use super::*;
use crate::queues::{PromptReads, SearchQueue};
use crate::{folder, look, project_commands, setup, terminal, tools, views};
use base64::Engine as _;
use faber_engine::model::{Area, Conversation, Project, Role};
use faber_engine::project::checks;
use faber_engine::{AppState, BranchRequest, SendRequest, WarmRequest};
use look::{Look, ThemeFile};
use serde_json::Map;
use std::path::{Path, PathBuf};
use std::time::Duration;
use views::ListState;

/// The panels `show_panel` opens: the Settings sections and the Catalog.
const SHOWN_PANELS: &[&str] = &["general", "projects", "appearance", "accounts", "privacy", "catalog"];

/// What `state` answers: the engine's state, with the look of the saved appearance (the screen applies it before it renders),
/// the models this account can use, and the version running.
#[derive(Serialize)]
struct FullState {
    #[serde(flatten)]
    state: AppState,
    look: Look,
    models: Vec<ModelInfo>,
    #[serde(rename = "antigravityModels", skip_serializing_if = "Option::is_none")]
    antigravity_models: Option<Vec<ModelInfo>>,
    version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    sharing: Option<SharingState>,
    #[serde(rename = "storageChoice", skip_serializing_if = "Option::is_none")]
    storage_choice: Option<crate::storage_choice::Status>,
}

#[derive(Serialize)]
struct SharingState {
    choice: Option<bool>,
}

fn sharing_state_for(sharing: &faber_engine::sharing::Sharing) -> Option<SharingState> {
    faber_engine::sharing::endpoint().map(|_| SharingState { choice: sharing.choice() })
}

fn scope_app_state(state: &mut AppState, by: Option<&str>, family: impl Fn(&str) -> Option<String>) {
    crate::keep_family(&mut state.background, by, &family);
    let owner = by.and_then(&family);
    let allowed = |id: &str| owner.as_ref().is_some_and(|root| family(id).as_ref() == Some(root));
    state.running.items.retain(|item| allowed(&item.conversation_id));
}

/// What `save_settings` answers: the settings as stored, and the look computed from the stored appearance.
#[derive(Serialize)]
struct SavedSettings {
    settings: Settings,
    look: Look,
}

const IMAGE_TYPES: [(&str, &str); 6] = [
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("gif", "image/gif"),
    ("webp", "image/webp"),
    ("svg", "image/svg+xml"),
];
static LISTING_CHANGES: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static READING_PATCH: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

const JSON_FILES: Filter = Filter { name: "JSON", extensions: &["json"] };
const MARKDOWN_FILES: Filter = Filter { name: "Markdown", extensions: &["md"] };

fn save_copy(mut source: impl std::io::Read, target: &Path) -> std::io::Result<()> {
    look::replace_file(target, |file| std::io::copy(&mut source, file).map(|_| ()))
}

/// Runs `work` off the async threads. `lost` is the error key if the work panicked.
async fn off<T: Send + 'static>(lost: &str, work: impl FnOnce() -> T + Send + 'static) -> Res<T> {
    let flag = request_flag();
    tokio::task::spawn_blocking(move || {
        check_revoked(flag.as_deref())?;
        Ok(work())
    })
    .await
    .map_err(|_| lost.to_string())?
}

/// What a dialog gave, as the error key of the command that asked: `not_local` for a path that is not on this computer.
fn picked(result: Picked, not_local: &str) -> Result<Option<PathBuf>, Failure> {
    result.map_err(|why| Failure::key(if why == PickError::NoWindow { NO_WINDOW } else { not_local }))
}

/// The asking AI's conversation (`by`), or `error.session.agentOnly` when the caller is not an AI in a conversation.
fn agent_by(by: Option<String>) -> Result<String, Failure> {
    by.filter(|s| !s.trim().is_empty()).ok_or_else(|| Failure::key("error.session.agentOnly"))
}

/// An English refusal sentence from one of `Engine::session_*`, wrapped for the host table.
fn refused(why: String) -> Failure {
    Failure::with("error.session.refused", why)
}

/// Only letters, digits, spaces, hyphens and underscores of a title or a theme's name make a file name.
fn file_stem(name: &str) -> String {
    name.chars().filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_')).collect()
}

/// A picture as a data URL when it is a known kind and at most `max` bytes; a picture that cannot be read only has no preview.
fn data_url(source: &Path, kind: &str, max: u64) -> Option<String> {
    if !std::fs::metadata(source).is_ok_and(|m| m.len() <= max) {
        return None;
    }
    let bytes = std::fs::read(source).ok()?;
    Some(format!("data:{kind};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}

table! {
    /// This conversation's requests and their current state.
    Read Free fn asks(h) -> Vec<Value> {
        Ok(lock(&h.asks).pending_details())
    }
    /// Allow one identical retry, or decline the request.
    Write Person fn answer_ask(h, id: String, allow: bool) -> bool {
        let (answered, pending) = {
            let mut book = lock(&h.asks);
            let answered = book.answer(&id, allow);
            (answered, book.pending())
        };
        if answered { h.announce(&json!({"type": "asks", "asks": pending})); }
        Ok(answered)
    }
    /// Starts a private Chat record; send uses the returned id.
    Write Screen fn start_temporary_chat(h) -> String {
        h.engine.start_temporary_chat()
    }
    /// Leaving an ordinary conversation never deletes it.
    Write Screen fn close_chat(h, id: String) -> () {
        h.engine.close_chat(&id)?;
        h.engine.media_close(&id);
        Ok(())
    }
    Read Screen fn open(h, id: String) -> Option<Conversation> {
        // Whatever it heard of a streaming reply, the screen now takes this snapshot: the next update of each goes whole.
        lock(&h.feed).forget();
        Ok(h.engine.open_for_screen_window(&id, 40))
    }
    Read Screen fn start_card(h, id: String) -> Option<String> {
        Ok(h.engine.start_card(&id))
    }
    /// The previous screen page, oldest first; message ids keep the boundary stable while a reply grows.
    Read Screen fn messages_before(h, id: String, message_id: String) -> Option<Vec<faber_engine::model::Message>> {
        Ok(h.engine.messages_before(&id, &message_id, 40))
    }
    /// The conversations, the running ones first, then pinned, then the newest, each with its state in plain words
    /// (`working`, `waiting`, `idle`). `state` can also select an exact ending (`concluded`, `ended`, `interrupted`,
    /// `stopped`, `failed`, `new`); matching rows show that state. `running` means working or waiting. `archived` true
    /// lists only archived ones and false every one; `family` keeps the family of a conversation (who started whom);
    /// `query` keeps titles holding its words. Uses list metadata, not message bodies.
    Read Free fn conversations(
        h,
        state: Option<Vec<ListState>>,
        archived: Option<bool>,
        project_id: Option<String>,
        family: Option<String>,
        query: Option<String>,
        limit: Option<usize>,
    ) -> Vec<views::Row> {
        let states = state.unwrap_or_default().into_iter().map(String::from).collect();
        let filter = views::Filter { states, archived, project_id, family, query, limit };
        let (summaries, running) = h.engine.conversation_list();
        Ok(views::rows(&summaries, &running, &filter))
    }
    /// One conversation: its state (`concluded` only when its AI concluded the whole request), its last `last` messages (all
    /// when missing) with the tools each used, and the AI's latest conclusion. For numbered messages, pass `from` (first
    /// is 1) and optionally `limit` (1 to 10, default 1) instead of `last`. `count` always holds the total. Unlike opening
    /// it on the screen, reading leaves its done mark for the person.
    Read Free fn conversation(h, id: String, last: Option<usize>, from: Option<usize>, limit: Option<usize>) -> views::Read {
        let (start, limit, first) = match (from, limit, last) {
            (None, None, Some(last)) => (None, last, None),
            (None, None, None) => (Some(0), usize::MAX, None),
            (Some(from), limit, None) if from > 0 && (1..=10).contains(&limit.unwrap_or(1)) => (Some(from - 1), limit.unwrap_or(1), Some(from)),
            _ => return Err("error.host.badArgs".into()),
        };
        let engine = h.engine.clone();
        off("error.conversationGone", move || {
            engine.view_history(&id, start, limit, |page, active, in_panel| views::read(page, active, in_panel, first))
        })
            .await?.ok_or_else(|| "error.conversationGone".into())
    }
    /// Message numbers, IDs, roles, times and statuses, without message bodies or tools. `from` starts at 1 and `limit`
    /// defaults to 20 (1 to 100). With `role`, `from` counts only that role. Pass each global `number` to `conversation` as `from`.
    /// `total` counts every message; `matching` counts the requested role, or every message with no filter.
    Read Free fn conversation_headers(
        h, id: String, from: Option<usize>, limit: Option<usize>, role: Option<Role>,
    ) -> faber_engine::history::Headers {
        let from = from.unwrap_or(1);
        let limit = limit.unwrap_or(20);
        if from == 0 || !(1..=100).contains(&limit) {
            return Err("error.host.badArgs".into());
        }
        let engine = h.engine.clone();
        off("error.conversationGone", move || engine.conversation_headers(&id, from - 1, limit, role))
            .await?.ok_or_else(|| "error.conversationGone".into())
    }
    /// Tool steps of one message in the caller's project. The caller must be an identified project agent.
    /// Number messages from 1. Omit `id` for the caller, and use `offset`, `limit` (1 to 50, default 10) or `failedOnly`
    /// to page steps. Or pass one `tool` ID to read a masked diagnostic of the caller or its direct child.
    Read Free fn conversation_steps(
        h,
        by: Option<String>,
        id: Option<String>,
        from: usize,
        limit: Option<usize>,
        offset: Option<usize>,
        tool: Option<String>,
        failed_only: Option<bool>,
    ) -> Value [Failure] {
        let reader = agent_by(by)?;
        let id = id.unwrap_or_else(|| reader.clone());
        let input = json!({"action":"steps","id":id,"from":from,"limit":limit,"offset":offset,"tool":tool,"failedOnly":failed_only});
        let engine = h.engine.clone();
        off("error.session.refused", move || engine.project_steps(&reader, &input)).await?.map_err(|why| refused(why.into()))
    }
    /// Recent background command endings in this Faber run, for an identified project agent's own conversation or direct child.
    /// Omit `id` for the caller. Page with `limit` (1 to 20, default 10) or select one execution `tool` ID for masked output.
    /// Set `runningOnly` to true for active command metadata without partial output.
    Read Free fn conversation_command_results(
        h, by: Option<String>, id: Option<String>, tool: Option<String>, limit: Option<usize>, running_only: Option<bool>,
    ) -> Value [Failure] {
        let reader = agent_by(by)?;
        let input = json!({"action":"command_results","id":id,"tool":tool,"limit":limit,"runningOnly":running_only});
        let engine = h.engine.clone();
        off("error.session.refused", move || engine.project_command_results(&reader, &input)).await?.map_err(|why| refused(why.into()))
    }
    /// The projects Faber knows, with their folders.
    Read Free fn projects(h) -> Vec<Project> {
        Ok(h.engine.projects())
    }
    /// Each vendor's quota windows (how much is used, when each resets) and what Faber has spent of them.
    Read Free fn quota(h) -> Value {
        let (quotas, usage) = h.engine.quota_view();
        Ok(json!({ "quotas": quotas, "usage": usage }))
    }
    /// The settings as Settings shows them.
    Read Free fn settings(h) -> Settings {
        Ok(h.shell.shown_settings(h.engine.settings()))
    }
    /// Hook previews are requested by the screen and never saved.
    Read Screen fn hook_list(h) -> Vec<faber_engine::claude::HookPreview> {
        let mut files = Vec::new();
        if let Some(dir) = faber_engine::claude_sessions::config_dir(&|key| std::env::var(key).ok()) {
            files.push(dir.join("settings.json"));
        }
        for project in h.engine.projects() {
            files.push(PathBuf::from(project.path).join(".claude").join("settings.json"));
        }
        off("error.settings.loadFailed", move || faber_engine::claude::list_hooks(&files)).await
    }
    /// Everything the screen draws its first frame from; the command line reads `conversations`, `projects` and `quota`.
    Read Screen fn state(h) -> FullState {
        lock(&h.feed).forget();
        let mut state = h.engine.state();
        state.settings = h.shell.shown_settings(std::mem::take(&mut state.settings));
        let look = look::stored(state.settings.appearance.as_ref());
        Ok(FullState { state, look, models: lock(&h.models).clone(), antigravity_models: lock(&h.antigravity_models).clone(), version: h.shell.version(), sharing: sharing_state_for(&h.sharing), storage_choice: Some(lock(&h.storage_choice).status()) })
    }
    /// Read-only bootstrap for a session-owned app renderer; never resets the person's streaming feed.
    Read Own("/by") fn app_control_state(h, by: Option<String>) -> FullState {
        let mut state = h.engine.state();
        let starters: std::collections::HashMap<_, _> =
            state.conversations.iter().map(|c| (c.id.clone(), c.started_by.clone())).collect();
        scope_app_state(&mut state, by.as_deref(), |id| reach::root(id, |at| starters.get(at).cloned()));
        state.settings = h.shell.shown_settings(std::mem::take(&mut state.settings));
        let look = look::stored(state.settings.appearance.as_ref());
        Ok(FullState { state, look, models: lock(&h.models).clone(), antigravity_models: lock(&h.antigravity_models).clone(), version: h.shell.version(), sharing: None, storage_choice: None })
    }
    #[cfg(faber_telemetry)]
    Read Screen fn sharing_state(h) -> SharingState {
        Ok(SharingState { choice: h.sharing.choice() })
    }
    #[cfg(faber_telemetry)]
    Write Screen fn sharing_set(h, yes: bool, language: String) -> SharingState {
        let sharing = Arc::clone(&h.sharing);
        let state = off("error.store.write", move || { sharing.set_choice(yes, Some(&language)).map_err(str::to_string)?; Ok::<_, String>(SharingState { choice: sharing.choice() }) }).await??;
        if yes {
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
            h.sharing.schedule(&h.runtime, now, false, Arc::new(faber_engine::sharing::HttpsTransport));
        }
        Ok(state)
    }
    #[cfg(faber_telemetry)]
    Write Screen fn sharing_language(h, language: String) -> () {
        let sharing = Arc::clone(&h.sharing);
        off("error.store.write", move || sharing.set_language(&language).map_err(str::to_string)).await?
    }
    #[cfg(faber_telemetry)]
    Read Screen fn sharing_preview(h) -> Option<String> {
        let sharing = Arc::clone(&h.sharing);
        off("error.store.read", move || sharing.preview().map_err(str::to_string)).await?
    }
    /// A session renderer may open only its own conversation family, without screen privileges.
    Read Own("/id") fn app_control_open(h, id: String) -> Option<Conversation> {
        Ok(h.engine.read_window(&id, 40))
    }
    Write Free fn create_chat_group(h, name: String) -> faber_engine::engine::ChatGroup {
        h.engine.create_chat_group(&name)
    }
    Write Ask fn save_chat_group(h, id: String, name: Option<String>, instructions: Option<String>) -> faber_engine::engine::ChatGroup {
        h.engine.save_chat_group(&id, name.as_deref(), instructions.as_deref())
    }
    Write Ask fn add_chat_group_files(h, id: String, files: Vec<faber_engine::workspace::Attachment>) -> faber_engine::engine::ChatGroup {
        h.engine.add_chat_group_files(&id, &files)
    }
    Write Ask fn remove_chat_group_file(h, id: String, file_id: String) -> faber_engine::engine::ChatGroup {
        h.engine.remove_chat_group_file(&id, &file_id)
    }
    Write Ask fn remove_chat_group(h, id: String) -> () {
        h.engine.remove_chat_group(&id)
    }
    Write Free fn move_chat_to_group(h, conversation_id: String, group_id: Option<String>) -> () {
        h.engine.move_chat_to_group(&conversation_id, group_id.as_deref())
    }
    StartsTurn Ask fn new_group_chat(h, group_id: String) -> Conversation {
        h.engine.new_group_chat(&group_id).map(Conversation::for_screen)
    }
    Read Screen fn set_clock(h, utc_offset: i64) -> () {
        h.engine.set_utc_offset(utc_offset);
        Ok(())
    }
    /// The window was shown, hidden or focused: the quota is read more often while it is open, and at once on focus.
    Read Screen fn quota_attention(h, visible: bool, focused: bool) -> () {
        h.engine.set_attention(visible, focused);
        Ok(())
    }
    Write Person fn answer_plan(h, vendor: Vendor, plan: String) -> () {
        h.engine.answer_plan(vendor, &plan);
        Ok(())
    }
    Write Person fn approve_credits(h, id: String, turn_id: String, vendor: Vendor) -> () {
        h.engine.approve_credits(&id, &turn_id, vendor)
    }
    Write Own("/id") fn move_turn(h, id: String, turn_id: String) -> () {
        h.engine.move_turn(&id, &turn_id)
    }
    /// The person read the requests held back in a reply: they leave the "needs you" list.
    Write Screen fn parked_seen(h, id: String, turn_id: String) -> () {
        h.engine.parked_seen(&id, &turn_id)
    }
    /// Leave a recoverable reply queued for its turn; this is not the `faber wait` conversation command.
    Write Own("/id") fn wait_turn(h, id: String, turn_id: String) -> () {
        h.engine.wait_turn(&id, &turn_id)
    }
    /// Go on with a reply Faber was closed under (R477): one fixed turn, refused once it is no longer the last.
    Write Own("/id") fn continue_closed(h, id: String, turn_id: String) -> () {
        h.engine.continue_closed(&id, &turn_id)
    }
    /// Recheck an overdue saved project wait, using its observed `since` and `until` from `waitingOn`.
    /// Uses the existing automatic admission and receipt. No message or override can be supplied.
    Write Ask fn wake_wait(h, id: String, since: u64, until: u64) -> faber_engine::engine::WaitCheck {
        if until <= since { return Err("error.host.badArgs".into()); }
        let engine = h.engine.clone();
        off("error.conversationGone", move || engine.wake_wait(&id, since, until)).await
    }
    Read Screen fn warm(h, req: WarmRequest) -> () {
        h.engine.warm(req);
        Ok(())
    }
    Write Ask fn forget_auto_learned(h) -> () {
        h.engine.forget_auto_learned()
    }
    Read Screen fn route_draft(h, req: WarmRequest, text: String) -> () {
        h.engine.route_draft(req, text);
        Ok(())
    }
    /// Stops a conversation's running turn. An AI may only stop a conversation it started; its own conversation, its
    /// parent and the person's conversations are refused.
    Write Own("/id") fn interrupt(h, id: String, by: Option<String>) -> () [Failure] {
        if let Some(by) = by {
            h.engine.session_stop(&by, &id).map(|_| ()).map_err(refused)
        } else {
            h.engine.interrupt(&id);
            Ok(())
        }
    }
    /// What a conversation delivered (files, checks, commits, answers) and what it cost; none before its first reply.
    Read Free fn results(h, id: String) -> Option<faber_engine::results::Results> {
        Ok(h.engine.results(&id).filter(|r| r.cost.turns > 0))
    }
    /// The same for a project: its total and each conversation's numbers, most spent first.
    Read Free fn project_results(h, project_id: String) -> faber_engine::results::ProjectResults {
        Ok(h.engine.project_results(&project_id))
    }
    Write Person fn answer(h, id: String, turn_id: String, request_id: String, allow: bool, always: bool) -> () {
        h.engine.answer(&id, &turn_id, &request_id, allow, always).await
    }

    Read Free fn search_messages(h, query: String, area: Area) -> faber_engine::search::Results {
        // As with mentions: one scan runs and only the latest waits. Superseded scans stop between conversation files.
        static SEARCHES: SearchQueue = SearchQueue::new();
        let Some((_one, cancelled)) = SEARCHES.acquire().await else {
            return Ok(faber_engine::search::Results { ready: true, hits: vec![] });
        };
        let engine = h.engine.clone();
        off("error.store.read", move || engine.search_messages_cancellable(&query, area, &cancelled).map_err(str::to_string)).await?
    }
    Read Free fn prompt_history(
        h,
        request_id: String,
        cancel: Option<bool>,
        conversation_id: Option<String>,
        area: Area,
        project_id: Option<String>,
    ) -> Vec<String> {
        static READS: std::sync::LazyLock<Mutex<PromptReads>> = std::sync::LazyLock::new(|| Mutex::new(PromptReads::default()));
        if cancel == Some(true) {
            lock(&READS).cancel(request_id);
            return Ok(vec![]);
        }
        let Some(cancelled) = lock(&READS).start(request_id.clone()) else { return Ok(vec![]) };
        let engine = h.engine.clone();
        let result = off("error.store.read", move || {
            engine.prompt_history_cancellable(conversation_id.as_deref(), area, project_id.as_deref(), &cancelled).map_err(str::to_string)
        })
        .await;
        lock(&READS).finish(&request_id);
        result?
    }
    StartsTurn Own("/req/conversationId") fn send(h, req: SendRequest) -> String {
        // A project conversation's first message makes its copy after the message is recorded (R201), on the engine's own
        // blocking thread; this call waits only for the engine's lock, away from the async workers.
        let engine = h.engine.clone();
        off("error.copy.failed", move || engine.send(req)).await?
    }
    /// The sessions started in a terminal in a project's folder (parity F5).
    Read Free fn cli_sessions(h, project_id: String) -> Vec<faber_engine::model::CliSession> {
        h.engine.cli_sessions(&project_id, Duration::from_secs(20)).await
    }
    /// Opens a session started in a terminal as a project conversation, or the one already opened for it.
    StartsTurn Ask fn pick_up(h, project_id: String, session: faber_engine::model::CliSession) -> String {
        let engine = h.engine.clone();
        off("error.conversationGone", move || engine.pick_up(&project_id, session)).await?
    }
    /// Person reach (R585): it starts a turn outside the conversation's own, which spends the person's quota or money.
    StartsTurn Person fn branch(h, req: BranchRequest) -> String {
        let engine = h.engine.clone();
        off("error.copy.failed", move || engine.branch(req)).await?
    }
    /// "Branch from here" on an answer: a new conversation with every message up to it, where nothing is sent yet. Person
    /// reach (R585): a new conversation outside the family is the person's to make.
    StartsTurn Person fn branch_here(h, conversation_id: String, message_id: String) -> String {
        let engine = h.engine.clone();
        off("error.copy.failed", move || engine.branch_here(&conversation_id, &message_id)).await?
    }
    /// The messages of a conversation that have other versions; the other conversations of its family are read from disk.
    Read Free fn versions(h, id: String) -> Vec<faber_engine::model::Versions> {
        let engine = h.engine.clone();
        off("error.messageGone", move || engine.versions(&id)).await
    }
    /// Pages and drawings, with the versions already held in the conversation's saved replies.
    Read Free fn visual_answers(h, id: String, message_id: Option<String>, block: Option<usize>) -> Vec<faber_engine::visual::VisualAnswer> {
        let engine = h.engine.clone();
        off("error.messageGone", move || engine.visual_answers_for_screen(&id, message_id.as_deref(), block)).await
    }
    Read Free fn visual_pieces(h, id: String, message_id: String) -> Vec<faber_engine::pieces::VisualPiece> {
        let engine = h.engine.clone();
        off("error.messageGone", move || engine.visual_pieces(&id, &message_id)).await
    }
    Write Own("/id") fn save_piece_value(h, id: String, message_id: String, block: usize, value: Value) -> () {
        let engine = h.engine.clone();
        off("error.store.write", move || engine.save_piece_value(&id, &message_id, block, value)).await?
    }
    Write Person fn approve_plan(h, id: String, turn_id: String, request_id: String, mode: Option<faber_engine::model::Permissions>) -> () {
        h.engine.approve_plan(&id, &turn_id, &request_id, mode).await
    }
    /// The person's choices for a question the agent asked: each question's text to the label picked.
    Write Person fn answer_question(h, id: String, turn_id: String, request_id: String, answers: Value) -> () {
        h.engine.answer_question(&id, &turn_id, &request_id, answers).await
    }
    Write Free fn rename(h, id: String, title: String) -> () {
        h.engine.rename(&id, &title);
        Ok(())
    }
    Write Person fn remove(h, id: String, discard: Option<bool>) -> () {
        // `error.copy.unsaved`: its copy holds work found nowhere else, and the screen asks before `discard` (R201).
        let engine = h.engine.clone();
        off("error.copy.failed", move || {
            engine.remove_with(&id, discard.unwrap_or(false))?;
            engine.media_close(&id);
            Ok(())
        }).await?
    }
    /// Archives a conversation, or brings an archived one back. An AI may only archive (`on: true`) a conversation of its
    /// own project that is not working, not waiting for the person and not pinned; unarchiving from an AI is refused.
    Write Free fn archive(h, id: String, on: bool, by: Option<String>) -> () [Failure] {
        if let Some(by) = by {
            if !on {
                return Err(refused("only the person can bring an archived conversation back".into()));
            }
            h.engine.session_archive(&by, &id).map(|_| ()).map_err(refused)
        } else {
            h.engine.archive(&id, on).map_err(Failure::from)
        }
    }
    /// Starts a new conversation in the same project for an independent piece of work and returns its id (the CLI exposes it as `.childId`). `ai` is `same`
    /// (default), `auto`, `claude` or `codex`; `model` and `effort` (`low`, `medium`, `high`) apply to a named AI or `same`,
    /// never `auto`. Refused outside an AI's conversation, from a conversation another AI started, or when the project cap
    /// or account quota is reached.
    StartsTurn Free fn start_conversation(
        h,
        by: Option<String>,
        title: String,
        brief: String,
        ai: Option<String>,
        model: Option<String>,
        effort: Option<String>,
        keep_going: Option<bool>,
        step: Option<String>,
    ) -> String [Failure] {
        let by = agent_by(by)?;
        let ai = faber_engine::sibling::Ai::parse(ai.as_deref().map(str::trim).filter(|s| !s.is_empty()).unwrap_or("same"))
            .map_err(|error| refused(error.into()))?;
        let new = faber_engine::engine::NewSession { title, brief, ai, model, effort, work: None, keep_going: keep_going.unwrap_or(false), step };
        h.engine.session_start(&by, new).map_err(refused)
    }
    /// Sends `message` to another unarchived conversation of the same project as a message from this AI. Refused outside
    /// an AI's conversation, for this conversation itself, for an archived conversation or for one in another project.
    StartsTurn Free fn message_conversation(h, by: Option<String>, id: String, message: String) -> String [Failure] {
        let by = agent_by(by)?;
        h.engine.session_message(&by, &id, &message).map_err(refused)
    }
    /// Resumes a conversation this AI started that is stopped, cut off, failed or ended, sending `message` or the default
    /// continuation line. Refused outside an AI's conversation, for a conversation this AI did not start, or while the
    /// conversation is working, waiting for the person or archived.
    StartsTurn Free fn resume_conversation(h, by: Option<String>, id: String, message: Option<String>) -> String [Failure] {
        let by = agent_by(by)?;
        h.engine.session_resume(&by, &id, message.as_deref()).map_err(refused)
    }
    /// Marks this AI's conversation as waiting for the conversations in `on` (all started by it, at most 12) and/or for
    /// `note` (at most 200 characters), so Faber wakes it when one of them reports and spares it from Keep going prods and
    /// auto-archive meanwhile. Refused outside an AI's conversation, when both `on` and `note` are empty, or for an `on` id
    /// this AI did not start.
    Write Free fn mark_waiting(h, by: Option<String>, on: Option<Vec<String>>, note: Option<String>) -> String [Failure] {
        let by = agent_by(by)?;
        h.engine.session_wait(&by, on.unwrap_or_default(), note).map_err(refused)
    }
    /// Pins a conversation, or unpins it. An AI may pin but never unpin: the person pinned it to keep it.
    Write Free fn pin(h, id: String, on: bool, by: Option<String>) -> () [Failure] {
        if by.is_some() && !on {
            return Err(refused("only the person unpins a conversation".into()));
        }
        h.engine.pin(&id, on).map_err(Failure::from)
    }
    Write Ask fn move_to_project(h, id: String, project_id: String) -> Conversation {
        let engine = h.engine.clone();
        off("error.store.write", move || engine.move_to_project(&id, &project_id).map(Conversation::for_screen)).await?
    }
    StartsTurn Own("/id") fn keep_going(h, id: String, on: bool) -> () {
        h.engine.keep_going(&id, on)
    }
    /// The person has the conversation open on a visible window: the done card and the dot of its last run go.
    Read Screen fn seen(h, id: String) -> () {
        h.engine.seen(&id)
    }
    /// Person reach (R585): it starts a turn outside the conversation's own, which spends the person's quota or money.
    StartsTurn Person fn follow_up(h, id: String, file: String, dismiss: bool) -> () {
        h.engine.follow_up(&id, &file, dismiss)
    }
    Write Own("/id") fn remove_queued(h, id: String, turn_id: String) -> () {
        h.engine.remove_queued(&id, &turn_id);
        Ok(())
    }
    /// Person reach (R585): it starts a turn outside the conversation's own, which spends the person's quota or money.
    Write Person fn edit_queued(h, id: String, turn_id: String, text: String) -> () {
        h.engine.edit_queued(&id, &turn_id, &text)
    }
    /// Person reach (R585): it starts a turn outside the conversation's own, which spends the person's quota or money.
    StartsTurn Person fn send_queued(h, id: String, turn_id: String) -> () {
        h.engine.send_queued(&id, &turn_id)
    }
    /// What a subagent wrote between its steps (R161 step 3), for its side chat while it is open. The screen's copy of a message
    /// carries only their length, so this is the one place they are read.
    Read Free fn child_words(h, id: String, call_id: String) -> Vec<faber_engine::model::Words> {
        Ok(h.engine.child_words(&id, &call_id))
    }
    Read Free fn child_transcript(h, id: String, call_id: String, page: u64) -> Vec<faber_engine::transcript::TranscriptEntry> {
        h.engine.child_transcript(&id, &call_id, page)
    }
    Write Own("/id") fn tell_helper(h, id: String, call_id: String, text: String) -> () {
        h.engine.tell_helper(&id, &call_id, &text)
    }
    /// `@` in the composer (parity T4): the entries of the working folder that match `query`, at most 50. Git may run, off the async
    /// threads. One listing runs at a time, and a query that a newer one replaced while it waited gives up, so typing fast in a
    /// big repository never piles up git runs or their output.
    Read Free fn mentions(h, id: Option<String>, project_id: Option<String>, query: String) -> Vec<faber_engine::mention::Mention> {
        // At most one query runs and one waits: a newer one answers the waiting one at once with nothing and takes its place.
        static LISTING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
        static WAITING: Mutex<Option<tokio::sync::oneshot::Sender<()>>> = Mutex::new(None);
        let (replaced, gone) = tokio::sync::oneshot::channel();
        if let Some(older) = lock(&WAITING).replace(replaced) {
            let _ = older.send(());
        }
        let _one = tokio::select! {
            biased;
            _ = gone => return Ok(vec![]),
            one = LISTING.lock() => one,
        };
        let engine = h.engine.clone();
        Ok(tokio::task::spawn_blocking(move || engine.mentions(id.as_deref(), project_id.as_deref(), &query)).await.unwrap_or_default())
    }
    /// The last advertised commands, read from memory without starting a CLI.
    Read Free fn commands(h, id: Option<String>, vendor: Option<Vendor>) -> Vec<faber_engine::model::AgentCommand> {
        Ok(h.engine.commands(id.as_deref(), vendor))
    }
    /// "Compact now" in the context meter: the CLI's own compaction of the conversation's context.
    Write Own("/id") fn compact_now(h, id: String) -> () {
        h.engine.compact_now(&id)
    }
    /// The context meter opened: asks what fills the context; the parts come as an event. False when no CLI can say now.
    Read Free fn context_parts(h, id: String) -> bool {
        Ok(h.engine.context_parts(&id))
    }
    Read Free fn background_commands(h) -> faber_engine::background::List {
        Ok(h.engine.background_commands())
    }
    Write Ask fn stop_background(h, id: String) -> () {
        h.engine.stop_background(&id).await.map_err(str::to_string)
    }
    /// At most one git listing runs and one waits; a newer request replaces the waiting one.
    Read Free fn changes(h, id: String, cursor: Option<String>) -> faber_engine::changes::ChangeList [Failure] {
        static WAITING: Mutex<Option<tokio::sync::oneshot::Sender<()>>> = Mutex::new(None);
        let (replaced, gone) = tokio::sync::oneshot::channel();
        if let Some(older) = lock(&WAITING).replace(replaced) {
            let _ = older.send(());
        }
        let one = tokio::select! {
            biased;
            _ = gone => return Err(Failure::with("error.changes.read", "A newer request replaced this read.")),
            one = LISTING_CHANGES.lock() => one,
        };
        let engine = h.engine.clone();
        Ok(off("error.changes.read", move || {
            let _one = one;
            engine.changes_page(&id, cursor.as_deref())
        })
        .await?)
    }
    Read Free fn change_patch(h, id: String, path: String) -> faber_engine::changes::ChangePatch [Failure] {
        // A newer request for the same file replaces its older waiting read.
        type Waiting = std::collections::HashMap<(String, String), tokio::sync::oneshot::Sender<()>>;
        static WAITING: std::sync::LazyLock<Mutex<Waiting>> = std::sync::LazyLock::new(|| Mutex::new(Waiting::new()));
        let key = (id.clone(), path.clone());
        let (replaced, mut gone) = tokio::sync::oneshot::channel();
        if let Some(older) = lock(&WAITING).insert(key.clone(), replaced) {
            let _ = older.send(());
        }
        let one = tokio::select! {
            biased;
            _ = &mut gone => return Err(Failure::with("error.changes.read", "A newer request replaced this read.")),
            one = READING_PATCH.lock() => one,
        };
        drop(gone);
        {
            let mut waiting = lock(&WAITING);
            // A newer request may have entered while this read acquired its slot.
            if waiting.get(&key).is_some_and(tokio::sync::oneshot::Sender::is_closed) {
                waiting.remove(&key);
            }
        }
        let engine = h.engine.clone();
        Ok(off("error.changes.read", move || {
            let _one = one;
            engine.change_patch(&id, &path)
        })
        .await?)
    }
    Read Free fn answer_changes(h, id: String, message_id: String, offset: usize) -> faber_engine::rewind::AnswerChanges {
        let engine = h.engine.clone();
        off("error.changes.read", move || engine.answer_changes(&id, &message_id, offset)).await?
    }
    Read Free fn answer_change_patch(h, id: String, message_id: String, path: String, offset: usize) -> faber_engine::rewind::AnswerChangePatch {
        let engine = h.engine.clone();
        off("error.changes.read", move || engine.answer_change_patch(&id, &message_id, &path, offset)).await?
    }
    Read Free fn rewind_points(h, id: String) -> Vec<faber_engine::rewind::PointInfo> {
        let engine = h.engine.clone();
        off("error.rewind.storage", move || engine.rewind_points(&id)).await?
    }
    Read Free fn rewind_redos(h, id: String) -> Vec<faber_engine::rewind::RedoInfo> {
        let engine = h.engine.clone();
        off("error.rewind.storage", move || engine.rewind_redos(&id)).await?
    }
    Read Free fn rewind_redo_preview(h, request: faber_engine::rewind::RedoRequest) -> faber_engine::rewind::Preview {
        let engine = h.engine.clone();
        off("error.rewind.storage", move || engine.rewind_redo_preview(request)).await?
    }
    Write Ask fn rewind_recover(h, id: String) -> () {
        let engine = h.engine.clone();
        off("error.rewind.storage", move || engine.rewind_recover(&id)).await?
    }
    Read Free fn rewind_preview(h, request: faber_engine::rewind::RestoreRequest) -> faber_engine::rewind::Preview {
        let engine = h.engine.clone();
        off("error.rewind.storage", move || engine.rewind_preview(request)).await?
    }
    Write Ask fn rewind_restore(h, request: faber_engine::rewind::RestoreRequest) -> faber_engine::rewind::RestoreResult {
        let engine = h.engine.clone();
        off("error.rewind.storage", move || engine.rewind_restore(request)).await?
    }
    Write Ask fn rewind_redo(h, request: faber_engine::rewind::RedoRequest) -> faber_engine::rewind::RestoreResult {
        let engine = h.engine.clone();
        off("error.rewind.storage", move || engine.rewind_redo(request)).await?
    }
    /// A quick question on the side of a conversation (R250); `model` is the tab's own pick, or the conversation's. Person
    /// reach (R585): it starts a turn outside the conversation's own, which spends the person's quota or money.
    StartsTurn Person fn ask_aside(h, id: String, question: String, model: Option<String>) -> String {
        h.engine.ask_aside(&id, &question, model)
    }
    Write Ask fn stop_aside(h, aside_id: String) -> () {
        h.engine.stop_aside(&aside_id);
        Ok(())
    }
    Write Own("/id") fn clear_asides(h, id: String) -> () {
        h.engine.clear_asides(&id);
        Ok(())
    }
    Read Free fn asides(h, id: String) -> Vec<faber_engine::engine::Aside> {
        Ok(h.engine.asides(&id))
    }
    /// The person stops following the pull request a project conversation follows (R200).
    Write Own("/id") fn unfollow_pr(h, id: String) -> () {
        h.engine.unfollow_pr(&id);
        Ok(())
    }
    Write Ask fn remove_project(h, id: String) -> () {
        let engine = h.engine.clone();
        off("error.store.write", move || engine.remove_project(&id)).await?
    }

    /// The look of an appearance that is not saved. Failing pairs come back inside the look.
    Read Screen fn preview_look(h, appearance: Map<String, Value>) -> Look [Failure] {
        look::preview(&appearance)
    }
    /// Measures the data folder and the free space on its drive, in bytes.
    Read Free fn storage_usage(h) -> faber_engine::storage::StorageUsage {
        h.engine.storage_usage().await
    }
    Read Screen fn storage_choice(h) -> crate::storage_choice::Status {
        Ok(lock(&h.storage_choice).status())
    }
    Write Person fn storage_choose(h, reset: bool) -> Option<crate::storage_choice::Status> {
        crate::storage_choice::change(h, reset).await
    }
    /// A theme file comes from other people, so an appearance is checked here before anything is saved.
    Write Ask fn save_settings(h, patch: Settings) -> SavedSettings [Failure] {
        let mut patch = patch;
        if patch.extensions_developer_mode.is_some() {
            return Err(Failure::key("error.host.badArgs"));
        }
        if let Some(map) = &patch.appearance {
            patch.appearance = Some(look::validated(map)?.0.to_map());
        }
        let autostart = patch.autostart;
        let mut patch = Some(patch);
        let settings = h.shell.persist_settings(autostart, &mut || h.engine.save_settings(patch.take().expect("the settings are stored once")))?;
        let settings = h.shell.shown_settings(settings);
        h.shell.settings_saved(&settings);
        let look = look::stored(settings.appearance.as_ref());
        Ok(SavedSettings { settings, look })
    }
    Write Person fn save_theme(h, appearance: Map<String, Value>, name: String) -> bool [Failure] {
        let json = look::write_theme_file(&appearance, &name)?;
        // The dialog suggests the theme's own name, which arrives in the person's language, keeping only plain characters.
        let file = format!("{}{}", file_stem(&name).trim(), look::theme_extension());
        let Some(target) = picked(h.shell.pick_save(Some(JSON_FILES), file).await, "error.theme.write")? else { return Ok(false) };
        look::export_theme(&target, &json)?;
        Ok(true)
    }
    /// The same readable record as the file export, without writing a file.
    Read Free fn conversation_markdown(h, id: String) -> Value {
        let engine = h.engine.clone();
        let language = faber_engine::guide::Language::resolve(engine.settings().language.as_deref(), &faber_engine::guide::system_languages());
        let source = match language {
            faber_engine::guide::Language::En => include_str!("../../ui/locales/en.ts"),
            faber_engine::guide::Language::PtBr => include_str!("../../ui/locales/pt-BR.ts"),
            faber_engine::guide::Language::Es => include_str!("../../ui/locales/es.ts"),
            faber_engine::guide::Language::Fr => include_str!("../../ui/locales/fr.ts"),
            faber_engine::guide::Language::De => include_str!("../../ui/locales/de.ts"),
        };
        let word = |key: &str| -> Res<String> {
            let line = source.lines().find(|line| line.trim_start().starts_with(&format!("\"{key}\":")) || line.trim_start().starts_with(&format!("'{key}':"))).ok_or("error.export.read")?;
            let value = line.split_once(':').ok_or("error.export.read")?.1.trim().trim_end_matches(',').trim();
            let quote = value.chars().next().ok_or("error.export.read")?;
            Ok(value.strip_prefix(quote).and_then(|v| v.strip_suffix(quote)).ok_or("error.export.read")?.to_string())
        };
        let words = faber_engine::export::Words { you: word("export.you")?, auto: word("model.auto")?, attachments: word("export.attachments")?, files: word("export.files")? };
        let (_, markdown) = off("error.export.read", move || engine.export_conversation(&id, &words)).await??;
        Ok(json!({"markdown": markdown}))
    }
    Write Person fn export_conversation(h, id: String, words: faber_engine::export::Words) -> bool [Failure] {
        use std::io::Write;
        let engine = h.engine.clone();
        let (title, markdown) = off("error.export.write", move || engine.export_conversation(&id, &words))
            .await?
            .map_err(Failure::key)?;
        let stem: String = file_stem(&title).chars().take(120).collect();
        let stem = if stem.trim().is_empty() { "Faber" } else { stem.trim() };
        let Some(target) = picked(h.shell.pick_save(Some(MARKDOWN_FILES), format!("{stem}.md")).await, "error.export.write")? else {
            return Ok(false);
        };
        off("error.export.write", move || look::replace_file(&target, |file| file.write_all(markdown.as_bytes())))
            .await?
            .map_err(|_| Failure::key("error.export.write"))?;
        Ok(true)
    }
    Read Person fn open_theme(h) -> Option<ThemeFile> [Failure] {
        let Some(file) = picked(h.shell.pick_file(Some(JSON_FILES)).await, "error.theme.read")? else { return Ok(None) };
        look::import_theme(&file).map(Some)
    }

    Write Person fn add_project(h) -> Option<Project> {
        let picked = h.shell.pick_folder().await.map_err(|why| if why == PickError::NoWindow { NO_WINDOW } else { "error.folder.notLocal" })?;
        let Some(path) = picked else { return Ok(None) };
        h.engine.open_folder(&path.to_string_lossy()).map(Some)
    }
    Write Person fn repoint_project(h, id: String, path: Option<String>) -> Option<Project> {
        project_commands::id(&id)?;
        if let Some(path) = path {
            return h.engine.repoint_project(&id, &path).map(Some);
        }
        if h.hidden {
            return Err("error.project.missing".into());
        }
        let picked = h.shell.pick_folder().await.map_err(|why| if why == PickError::NoWindow { NO_WINDOW } else { "error.folder.notLocal" })?;
        let Some(path) = picked else { return Ok(None) };
        h.engine.repoint_project(&id, &path.to_string_lossy()).map(Some)
    }
    Read Person fn open_log_folder(h) -> String {
        let crash_file = h.data_dir.join(faber_engine::crash::CRASHES);
        let target = if crash_file.exists() { crash_file } else { h.data_dir.clone() };
        if !h.hidden && !cfg!(feature = "test-hooks") {
            h.shell.reveal(target)?;
        }
        Ok(h.data_dir.to_string_lossy().into_owned())
    }
    Write Free fn dismiss_unclean_exit(h) -> () {
        h.engine.dismiss_unclean_exit();
        Ok(())
    }
    /// The recent folders and saved environments, judged now: a folder that is gone or refused carries a reason key.
    Read Free fn recents(h) -> faber_engine::recents::RecentsView {
        Ok(h.engine.recents())
    }
    Write Person fn open_folders(h, paths: Vec<String>) -> Vec<Project> {
        h.engine.open_folders(&paths)
    }
    /// Open a panel on the person's screen to show them something: a Settings section (`general`, `projects`, `appearance`,
    /// `accounts`, `privacy`) or the Catalog (`catalog`). It changes nothing; the window decides how to open it.
    Write Ask fn show_panel(h, panel: String) -> () {
        if !SHOWN_PANELS.contains(&panel.as_str()) {
            return Err(crate::reach::BAD_ARGS.into());
        }
        h.announce(&json!({"type": "showPanel", "panel": panel}));
        Ok(())
    }
    Write Ask fn forget_recent(h, path: String) -> () {
        h.engine.forget_recent(&path)
    }
    Write Ask fn save_environment(h, name: String, folders: Vec<String>) -> () {
        h.engine.save_environment(&name, &folders)
    }
    Write Ask fn forget_environment(h, id: String) -> () {
        h.engine.forget_environment(&id)
    }
    /// Person reach (R585): it opens projects and their conversations for the person.
    Write Person fn start_environment(h, id: String) -> Vec<Project> {
        h.engine.start_environment(&id)
    }
    Write Person fn chat_folder(h, id: Option<String>, scratch: bool, path: Option<String>) -> Option<Conversation> {
        if scratch {
            return h.engine.chat_folder(id.as_deref(), None).map(Conversation::for_screen).map(Some);
        }
        // A folder taken from the recents needs no dialog; the engine checks it like any chosen folder.
        if let Some(path) = path {
            return h.engine.chat_folder(id.as_deref(), Some(Path::new(&path))).map(Conversation::for_screen).map(Some);
        }
        // A hidden test build must never open a visible native dialog.
        if h.hidden {
            return Err("error.chatFolder".into());
        }
        let picked = h.shell.pick_folder().await.map_err(|why| if why == PickError::NoWindow { NO_WINDOW } else { "error.chatFolder" })?;
        let Some(path) = picked else { return Ok(None) };
        h.engine.chat_folder(id.as_deref(), Some(&path)).map(Conversation::for_screen).map(Some)
    }
    /// Copies a file from a chat workspace to wherever the user picks.
    Write Person fn save_file(h, id: String, path: String) -> bool {
        let Some(source) = h.engine.workspace_file(&id, &path) else { return Ok(false) };
        let name = source.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let picked = h.shell.pick_save(None, name).await.map_err(|why| if why == PickError::NoWindow { NO_WINDOW } else { "error.file.save" })?;
        let Some(target) = picked else { return Ok(false) };
        std::fs::File::open(&source).and_then(|source| save_copy(source, &target)).map_err(|_| "error.file.save")?;
        Ok(true)
    }
    Read Ask fn show_file(h, id: String, path: String) -> () {
        let Some(source) = h.engine.folder_file(&id, &path) else { return Ok(()) };
        h.shell.reveal(source)
    }
    Read Screen fn show_damaged_conversation(h, id: String) -> () {
        let engine = h.engine.clone();
        let source = off("error.file.show", move || engine.damaged_conversation(&id)).await??;
        h.shell.reveal(source)
    }
    Write Screen fn remove_damaged_conversation(h, id: String) -> () {
        let engine = h.engine.clone();
        off("error.chat.deleteFailed", move || engine.remove_damaged_conversation(&id)).await?
    }
    /// Shows a project's folder in the file manager. The saved path is checked first (folder.rs), and the folder is only
    /// revealed, never opened: if something else took its place, the file manager selects it and starts nothing.
    Read Ask fn show_project(h, id: String) -> () {
        let project = h.engine.project(&id)?;
        let folder = folder::local_folder(&project.path)?;
        h.shell.reveal(folder)
    }
    /// A file a reply names, read only around its line (parity P2), from inside the conversation's folder.
    Read Own("/id") fn read_file_at(h, id: String, path: String) -> faber_engine::fileview::FileView {
        let engine = h.engine.clone();
        Ok(tokio::task::spawn_blocking(move || engine.read_file_at(&id, &path))
            .await
            .unwrap_or_else(|_| faber_engine::fileview::FileView::failed("error.file.read")))
    }
    /// An image from a chat workspace as a data URL, so the page never reads the disk itself.
    Read Own("/id") fn read_image(h, id: String, path: String) -> Option<String> {
        let Some(source) = h.engine.workspace_file(&id, &path) else { return Ok(None) };
        let ext = source.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
        let Some((_, kind)) = IMAGE_TYPES.iter().find(|(e, _)| Some(*e) == ext.as_deref()) else { return Ok(None) };
        Ok(data_url(&source, kind, 8_000_000))
    }
    /// Begins the visible conversation's media lease; the screen releases it when the view leaves.
    Read Screen fn media_enter(h, id: String) -> String {
        Ok(h.engine.media_enter(&id))
    }
    Read Screen fn media_leave(h, lease: String) -> () {
        h.engine.media_leave(&lease);
        Ok(())
    }
    Read Screen fn media_release(h, lease: String, url: String) -> () {
        h.engine.media_release(&lease, &url);
        Ok(())
    }
    /// A capability for a file named relative to the visible conversation's working folder.
    Read Screen fn media_grant(h, id: String, path: String) -> String {
        let engine = h.engine.clone();
        off("error.media.gone", move || engine.media_grant(&id, &path).map_err(str::to_string)).await?
    }
    /// A capability for public HTTPS media, using the visible conversation's existing lease.
    Read Screen fn media_grant_web(h, id: String, url: String) -> String [Failure] {
        h.engine.media_grant_web(&id, &url).await.map_err(|error| match error.retry_after {
            Some(seconds) => Failure::with(error.key, seconds.to_string()),
            None => Failure::key(error.key),
        })
    }
    Read Screen fn media_clear(h) -> () {
        h.engine.media_clear();
        Ok(())
    }
    /// Bounded bytes for the Electron protocol, kept out of the screen's invoke surface.
    Read Screen fn media_chunk(h, token: String, range: Option<String>) -> Value {
        let engine = h.engine.clone();
        Ok(match engine.media_read_route(&token, range.as_deref()).await {
            Ok(chunk) => json!({
                "type": chunk.kind, "body": base64::engine::general_purpose::STANDARD.encode(chunk.bytes),
                "range": chunk.content_range,
            }),
            Err(error) => json!({"error": error.key, "length": error.length}),
        })
    }
    /// A conversation's tabs in Faber's browser, kept in its file, so the pane opens the front one again.
    Read Own("/id") fn site_tabs(h, id: String) -> Option<faber_engine::browser::tabs::Tabs> {
        // Reading a conversation's file waits on the engine, never on the window's thread.
        let engine = h.engine.clone();
        off("error.conversationGone", move || engine.browser_tabs(&id)).await
    }
    /// The picture the browser took for a call, as a data URL, so the page never reads the disk itself.
    Read Own("/id") fn site_picture(h, id: String, call: String) -> Option<String> {
        let Some(source) = h.engine.site_picture(&id, &call) else { return Ok(None) };
        Ok(data_url(&source, "image/jpeg", 2_000_000))
    }
    /// The pages each project answer of a conversation pictured before and after it ran, by message id: each page's number
    /// and its two picture names, which `site_picture` shows.
    Read Free fn answer_pictures(h, id: String) -> std::collections::BTreeMap<String, Vec<faber_engine::engine::AnswerPage>> {
        let engine = h.engine.clone();
        off("error.conversationGone", move || engine.answer_pictures(&id)).await
    }
    /// A project conversation's rooms (its screens and major parts) with their recent changes, newest picture and the room the
    /// newest answer changed: one scan of the folder, read once per open of the map and per answer end.
    Read Free fn project_rooms(h, id: String) -> Vec<faber_engine::engine::Room> {
        let engine = h.engine.clone();
        off("error.conversationGone", move || engine.project_rooms(&id)).await
    }
    Write Person fn open_terminal(h, id: String) -> bool {
        let Some(cmd) = h.engine.terminal(&id) else { return Ok(false) };
        terminal::open(&cmd).map_err(|_| "error.terminal.open")?;
        Ok(true)
    }
    /// Prepare the exact fields the person can review before opening the bug form.
    Read Screen fn problem_report(h, language: String, crash: bool) -> crate::report::Report {
        Ok(crate::report::prepare(&h.data_dir, &h.shell.version(), h.shell.report_shell(), &language, crash))
    }
    /// Links in answers open in the user's browser. Video cards and the reply link's pane action use a separate command.
    Write Ask fn open_link(h, url: String) -> () {
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            return Err("error.link.web".into());
        }
        h.shell.open_link(&url)
    }
    /// The words the shell shows by itself (tray menu, tooltip, notification), sent whenever the language changes.
    Read Screen fn set_shell_texts(h, texts: Value) -> () {
        h.shell.set_texts(texts)
    }

    /// Settings > Projects: whether each new conversation of this project works in a separate copy (R201). Person reach
    /// (R585): a project's setting is the person's.
    Write Person fn project_copies(h, project_id: String, on: bool) -> () {
        project_commands::id(&project_id)?;
        let project = h.engine.project(&project_id)?;
        let mut updated = project.clone();
        updated.separate_copies = on;
        h.engine.update_project(&project, &updated)
    }
    /// Settings > Projects and the project page's Goals fold: a project's standing goals, the switch and the day's limit.
    Read Free fn project_goals(h, project_id: String) -> faber_engine::project::goals::View {
        project_commands::id(&project_id)?;
        h.engine.project_goals(&project_id)
    }
    /// The project's switch for starting fixes by itself. Off until the person turns it on: Person reach (R585).
    Write Person fn project_goals_auto(h, project_id: String, on: bool) -> faber_engine::project::goals::View {
        project_commands::id(&project_id)?;
        h.engine.set_project_goals(&project_id, on)
    }
    /// Adds a goal: the person's words and one of the project's confirmed checks.
    Write Ask fn project_goal_add(h, project_id: String, words: String, check: String) -> faber_engine::project::goals::View {
        project_commands::id(&project_id)?;
        project_commands::text(&words)?;
        project_commands::text(&check)?;
        h.engine.add_project_goal(&project_id, &words, &check)
    }
    /// Adds a goal that breaks when the installed Faber's crash file gets a new line: Faber's own repository only.
    Write Ask fn project_goal_add_crashes(h, project_id: String, words: String) -> faber_engine::project::goals::View {
        project_commands::id(&project_id)?;
        project_commands::text(&words)?;
        h.engine.add_project_crash_goal(&project_id, &words)
    }
    Write Ask fn project_goal_remove(h, project_id: String, goal_id: String) -> faber_engine::project::goals::View {
        project_commands::id(&project_id)?;
        project_commands::id(&goal_id)?;
        h.engine.remove_project_goal(&project_id, &goal_id)
    }
    /// A paused goal, or one that needs the person, is checked again.
    Write Ask fn project_goal_resume(h, project_id: String, goal_id: String) -> faber_engine::project::goals::View {
        project_commands::id(&project_id)?;
        project_commands::id(&goal_id)?;
        h.engine.resume_project_goal(&project_id, &goal_id)
    }
    Read Free fn project_proof(h, project_id: String) -> checks::ProofView {
        project_commands::id(&project_id)?;
        Ok(checks::view(&h.engine.project(&project_id)?))
    }
    Read Free fn project_memory(h, project_id: String) -> faber_engine::project::memory::MemoryList {
        project_commands::id(&project_id)?;
        let engine = h.engine.clone();
        off("error.memory.read", move || engine.project_memory(&project_id)).await?
    }
    /// Edits or removes a memory entry, or keeps a decision as the person's (R16). The person's alone: a kept decision can be
    /// a "never touch" rule (R575), which is the agent's own permission (R585).
    Write Person fn edit_project_memory(h, project_id: String, edit: faber_engine::project::memory::Edit) -> () {
        project_commands::id(&project_id)?;
        let engine = h.engine.clone();
        off("error.memory.write", move || engine.edit_project_memory(&project_id, &edit)).await?
    }
    /// The owner confirms the commands Faber checks a project's work with (R137): the project's declared list, or the edited one.
    Write Person fn project_proof_confirm(h, project_id: String, commands: Vec<String>, setup: Vec<String>, declared: Vec<String>) -> checks::ProofView {
        project_commands::id(&project_id)?;
        for command in commands.iter().chain(&setup).chain(&declared) {
            project_commands::text(command)?;
        }
        let before = h.engine.project(&project_id)?;
        let after = checks::confirm(&before, &commands, &setup, &declared).map_err(str::to_string)?;
        h.engine.update_project(&before, &after)?;
        Ok(checks::view(&after))
    }

    /// Find the app command in the conversation's project copy and start it if already confirmed.
    Write Person fn run_app_start(h, id: String) -> faber_engine::engine::run_app::View {
        project_commands::id(&id)?;
        let engine = h.engine.clone();
        off("reason.runApp.launch", move || engine.run_app_start(&id, None)).await?
    }
    /// Confirm the displayed app command for the project and start it in the conversation's copy.
    Write Person fn run_app_confirm(h, id: String, seen: String) -> faber_engine::engine::run_app::View {
        project_commands::id(&id)?;
        if seen.len() != 64 || !seen.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("error.host.badArgs".into());
        }
        let engine = h.engine.clone();
        off("reason.runApp.launch", move || engine.run_app_start(&id, Some(&seen))).await?
    }
    /// Read the conversation's app status, local address or recent output if it stopped.
    Read Person fn run_app_status(h, id: String) -> faber_engine::engine::run_app::View {
        project_commands::id(&id)?;
        Ok(h.engine.run_app_status(&id))
    }
    /// Stop the app owned by this conversation.
    Write Person fn run_app_stop(h, id: String) -> faber_engine::engine::run_app::View {
        project_commands::id(&id)?;
        Ok(h.engine.run_app_stop(&id))
    }

    /// The screen's view of the update: its phase and the download's progress. Changes arrive as `faber:update`.
    Read Free fn update_status(h) -> Value {
        Ok(h.shell.update_status())
    }
    /// "Check for updates now" (Settings): one look outside the six-hour clock; a found update is downloaded and kept, and
    /// the answer is the update's phase.
    Write Screen fn update_check(h) -> Value {
        h.shell.update_check().await
    }
    /// "Update ready, restart": installs the kept update now. Work that runs is cut and goes on after the restart.
    Write Ask fn update_restart(h) -> () {
        h.shell.update_restart().await
    }
    /// The screen saw the first input of the session (typing, a paste, a drop): from then on the install at open does not end
    /// the window under it.
    Read Screen fn input_seen(h) -> () {
        h.shell.input_seen();
        Ok(())
    }
    /// The screen says whether any field holds an unsent draft (text or attachments); read when an install is decided.
    Read Screen fn draft_dirty(h, dirty: bool) -> () {
        h.shell.draft_dirty(dirty);
        Ok(())
    }
    /// Whether a game or another full-screen app is in front (R191). The screen asks while its window is shown without the
    /// focus and goes quiet while the answer is yes. A test build answers no, so a game on the PC that runs the checks never
    /// changes them.
    Read Screen fn game_in_front(h) -> bool {
        Ok(!cfg!(feature = "test-hooks") && h.shell.game_in_front())
    }
    /// The last "what's new" entry the person has seen (`ui/notes.ts`), in its own small file in the data folder. None means
    /// no entry was ever shown, which the screen reads as a first install or as an install from before the screen existed. A
    /// test build answers "all seen", so no scenario starts behind the screen.
    Read Screen fn notes_seen(h) -> Option<u32> {
        let saved = std::fs::read_to_string(h.data_dir.join("notes-seen")).ok().and_then(|text| text.trim().parse().ok());
        Ok(saved.or(cfg!(feature = "test-hooks").then_some(u32::MAX)))
    }
    Write Screen fn save_notes_seen(h, id: u32) -> () {
        std::fs::write(h.data_dir.join("notes-seen"), id.to_string()).map_err(|_| "error.settings.saveFailed".to_string())
    }

    Read Free fn setup_check(h, vendor: String) -> setup::CheckResult {
        setup::check(h, vendor).await
    }
    Write Ask fn setup_install(h, vendor: String) -> () {
        setup::install(h, vendor).await
    }
    Write Person fn setup_sign_in(h, vendor: String) -> () {
        setup::sign_in(vendor).await
    }

    Read Free fn tools_list(h) -> Vec<tools::ToolRow> {
        tools::tools_list(h).await
    }
    Read Screen fn tools_search(h, query: String) -> Vec<tools::OpenHit> {
        tools::tools_search(h, query).await
    }
    Read Free fn tools_inventory(h, project_id: Option<String>) -> Vec<faber_engine::tool_inventory::List> {
        tools::tools_inventory(h, project_id).await
    }
    Read Free fn mcp_servers(h, project_id: String) -> Vec<faber_engine::mcp_servers::List> {
        tools::mcp_servers(h, project_id).await
    }
    Write Person fn set_mcp_server(h, project_id: String, name: String, enabled: bool) -> () {
        tools::set_mcp_server(h, project_id, name, enabled).await
    }
    Write Ask fn tools_install(h, id: String) -> () {
        tools::tools_install(h, id).await
    }
    Read Ask fn tools_prepare_replacement(h, id: String) -> tools::PreparedReplacement {
        tools::tools_prepare_replacement(h, id).await
    }
    Write Ask fn tools_replace(h, plan: faber_engine::tools::ReplacementPlan) -> () {
        tools::tools_replace(h, plan).await
    }
    Write Ask fn tools_remove(h, id: String) -> () {
        tools::tools_remove(h, id).await
    }
    Write Ask fn tools_set_enabled(h, id: String, enabled: bool) -> () {
        tools::tools_set_enabled(h, id, enabled).await
    }
    Write Ask fn tools_restore(h, id: String) -> () {
        tools::tools_restore(h, id).await
    }
    Write Person fn tools_clear_removed(h) -> () {
        tools::tools_clear_removed(h).await
    }
    /// The catalog tools a conversation's agent has now, each with its job group, for the context meter's panel: read from the
    /// engine's memory when the panel opens; nothing is prepared, polled or started.
    Read Free fn tools_in_use(h, id: String) -> Vec<(String, String)> {
        tools::tools_in_use(h, id).await
    }

    // What only a window answers: the front end that has one handles these itself, and any other answers `error.host.noWindow`.
    // The browser pane's commands (`site_*`, `cookie_*`) are the exception: a shell whose pane lives behind the host (the sidecar
    // of the Electron shell) gives the host its pane (`Host::set_pane`), and they reach it here.

    /// Bring the window to a conversation.
    Window Ask fn show_conversation(h, id: String) -> () {
        h.shell.show_conversation(&id)?;
        Ok(())
    }

    /// Faber's browser pane (R161 step 7) over the screen's own rectangle for it, in the page's logical pixels.
    Window Screen fn site_place(h, load: u32, box_id: u64, x: f64, y: f64, width: f64, height: f64) -> bool {
        h.pane()?.site_place(load, box_id, x, y, width, height).await
    }
    /// The browser pane's page takes the corners of the card the screen shows it in: `radius` in logical pixels and the card's
    /// colour as red, green and blue; a radius of 0 leaves them square.
    Window Screen fn site_round(h, radius: f64, color: [u8; 3]) -> () {
        h.pane()?.site_round(radius, color).await
    }
    /// The screen sizes Faber's browser can draw its pages at and the one in use; with `pick`, `size` becomes the pane's first.
    Window Screen fn site_size(h, pick: bool, size: Option<String>) -> Value {
        h.pane()?.site_size(pick, size).await
    }
    /// The browser pane leaves the screen for the screen's box `box_id` and every older one; it keeps its page.
    Window Screen fn site_park(h, load: u32, box_id: u64) -> () {
        h.pane()?.site_park(load, box_id).await
    }
    /// The person opens a page from a reply's card in the browser pane; it is that conversation's page from then on.
    Window Screen fn site_extensions_mode(h, enabled: bool) -> bool {
        let pane = h.pane().map_err(|_| "error.browser.extensionsUnavailable")?;
        pane.extension_mode(enabled, || {
            h.engine.save_settings(Settings { extensions_developer_mode: Some(enabled), ..Settings::default() }).map(drop)
        }).await?;
        Ok(enabled)
    }
    Window Screen fn site_extensions(h, id: String, action: String, folder: Option<String>) -> Value {
        let pane = h.pane().map_err(|_| "error.browser.extensionsUnavailable")?;
        pane.extensions_available().await?;
        if h.engine.read_window(&id, 1).is_none() {
            return Err("error.browser.extensionsFailed".into());
        }
        let (action, folder) = match (action.as_str(), folder) {
            ("list", None) => ("list", None),
            ("reload", Some(folder)) => ("reload", Some(folder)),
            ("remove", Some(folder)) => ("remove", Some(folder)),
            ("pick", None) => {
                match picked(h.shell.pick_folder().await, "error.browser.extensionsFailed").map_err(|failure| failure.key)? {
                    Some(path) => ("load", Some(path.into_os_string().into_string().map_err(|_| "error.browser.extensionsFailed")?)),
                    None => ("list", None),
                }
            }
            _ => return Err("error.host.badArgs".into()),
        };
        pane.extensions(&id, action, folder.as_deref()).await.map_err(|why| {
            if matches!(why.as_str(), "error.browser.extensionsUnavailable" | "error.browser.extensionsDisabled") {
                why
            } else {
                "error.browser.extensionsFailed".into()
            }
        })
    }
    Window Screen fn site_open(h, id: String, url: String) -> bool {
        h.pane()?.site_open(id, url).await
    }
    /// The person goes back, forward or reloads in the browser pane, on that conversation's page only.
    Window Screen fn site_step(h, id: String, step: String) -> bool {
        h.pane()?.site_step(id, step).await
    }
    /// The person loads an address they typed in the tab in front of the browser pane.
    Window Screen fn site_go(h, id: String, url: String) -> bool {
        h.pane()?.site_go(id, url).await
    }
    /// What the person did to the browser pane's tabs, answered with the tabs and the tab it concerns.
    Window Screen fn site_tab(h, id: String, step: String, tab: Option<String>) -> Option<Value> {
        h.pane()?.site_tab(id, step, tab).await
    }
    /// Whether Back and Forward of the browser pane can act on conversation `id`'s page: `[back, forward]`.
    Window Screen fn site_history(h, id: String) -> [bool; 2] {
        h.pane()?.site_history(id).await
    }
    /// Pick mode on conversation `id`'s page in view: with `on`, waits a few seconds for the person to pick something and answers
    /// its masked words and picture, `waiting` or `ended`; without, ends the mode.
    Window Screen fn site_pick(h, id: String, on: bool) -> Value {
        h.pane()?.site_pick(id, on).await
    }
    /// Clears conversation `id`'s browsing data once the person said yes: tabs, closed tabs, sign-ins and the page's own data.
    Window Screen fn site_clear(h, id: String) -> () {
        h.pane()?.site_clear(id).await
    }
    /// The Firefox profiles on this computer whose sign-ins the browser pane can bring over.
    Window Person fn cookie_profiles(h) -> Value {
        h.pane()?.cookie_profiles().await
    }
    /// The sites with cookies in the profile `profile`, under a ticket the import names.
    Window Person fn cookie_sites(h, profile: String) -> Value {
        h.pane()?.cookie_sites(profile).await
    }
    /// Brings the person's sign-ins on the sites they picked into conversation `id`'s pages; answers how many sites came over.
    Window Person fn cookie_import(h, id: String, ticket: String, sites: Vec<String>) -> usize {
        h.pane()?.cookie_import(id, ticket, sites).await
    }
    /// The sites whose sign-ins conversation `id` holds.
    Window Person fn cookie_held(h, id: String) -> Vec<String> {
        h.pane()?.cookie_held(id).await
    }
    /// Forgets the sign-ins conversation `id` holds and signs its page out of them.
    Window Person fn cookie_forget(h, id: String) -> () {
        h.pane()?.cookie_forget(id).await
    }
    /// The screen painted its first frame: the window may show now.
    Window Screen fn screen_ready(h) -> () {
        Err(NO_WINDOW.into())
    }
    /// A page a reply drew gets its own address in the window's page server; the window answers all four.
    Window Screen fn visual_open(h) -> String {
        h.shell.visual_open()
    }
    Window Screen fn visual_page(h, id: String, revision: u64, document: String) -> visual::Prepared {
        h.shell.visual_page(&id, revision, document, h.engine.visual_network())
    }
    Window Screen fn visual_close(h, id: String) -> () {
        h.shell.visual_close(&id)
    }
    /// The single-use bytes and CSP header of a prepared visual page, for the Electron shell's `faber-visual` scheme.
    Window Screen fn visual_document(h, path: String) -> visual::Document {
        h.shell.visual_document(&path)
    }

    // Test builds only.

    /// No authored page can call this command; the screen snapshots the saved choice for each opened version.
    #[cfg(feature = "test-hooks")]
    Window Screen fn visual_policy(h) -> bool {
        h.shell.visual_policy(h.engine.visual_network())
    }
    /// The screen's checks send their result to `FABER_CHECKS_OUT`, written whole before the app ends.
    #[cfg(feature = "test-hooks")]
    Window Screen fn report_checks(h, json: String) -> () {
        Err(NO_WINDOW.into())
    }
    /// The checks page says which check starts, and the shell keeps that note next to the report (`FABER_CHECKS_OUT` with the
    /// extension `progress`). The runner resets its wait whenever the check in it changes, so a run that is slow but alive never
    /// times out while a page stuck inside one check still does.
    #[cfg(feature = "test-hooks")]
    Write Screen fn checks_progress(h, note: String) -> () {
        let out = PathBuf::from(std::env::var_os("FABER_CHECKS_OUT").ok_or("FABER_CHECKS_OUT is not set")?);
        // Written whole and renamed into place: the runner polls the file, and half a note would read as progress.
        let part = out.with_extension("progress.part");
        std::fs::write(&part, note).map_err(|e| e.to_string())?;
        std::fs::rename(&part, out.with_extension("progress")).map_err(|e| e.to_string())
    }
    /// Observed native child geometry and a bounded view of the browser pane's fixture page.
    #[cfg(all(windows, feature = "test-hooks"))]
    Window Screen fn test_site_probe(h, capture: bool) -> Value {
        Err(NO_WINDOW.into())
    }
    /// Resizes the real hidden host, rather than emulating the main page's viewport.
    #[cfg(all(windows, feature = "test-hooks"))]
    Window Screen fn test_host_size(h, width: u32, height: u32) -> () {
        Err(NO_WINDOW.into())
    }
    /// Refuses the next page open, to exercise the retry feedback.
    #[cfg(all(windows, feature = "test-hooks"))]
    Window Screen fn test_site_refuse_once(h) -> () {
        Err(NO_WINDOW.into())
    }
}

#[cfg(test)]
mod revoke_tests {
    struct Quiet;
    impl Shell for Quiet {}
    async fn host() -> (Host, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        let wiring = Wiring::new(Arc::new(Quiet));
        let opts = EngineOptions {
            data_dir: data.clone(),
            // Private Chat work lives beside the data folder, never inside it (#383).
            #[cfg(windows)]
            chat_dir: dir.path().join("chat-work"),
            clis: std::collections::BTreeMap::new(),
            spawn: real_spawner(),
            windows: false,
            claude_env_billed: false,
            max_warm: 3,
            idle: Duration::from_secs(120),
            runtime: Handle::current(),
            ask: None,
        };
        let engine = Engine::new(opts, wiring.emitter()).unwrap();
        (wiring.host(engine, data, true, Handle::current()), dir)
    }

    #[tokio::test]
    async fn superseded_change_reads_return_an_error_instead_of_empty_success() {
        use faber_engine::project::git::{Executor, GitError, Invocation, Output};
        use std::future::Future;
        use std::task::{Context, Waker};
        use tokio::sync::oneshot;

        type First = Arc<Mutex<Option<(oneshot::Sender<()>, std::sync::mpsc::Receiver<()>)>>>;
        struct Git(First);
        impl Executor for Git {
            fn execute(&mut self, call: &Invocation) -> Result<Output, GitError> {
                let first = lock(&self.0).take();
                if let Some((started, release)) = first {
                    let _ = started.send(());
                    release.recv_timeout(Duration::from_secs(5)).unwrap();
                }
                let stdout = if call.args.iter().any(|word| word == "rev-parse" || word == "merge-base") {
                    "a".repeat(40).into_bytes()
                } else {
                    vec![]
                };
                Ok(Output { success: true, stdout })
            }
        }

        let (host, dir) = host().await;
        let path = dir.path().join("copy");
        std::fs::create_dir(&path).unwrap();
        let record: Conversation = serde_json::from_value(json!({
            "id":"copy", "title":"Copy", "area":"project", "messages":[], "createdAt":1, "updatedAt":1,
            "copy":{"path":path,"branch":"faber/copy","root":path}
        }))
        .unwrap();
        faber_engine::store::Store::new(&dir.path().join("data")).save_checked(&record).unwrap();
        let who = Caller::Person;
        let mut outcomes = vec![];
        for (command, args, slot) in
            [("changes", json!({"id":"copy"}), &LISTING_CHANGES), ("change_patch", json!({"id":"copy","path":"file.txt"}), &READING_PATCH)]
        {
            let (started, entered) = oneshot::channel();
            let (release, hold) = std::sync::mpsc::channel();
            let first = Arc::new(Mutex::new(Some((started, hold))));
            host.engine.set_git(Arc::new(move || Some(Box::new(Git(first.clone())))));

            let read = || host.dispatch_as(&who, command, args.clone());
            let mut running = Box::pin(read());
            tokio::select! {
                biased;
                result = running.as_mut() => panic!("the first read did not reach git: {result:?}"),
                started = tokio::time::timeout(Duration::from_secs(5), entered) => started.unwrap().unwrap(),
            };
            drop(running);
            assert!(slot.try_lock().is_err(), "the first Git read keeps its slot after its caller leaves");

            let mut waiting = std::pin::pin!(read());
            let mut newer = std::pin::pin!(read());
            {
                let mut context = Context::from_waker(Waker::noop());
                assert!(waiting.as_mut().poll(&mut context).is_pending());
                assert!(newer.as_mut().poll(&mut context).is_pending());
            }
            let skipped = tokio::time::timeout(Duration::from_secs(5), waiting).await.unwrap();
            release.send(()).unwrap();
            assert!(newer.await.is_ok());
            outcomes.push(skipped);
        }
        for skipped in outcomes {
            let error = skipped.expect_err("a replaced read must report that it did not run");
            assert_eq!(error.key, "error.changes.read");
            assert_eq!(error.detail.as_deref(), Some("A newer request replaced this read."));
        }
    }

    #[tokio::test]
    async fn app_bootstrap_and_refresh_hide_foreign_command_output_and_running_details() {
        use faber_engine::model::{RunState, RunningItem, Vendor};
        let (host, _dir) = host().await;
        let mut original = host.engine.state();
        for id in ["owner", "child", "other"] {
            original.background.items.push(faber_engine::background::Command {
                id: id.into(),
                conversation_id: id.into(),
                vendor: Vendor::Codex,
                command: "private command".into(),
                started: 1,
                output: format!("{id} secret output"),
                state: faber_engine::background::State::Running,
                stoppable: true,
            });
            original.running.items.push(RunningItem {
                conversation_id: id.into(),
                turn_id: "turn".into(),
                title: id.into(),
                area: Area::Chat,
                project_id: None,
                vendor: None,
                model: None,
                state: RunState::Working,
                since: 1,
                permissions: vec![],
                parked: vec![],
                parked_more: 0,
                stuck: vec![],
                stall: None,
                wait_until: None,
                held: None,
                keep_going: false,
                keep_wait: None,
                waiting_on: None,
                keep_progress: None,
                settled: false,
                plan: None,
                now: Some(format!("{id} private activity")),
            });
        }
        for by in [Some("owner"), None, Some("missing")] {
            for _ in 0..2 {
                // Bootstrap and the state refresh following a list event use the same filter.
                let mut state = original.clone();
                scope_app_state(&mut state, by, |id| match id {
                    "owner" | "child" => Some("owner".into()),
                    "other" => Some("other".into()),
                    _ => None,
                });
                let expected = if by == Some("owner") { vec!["owner", "child"] } else { vec![] };
                assert_eq!(state.background.items.iter().map(|i| i.conversation_id.as_str()).collect::<Vec<_>>(), expected);
                assert_eq!(state.running.items.iter().map(|i| i.conversation_id.as_str()).collect::<Vec<_>>(), expected);
                assert!(!serde_json::to_string(&state).unwrap().contains("other secret output"));
            }
        }
        let caller = Caller::Agent { conversation: Some("owner".into()) };
        assert_eq!(with_by(&caller, json!({"by":"other"}))["by"], "owner");
        assert!(with_by(&Caller::Agent { conversation: None }, json!({"by":"other"})).get("by").is_none());
    }

    #[tokio::test]
    async fn conversation_list_filters_saved_endings_without_broadening_a_family() {
        let (host, dir) = host().await;
        let store = faber_engine::store::Store::new(&dir.path().join("data"));
        for (id, status, closing) in [
            ("failed", "error", false),
            ("stopped", "stopped", false),
            ("interrupted", "streaming", false),
            ("ended", "done", false),
            ("concluded", "done", true),
            ("new", "", false),
        ] {
            let messages = if status.is_empty() {
                vec![]
            } else {
                vec![json!({"id":"reply", "role":"assistant", "text":"Reply", "status":status, "at":1,
                    "wrapUp":closing.then(|| json!({"done":true, "recap":"Done"}))})]
            };
            let saved: Conversation = serde_json::from_value(json!({
                "id": id, "title": id, "area":"chat", "createdAt":1, "updatedAt":1, "messages": messages,
                "startedBy": (id != "failed" && id != "new").then_some("failed"),
                "archivedAt": (id == "stopped").then_some(2),
                "waitingOn": matches!(id, "failed" | "interrupted" | "stopped").then(|| json!({"since":1,"until":9})),
            }))
            .unwrap();
            store.save_checked(&saved).unwrap();
        }
        let caller = Caller::Agent { conversation: Some("failed".into()) };
        for state in ["failed", "stopped", "interrupted", "ended", "concluded", "new"] {
            let rows = host.dispatch_as(&caller, "conversations", json!({"state":[state], "archived":false})).await.unwrap();
            assert_eq!(rows.as_array().unwrap().len(), 1, "{state}: {rows}");
            assert_eq!(rows[0]["id"], state);
            let read = host.dispatch_as(&caller, "conversation", json!({"id":state, "from":1})).await.unwrap();
            assert_eq!(rows[0]["state"], read["state"]);
        }
        let family = host.dispatch_as(&caller, "conversations", json!({"family":"ended"})).await.unwrap();
        assert_eq!(family.as_array().unwrap().len(), 4);
        assert!(family.as_array().unwrap().iter().all(|r| r["state"] == "idle" && r["family"] == "failed"));
        let missing = host.dispatch_as(&caller, "conversations", json!({"family":"missing"})).await.unwrap();
        assert_eq!(missing, json!([]));
        let bad_state = host.dispatch_as(&caller, "conversations", json!({"state":["faild"]})).await.unwrap_err();
        assert_eq!(bad_state.key, "error.host.badArgs");
    }

    #[tokio::test]
    async fn quota_and_project_reads_leave_conversation_records_alone() {
        let (host, dir) = host().await;
        let data = dir.path().join("data");
        let store = faber_engine::store::Store::new(&data);
        let project = Project::new("sample".into(), "Sample".into(), dir.path().to_string_lossy().into_owned());
        store.save_projects(&[project]).unwrap();
        let path = data.join("conversations").join("invalid.json");
        std::fs::write(&path, b"incomplete conversation").unwrap();
        let damaged = path.with_extension("json.damaged");
        let who = Caller::Person;
        // The first unreadable record is retried on the next list before it is preserved.
        host.dispatch_as(&who, "conversations", json!({})).await.unwrap();
        assert!(!damaged.exists());
        let quota = host.dispatch_as(&who, "quota", json!({})).await.unwrap();
        assert!(quota["quotas"].is_array() && quota["usage"].is_array());
        assert!(!damaged.exists(), "reading quota must not try to open conversations");
        let projects = host.dispatch_as(&who, "projects", json!({})).await.unwrap();
        assert_eq!(projects[0]["id"], "sample");
        assert!(!damaged.exists(), "reading projects must not try to open conversations");
        host.dispatch_as(&who, "conversations", json!({})).await.unwrap();
        assert!(damaged.exists(), "the control: listing notices an unreadable record");
    }

    #[tokio::test]
    async fn recent_command_results_use_the_channels_authenticated_agent() {
        let (host, dir) = host().await;
        let store = faber_engine::store::Store::new(&dir.path().join("data"));
        for id in ["lead", "child", "peer"] {
            let record: Conversation = serde_json::from_value(json!({
                "id": id, "title": id, "area": "project", "projectId": "test-project",
                "startedBy": (id == "child").then_some("lead"), "createdAt": 1, "updatedAt": 2, "messages":[]
            }))
            .unwrap();
            store.save_checked(&record).unwrap();
            host.engine.open(id).unwrap();
        }
        let command = "conversation_command_results";
        let args = json!({"id":"child","by":"lead"});
        let lead = Caller::Agent { conversation: Some("lead".into()) };
        let peer = Caller::Agent { conversation: Some("peer".into()) };
        assert_eq!(host.dispatch_as(&lead, command, args.clone()).await.unwrap()["items"], json!([]));
        let active = json!({"id":"child","by":"lead","runningOnly":true});
        assert_eq!(
            host.dispatch_as(&lead, command, active.clone()).await.unwrap(),
            json!({"id":"child","scope":"runningInThisFaberRun","items":[]})
        );
        assert_eq!(host.dispatch_as(&peer, command, active).await.unwrap_err().key, "error.session.refused");
        assert_eq!(host.dispatch_as(&peer, command, args.clone()).await.unwrap_err().key, "error.session.refused");
        assert_eq!(host.dispatch_as(&Caller::Person, command, args.clone()).await.unwrap_err().key, "error.session.agentOnly");
        assert_eq!(
            host.dispatch_as(&Caller::Agent { conversation: None }, command, args).await.unwrap_err().key,
            "error.session.agentOnly"
        );
    }

    #[tokio::test]
    async fn the_command_reads_one_tool_page_with_the_internal_readers_scope() {
        let (host, dir) = host().await;
        let store = faber_engine::store::Store::new(&dir.path().join("data"));
        let save = |id: &str, project: &str, parent: Option<&str>, temporary: bool| -> Conversation {
            let saved = serde_json::from_value(json!({
                "id": id, "title": id, "area":"project", "projectId":project, "startedBy":parent,
                "temporary":temporary, "createdAt":1, "updatedAt":2,
                "messages":[
                    {"id":"question", "role":"user", "text":"An unrelated message body", "at":1},
                    {"id":"answer", "role":"assistant", "text":"An unrelated reply body", "at":2, "status":"error", "tools":[
                        {"id":"build","name":"Bash","ok":false,"input":{"command":"a private command"},"exitCode":32,
                         "output":"Permission denied\nPASSWORD=sample-hidden-value"},
                        {"id":"search","name":"webSearch","ok":true,"input":null,"output":"release notes"}
                    ]}
                ]
            }))
            .unwrap();
            store.save_checked(&saved).unwrap();
            saved
        };
        save("lead", "p", None, false);
        let mut child = save("child", "p", Some("lead"), false);
        save("grandchild", "p", Some("child"), false);
        save("peer", "p", None, false);
        save("other", "q", Some("lead"), false);
        save("temporary", "p", Some("lead"), true);
        let lead = Caller::Agent { conversation: Some("lead".into()) };
        assert!(host.engine.open("lead").is_some());
        let answer = host.dispatch_as(&lead, "conversation_steps", json!({"id":"child","from":2,"limit":1})).await.unwrap();
        assert_eq!((answer["messageId"].as_str(), answer["number"].as_u64()), (Some("answer"), Some(2)));
        assert_eq!(answer["count"], 2);
        assert_eq!(answer["nextOffset"], 1);
        assert_eq!(answer["tools"][0]["exitCode"], 32);
        assert!(answer["tools"][0].get("diagnostic").is_none());
        let failures = host.dispatch_as(&lead, "conversation_steps", json!({"id":"child","from":2,"failedOnly":true})).await.unwrap();
        assert_eq!(failures["count"], 1);
        let own = host.dispatch_as(&lead, "conversation_steps", json!({"from":2,"tool":"search"})).await.unwrap();
        assert_eq!(own["id"], "lead");
        assert_eq!(own["tools"][0]["diagnostic"], "release notes");
        let read = async |caller: &Caller, id: &str| {
            host.dispatch_as(caller, "conversation_steps", json!({"by":"child","id":id,"from":2,"tool":"build"})).await
        };
        let details = read(&lead, "child").await.unwrap();
        let words = details["tools"][0]["diagnostic"].as_str().unwrap();
        assert!(words.contains("Permission denied") && !details.to_string().contains("sample-hidden-value"));
        assert!(!details.to_string().contains("private command") && !details.to_string().contains("unrelated reply body"));
        for id in ["peer", "grandchild"] {
            let step = read(&lead, id).await.unwrap();
            assert_eq!(step["tools"][0]["diagnosticAvailable"], false);
            assert!(step["tools"][0].get("diagnostic").is_none());
        }
        for id in ["other", "temporary"] {
            assert_eq!(read(&lead, id).await.unwrap_err().key, "error.session.refused");
        }
        for caller in [Caller::Person, Caller::Screen, Caller::Agent { conversation: None }] {
            assert_eq!(read(&caller, "child").await.unwrap_err().key, "error.session.agentOnly");
        }
        let missing = Caller::Agent { conversation: Some("missing".into()) };
        assert_eq!(read(&missing, "child").await.unwrap_err().key, "error.session.refused");
        let bad = host.dispatch_as(&lead, "conversation_steps", json!({"id":"child","from":2,"limit":1,"tool":"build"})).await.unwrap_err();
        assert_eq!(bad.key, "error.session.refused");

        child.messages[1].tools.as_mut().unwrap()[0].output = Some("New diagnostic from the saved version".into());
        store.save_checked(&child).unwrap();
        assert_eq!(read(&lead, "child").await.unwrap()["tools"][0]["diagnostic"], "New diagnostic from the saved version");
        child.project_id = Some("q".into());
        store.save_checked(&child).unwrap();
        assert_eq!(read(&lead, "child").await.unwrap_err().key, "error.session.refused");
    }

    #[tokio::test]
    async fn conversation_tail_reads_new_disk_versions_without_loading_them() {
        use faber_engine::model::{Message, Role};
        let (host, dir) = host().await;
        let store = faber_engine::store::Store::new(&dir.path().join("data"));
        let mut saved: Conversation = serde_json::from_value(json!({
            "id":"saved", "title":"Saved", "area":"chat", "createdAt":1, "updatedAt":3,
            "messages":[
                {"id":"one", "role":"user", "text":"First", "at":1},
                {"id":"two", "role":"assistant", "text":"Answer", "at":2, "status":"done", "wrapUp":{"recap":"Done","done":true}},
                {"id":"three", "role":"user", "text":"Question", "at":3}
            ]
        }))
        .unwrap();
        store.save_checked(&saved).unwrap();
        let caller = Caller::Person;
        let summary = host.dispatch_as(&caller, "conversation", json!({"id":"saved","last":0})).await.unwrap();
        assert_eq!(summary["count"], 3);
        assert_eq!(summary["messages"], json!([]));
        assert_eq!(summary["conclusion"]["recap"], "Done");

        saved.messages.push(Message::new("four".into(), Role::Assistant, "Added on disk".into(), 4));
        store.save_checked(&saved).unwrap();
        let newest = host.dispatch_as(&caller, "conversation", json!({"id":"saved","last":1})).await.unwrap();
        assert_eq!(newest["count"], 4);
        assert_eq!(newest["messages"].as_array().unwrap().len(), 1);
        assert_eq!(newest["messages"][0]["text"], "Added on disk");
        assert!(newest.get("first").is_none());

        saved.messages[3].text = "Updated again".into();
        store.save_checked(&saved).unwrap();
        let full = host.dispatch_as(&caller, "conversation", json!({"id":"saved","last":20})).await.unwrap();
        assert_eq!(full["messages"].as_array().unwrap().len(), 4);
        assert_eq!(full["messages"][3]["text"], "Updated again");
    }

    #[tokio::test]
    async fn conversation_read_selects_message_numbers_without_changing_its_total_state() {
        let (host, dir) = host().await;
        let store = faber_engine::store::Store::new(&dir.path().join("data"));
        let mut saved: Conversation = serde_json::from_value(json!({
            "id":"saved", "title":"Work", "area":"project", "createdAt":1, "updatedAt":4,
            "archivedAt":5,
            "messages":[
                {"id":"one", "role":"user", "text":"First request", "at":1},
                {"id":"two", "role":"assistant", "text":"First reply", "at":2, "status":"done",
                    "wrapUp":{"recap":"First conclusion","done":true},
                    "tools":[
                        {"id":"exit","name":"PowerShell","input":{"private":"argument"},"ok":false,"exitCode":7,"output":"raw command output"},
                        {"id":"refusal","name":"Shell","input":null,"ok":false,"declined":true},
                        {"id":"lost","name":"webSearch","input":null,"interrupted":true},
                        {"id":"legacy","name":"Read","input":null,"ok":true}
                    ]},
                {"id":"three", "role":"user", "text":"Second request", "at":3},
                {"id":"four", "role":"assistant", "text":"Interrupted", "at":4, "status":"streaming"}
            ]
        }))
        .unwrap();
        store.save_checked(&saved).unwrap();
        let caller = Caller::Agent { conversation: Some("saved".into()) };
        let page = host.dispatch_as(&caller, "conversation", json!({"id":"saved","from":2})).await.unwrap();
        assert_eq!(page["count"], 4);
        assert_eq!(page["first"], 2);
        assert_eq!(page["state"], "interrupted");
        assert_eq!(page["interruptedTurn"], "four");
        assert_eq!(page["messages"].as_array().unwrap().len(), 1);
        assert_eq!(page["messages"][0]["id"], "two");
        assert_eq!(
            page["messages"][0]["tools"],
            json!([
                {"name":"PowerShell","ok":false,"exitCode":7},
                {"name":"Shell","ok":false,"declined":true},
                {"name":"webSearch","interrupted":true},
                {"name":"Read","ok":true}
            ])
        );
        assert_eq!(page["conclusion"]["recap"], "First conclusion");

        saved.messages[1].text = "Updated on disk".into();
        store.save_checked(&saved).unwrap();
        let updated = host.dispatch_as(&caller, "conversation", json!({"id":"saved","from":2})).await.unwrap();
        assert_eq!(updated["messages"][0]["text"], "Updated on disk", "a numbered read does not keep a closed conversation resident");

        for args in [json!({"from":0}), json!({"limit":2}), json!({"from":1,"last":1}), json!({"from":1,"limit":11})] {
            let mut args = args;
            args["id"] = json!("saved");
            assert_eq!(host.dispatch_as(&caller, "conversation", args).await.unwrap_err().key, "error.host.badArgs");
        }
        let beyond = host.dispatch_as(&caller, "conversation", json!({"id":"saved","from":7})).await.unwrap();
        assert_eq!(beyond["count"], 4);
        assert!(beyond.get("first").is_none());
        assert_eq!(beyond["messages"], json!([]));
        let tail = host.dispatch_as(&caller, "conversation", json!({"id":"saved","last":1})).await.unwrap();
        assert!(tail.get("first").is_none());
        assert_eq!(tail["count"], 4);
        assert_eq!(tail["messages"][0]["id"], "four");
        let hot = host.dispatch_as(&caller, "conversation", json!({"id":"saved","from":2})).await.unwrap();
        assert_eq!(updated, hot);
    }

    #[tokio::test]
    async fn app_control_open_preserves_the_persons_temporary_chat() {
        let (host, dir) = host().await;
        let store = faber_engine::store::Store::new(&dir.path().join("data"));
        let mut person: Conversation = serde_json::from_value(json!({
            "id":"person", "title":"Private draft", "area":"chat", "createdAt":1, "updatedAt":1,
            "temporary":true, "messages":[{"id":"draft", "role":"user", "text":"Keep my draft", "at":1}],
            "queued":[{"id":"pending", "text":"Keep pending work", "choice":{"vendor":"auto"}, "at":1}]
        }))
        .unwrap();
        store.save_checked(&person).unwrap();
        let mut owner = person.clone();
        owner.id = "owner".into();
        owner.temporary = false;
        owner.queued.clear();
        owner.messages = (0..45)
            .map(|i| {
                let mut message = person.messages[0].clone();
                message.id = format!("message-{i}");
                message
            })
            .collect();
        store.save_checked(&owner).unwrap();
        host.engine.open_for_screen_window("person", 40).unwrap();
        let caller = Caller::Agent { conversation: Some("owner".into()) };
        let opened = host.dispatch_as(&caller, "app_control_open", json!({"id":"owner"})).await.unwrap();
        person = host.engine.open("person").expect("agent reads must preserve the person's temporary chat");
        assert!(!person.stopped);
        assert_eq!(person.messages[0].text, "Keep my draft");
        assert_eq!(person.queued[0].text, "Keep pending work");
        assert!(store.get("person").is_some());
        assert_eq!(opened["messages"].as_array().unwrap().len(), 40);
        assert_eq!(opened["earlier"], 5);
        host.engine.shutdown();
    }

    #[tokio::test]
    async fn app_agent_send_preserves_temporary_chats() {
        let (host, dir) = host().await;
        let store = faber_engine::store::Store::new(&dir.path().join("data"));
        for (id, messages, queued) in [
            (
                "person",
                json!([{"id":"draft", "role":"user", "text":"Keep my draft", "at":1}]),
                json!([{"id":"pending", "text":"Keep pending work", "choice":{"vendor":"auto"}, "at":1}]),
            ),
            ("blank", json!([]), json!([])),
        ] {
            let chat: Conversation = serde_json::from_value(json!({
                "id":id, "title":"Draft", "area":"chat", "createdAt":1, "updatedAt":1,
                "temporary":true, "messages":messages, "queued":queued
            }))
            .unwrap();
            store.save_checked(&chat).unwrap();
        }
        host.engine.open_for_screen_window("person", 40).unwrap();
        let before_person = serde_json::to_value(host.engine.open("person").unwrap()).unwrap();
        let before_blank = serde_json::to_value(host.engine.open("blank").unwrap()).unwrap();
        let caller = Caller::Agent { conversation: Some("owner".into()) };
        for req in [json!({"text":"Injected", "conversationId":"owner"}), json!({"text":"Injected"})] {
            assert!(host.dispatch_as(&caller, "send", json!({"req":req})).await.is_err());
            assert_eq!(serde_json::to_value(host.engine.open("person").unwrap()).unwrap(), before_person);
            assert_eq!(serde_json::to_value(host.engine.open("blank").unwrap()).unwrap(), before_blank);
            assert!(store.get("person").is_some());
            assert!(store.get("blank").is_some());
        }
        host.engine.shutdown();
    }

    #[tokio::test]
    async fn app_renderer_reads_bootstrap_but_cannot_gain_screen_authority() {
        let (host, _dir) = host().await;
        let caller = Caller::Agent { conversation: Some("app-owner".into()) };
        assert!(host.dispatch_as(&caller, "app_control_state", json!({})).await.unwrap()["settings"].is_object());
        assert!(host.dispatch_as(&caller, "app_control_state", json!(["other"])).await.is_err());
        assert!(host.dispatch_as(&caller, "app_control_state", json!({"by":"other"})).await.is_err());
        for command in ["state", "answer_ask", "open_folders", "screen_ready"] {
            assert!(host.dispatch_as(&caller, command, json!({})).await.is_err(), "{command}");
        }
    }

    #[test]
    fn t515a_failed_workspace_copy_preserves_existing_destination() {
        use std::io::{self, Read};
        struct Failing(bool);
        impl Read for Failing {
            fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
                if self.0 {
                    return Err(io::Error::other("copy interrupted"));
                }
                self.0 = true;
                bytes[0] = b'x';
                Ok(1)
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("saved.txt");
        std::fs::write(&target, b"previous contents").unwrap();
        assert!(super::save_copy(Failing(false), &target).is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"previous contents");
        super::save_copy(&b"new contents"[..], &target).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"new contents");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1, "temporary siblings are cleaned up");
    }

    use super::*;
    #[test]
    fn revoked_blocking_operation_does_not_mutate() {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().max_blocking_threads(1).build().unwrap();
        runtime.block_on(async {
            let (started, waiting) = std::sync::mpsc::channel();
            let (release, blocked) = std::sync::mpsc::channel();
            let occupied = tokio::task::spawn_blocking(move || {
                started.send(()).unwrap();
                blocked.recv().unwrap();
            });
            waiting.recv().unwrap();
            let flag = Arc::new(AtomicBool::new(false));
            let changed = Arc::new(AtomicBool::new(false));
            let value = changed.clone();
            let request = tokio::spawn(
                REQUEST_REVOKED
                    .scope(Some(flag.clone()), async move { off("error.test", move || value.store(true, Ordering::Release)).await }),
            );
            tokio::task::yield_now().await;
            flag.store(true, Ordering::Release);
            release.send(()).unwrap();
            occupied.await.unwrap();
            assert_eq!(request.await.unwrap().unwrap_err(), "error.channel.revoked");
            assert!(!changed.load(Ordering::Acquire));
        });
    }
}
