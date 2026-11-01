//! Every command Faber answers, with its arguments and what it does to the person's work (`Kind`). The names and the
//! camelCase keys are the ones `ui/api.ts` sends.

use super::*;
use crate::queues::{PromptReads, SearchQueue};
use crate::{folder, look, project_commands, setup, terminal, tools, views};
use base64::Engine as _;
use faber_engine::model::{Area, Conversation, Project};
use faber_engine::project::checks;
use faber_engine::{AppState, BranchRequest, SendRequest, WarmRequest};
use look::{Look, ThemeFile};
use serde_json::Map;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// What `state` answers: the engine's state, with the look of the saved appearance (the screen applies it before it renders),
/// the models this account can use, and the version running.
#[derive(Serialize)]
struct FullState {
    #[serde(flatten)]
    state: AppState,
    look: Look,
    models: Vec<ModelInfo>,
    version: String,
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

const JSON_FILES: Filter = Filter { name: "JSON", extensions: &["json"] };
const MARKDOWN_FILES: Filter = Filter { name: "Markdown", extensions: &["md"] };

/// Runs `work` off the async threads. `lost` is the error key if the work panicked.
async fn off<T: Send + 'static>(lost: &str, work: impl FnOnce() -> T + Send + 'static) -> Res<T> {
    tokio::task::spawn_blocking(work).await.map_err(|_| lost.to_string())
}

/// What a dialog gave, as the error key of the command that asked: `not_local` for a path that is not on this computer.
fn picked(result: Picked, not_local: &str) -> Result<Option<PathBuf>, Failure> {
    result.map_err(|why| Failure::key(if why == PickError::NoWindow { NO_WINDOW } else { not_local }))
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
    Read Screen fn open(h, id: String) -> Option<Conversation> {
        // Whatever it heard of a streaming reply, the screen now takes this snapshot: the next update of each goes whole.
        lock(&h.feed).forget();
        Ok(h.engine.open_for_screen(&id))
    }
    /// The conversations, the running ones first, then pinned, then the newest, each with its state in plain words
    /// (`working`, `waiting`, `idle`; read one conversation for how it ended). `state` keeps only these states (`running` is
    /// working or waiting); `archived` true lists only archived ones and false every one; `family` keeps the family of a
    /// conversation (who started whom); `query` keeps titles holding its words. Reads no conversation's messages.
    Read Free fn conversations(
        h,
        state: Option<Vec<String>>,
        archived: Option<bool>,
        project_id: Option<String>,
        family: Option<String>,
        query: Option<String>,
        limit: Option<usize>,
    ) -> Vec<views::Row> {
        let filter = views::Filter { states: state.unwrap_or_default(), archived, project_id, family, query, limit };
        Ok(views::rows(&h.engine.state(), &filter))
    }
    /// One conversation: its state (`concluded` only when its AI concluded the whole request), its last `last` messages (all
    /// when missing) with the tools each used, and the AI's latest conclusion. Unlike opening it on the screen, reading it
    /// leaves its done mark for the person.
    Read Free fn conversation(h, id: String, last: Option<usize>) -> views::Read {
        let running = h.engine.state().running.items.into_iter().find(|i| i.conversation_id == id).map(|i| i.state);
        let engine = h.engine.clone();
        let conversation = off("error.conversationGone", move || engine.open(&id)).await?.ok_or("error.conversationGone")?;
        Ok(views::read(&conversation, running, last))
    }
    /// The projects Faber knows, with their folders.
    Read Free fn projects(h) -> Vec<Project> {
        Ok(h.engine.state().projects)
    }
    /// Each vendor's quota windows (how much is used, when each resets) and what Faber has spent of them.
    Read Free fn quota(h) -> Value {
        let state = h.engine.state();
        Ok(json!({ "quotas": state.quotas, "usage": state.usage }))
    }
    /// The settings as Settings shows them.
    Read Free fn settings(h) -> Settings {
        Ok(h.shell.shown_settings(h.engine.settings()))
    }
    /// Everything the screen draws its first frame from; the command line reads `conversations`, `projects` and `quota`.
    Read Screen fn state(h) -> FullState {
        lock(&h.feed).forget();
        let mut state = h.engine.state();
        state.settings = h.shell.shown_settings(std::mem::take(&mut state.settings));
        let look = look::stored(state.settings.appearance.as_ref());
        Ok(FullState { state, look, models: lock(&h.models).clone(), version: h.shell.version() })
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
    StartsTurn Free fn new_group_chat(h, group_id: String) -> Conversation {
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
    Write Own("/id") fn wait_turn(h, id: String, turn_id: String) -> () {
        h.engine.wait_turn(&id, &turn_id)
    }
    /// Go on with a reply Faber was closed under (R477): one fixed turn, refused once it is no longer the last.
    Write Own("/id") fn continue_closed(h, id: String, turn_id: String) -> () {
        h.engine.continue_closed(&id, &turn_id)
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
    Write Own("/id") fn interrupt(h, id: String) -> () {
        h.engine.interrupt(&id);
        Ok(())
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
    StartsTurn Own("/req/conversationId") fn branch(h, req: BranchRequest) -> String {
        let engine = h.engine.clone();
        off("error.copy.failed", move || engine.branch(req)).await?
    }
    /// "Branch from here" on an answer: a new conversation with every message up to it, where nothing is sent yet.
    StartsTurn Own("/conversationId") fn branch_here(h, conversation_id: String, message_id: String) -> String {
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
        off("error.copy.failed", move || engine.remove_with(&id, discard.unwrap_or(false))).await?
    }
    /// Archives a conversation, or brings an archived one back.
    Write Free fn archive(h, id: String, on: bool) -> () {
        h.engine.archive(&id, on)
    }
    Write Free fn pin(h, id: String, on: bool) -> () {
        h.engine.pin(&id, on)
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
    Write Own("/id") fn remove_queued(h, id: String, turn_id: String) -> () {
        h.engine.remove_queued(&id, &turn_id);
        Ok(())
    }
    Write Own("/id") fn edit_queued(h, id: String, turn_id: String, text: String) -> () {
        h.engine.edit_queued(&id, &turn_id, &text)
    }
    StartsTurn Own("/id") fn send_queued(h, id: String, turn_id: String) -> () {
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
    Read Free fn changes(h, id: String, cursor: Option<String>) -> faber_engine::changes::ChangeList {
        static LISTING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
        static WAITING: Mutex<Option<tokio::sync::oneshot::Sender<()>>> = Mutex::new(None);
        let (replaced, gone) = tokio::sync::oneshot::channel();
        if let Some(older) = lock(&WAITING).replace(replaced) {
            let _ = older.send(());
        }
        let _one = tokio::select! {
            biased;
            _ = gone => return Ok(faber_engine::changes::ChangeList::default()),
            one = LISTING.lock() => one,
        };
        let engine = h.engine.clone();
        Ok(tokio::task::spawn_blocking(move || engine.changes_page(&id, cursor.as_deref())).await.unwrap_or_default())
    }
    Read Free fn change_patch(h, id: String, path: String) -> faber_engine::changes::ChangePatch {
        // The newest read waiting for each (conversation, file); a newer one answers the older with nothing.
        type Waiting = std::collections::HashMap<(String, String), tokio::sync::oneshot::Sender<()>>;
        static READING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
        static WAITING: std::sync::LazyLock<Mutex<Waiting>> = std::sync::LazyLock::new(|| Mutex::new(Waiting::new()));
        let key = (id.clone(), path.clone());
        let (replaced, gone) = tokio::sync::oneshot::channel();
        if let Some(older) = lock(&WAITING).insert(key.clone(), replaced) {
            let _ = older.send(());
        }
        let _one = tokio::select! {
            biased;
            _ = gone => return Ok(faber_engine::changes::ChangePatch::default()),
            one = READING.lock() => one,
        };
        lock(&WAITING).remove(&key);
        let engine = h.engine.clone();
        Ok(tokio::task::spawn_blocking(move || engine.change_patch(&id, &path)).await.unwrap_or_default())
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
    /// A quick question on the side of a conversation (R250); `model` is the tab's own pick, or the conversation's.
    StartsTurn Own("/id") fn ask_aside(h, id: String, question: String, model: Option<String>) -> String {
        h.engine.ask_aside(&id, &question, model)
    }
    Write Free fn stop_aside(h, aside_id: String) -> () {
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
    Write Free fn unfollow_pr(h, id: String) -> () {
        h.engine.unfollow_pr(&id);
        Ok(())
    }
    Write Ask fn remove_project(h, id: String) -> () {
        h.engine.remove_project(&id)
    }

    /// The look of an appearance that is not saved. Failing pairs come back inside the look.
    Read Screen fn preview_look(h, appearance: Map<String, Value>) -> Look [Failure] {
        look::preview(&appearance)
    }
    /// A theme file comes from other people, so an appearance is checked here before anything is saved.
    Write Ask fn save_settings(h, patch: Settings) -> SavedSettings [Failure] {
        let mut patch = patch;
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
    /// The recent folders and saved environments, judged now: a folder that is gone or refused carries a reason key.
    Read Free fn recents(h) -> faber_engine::recents::RecentsView {
        Ok(h.engine.recents())
    }
    Write Person fn open_folders(h, paths: Vec<String>) -> Vec<Project> {
        h.engine.open_folders(&paths)
    }
    Write Free fn forget_recent(h, path: String) -> () {
        h.engine.forget_recent(&path)
    }
    Write Ask fn save_environment(h, name: String, folders: Vec<String>) -> () {
        h.engine.save_environment(&name, &folders)
    }
    Write Ask fn forget_environment(h, id: String) -> () {
        h.engine.forget_environment(&id)
    }
    Write Ask fn start_environment(h, id: String) -> Vec<Project> {
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
        std::fs::copy(&source, target).map_err(|_| "error.file.save")?;
        Ok(true)
    }
    Read Ask fn show_file(h, id: String, path: String) -> () {
        let Some(source) = h.engine.folder_file(&id, &path) else { return Ok(()) };
        h.shell.reveal(source)
    }
    /// Shows a project's folder in the file manager. The saved path is checked first (folder.rs), and the folder is only
    /// revealed, never opened: if something else took its place, the file manager selects it and starts nothing.
    Read Ask fn show_project(h, id: String) -> () {
        let project = h.engine.project(&id)?;
        let folder = folder::local_folder(&project.path)?;
        h.shell.reveal(folder)
    }
    /// A file a reply names, read only around its line (parity P2), from inside the conversation's folder.
    Read Free fn read_file_at(h, id: String, path: String) -> faber_engine::fileview::FileView {
        let engine = h.engine.clone();
        Ok(tokio::task::spawn_blocking(move || engine.read_file_at(&id, &path))
            .await
            .unwrap_or_else(|_| faber_engine::fileview::FileView::failed("error.file.read")))
    }
    /// An image from a chat workspace as a data URL, so the page never reads the disk itself.
    Read Free fn read_image(h, id: String, path: String) -> Option<String> {
        let Some(source) = h.engine.workspace_file(&id, &path) else { return Ok(None) };
        let ext = source.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
        let Some((_, kind)) = IMAGE_TYPES.iter().find(|(e, _)| Some(*e) == ext.as_deref()) else { return Ok(None) };
        Ok(data_url(&source, kind, 8_000_000))
    }
    /// A conversation's tabs in Faber's browser, kept in its file, so the pane opens the front one again.
    Read Free fn site_tabs(h, id: String) -> Option<faber_engine::browser::tabs::Tabs> {
        // Reading a conversation's file waits on the engine, never on the window's thread.
        let engine = h.engine.clone();
        off("error.conversationGone", move || engine.browser_tabs(&id)).await
    }
    /// The picture the browser took for a call, as a data URL, so the page never reads the disk itself.
    Read Free fn site_picture(h, id: String, call: String) -> Option<String> {
        let Some(source) = h.engine.site_picture(&id, &call) else { return Ok(None) };
        Ok(data_url(&source, "image/jpeg", 2_000_000))
    }
    Write Person fn open_terminal(h, id: String) -> bool {
        let Some(cmd) = h.engine.terminal(&id) else { return Ok(false) };
        terminal::open(&cmd).map_err(|_| "error.terminal.open")?;
        Ok(true)
    }
    /// Links in answers open in the user's browser, never inside Faber.
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

    /// Settings > Projects: whether each new conversation of this project works in a separate copy (R201).
    Write Ask fn project_copies(h, project_id: String, on: bool) -> () {
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
    /// The project's switch for starting fixes by itself. Off until the person turns it on.
    Write Ask fn project_goals_auto(h, project_id: String, on: bool) -> faber_engine::project::goals::View {
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
    Write Ask fn edit_project_memory(h, project_id: String, edit: faber_engine::project::memory::Edit) -> () {
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

    /// The screen's view of the update: its phase and the download's progress. Changes arrive as `faber:update`.
    Read Free fn update_status(h) -> Value {
        Ok(h.shell.update_status())
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

    /// Faber's browser pane (R161 step 7) over the screen's own rectangle for it, in the page's logical pixels.
    Window Screen fn site_place(h, load: u32, box_id: u64, x: f64, y: f64, width: f64, height: f64) -> bool {
        Err(NO_WINDOW.into())
    }
    /// The browser pane's page takes the corners of the card the screen shows it in: `radius` in logical pixels and the card's
    /// colour as red, green and blue; a radius of 0 leaves them square.
    Window Screen fn site_round(h, radius: f64, color: [u8; 3]) -> () {
        Err(NO_WINDOW.into())
    }
    /// The screen sizes Faber's browser can draw its pages at and the one in use; with `pick`, `size` becomes the pane's first.
    Window Screen fn site_size(h, pick: bool, size: Option<String>) -> Value {
        Err(NO_WINDOW.into())
    }
    /// The browser pane leaves the screen for the screen's box `box_id` and every older one; it keeps its page.
    Window Screen fn site_park(h, load: u32, box_id: u64) -> () {
        Err(NO_WINDOW.into())
    }
    /// The person opens a page from a reply's card in the browser pane; it is that conversation's page from then on.
    Window Screen fn site_open(h, id: String, url: String) -> bool {
        Err(NO_WINDOW.into())
    }
    /// The person goes back, forward or reloads in the browser pane, on that conversation's page only.
    Window Screen fn site_step(h, id: String, step: String) -> bool {
        Err(NO_WINDOW.into())
    }
    /// The person loads an address they typed in the tab in front of the browser pane.
    Window Screen fn site_go(h, id: String, url: String) -> bool {
        Err(NO_WINDOW.into())
    }
    /// What the person did to the browser pane's tabs, answered with the tabs and the tab it concerns.
    Window Screen fn site_tab(h, id: String, step: String, tab: Option<String>) -> Option<Value> {
        Err(NO_WINDOW.into())
    }
    /// Whether Back and Forward of the browser pane can act on conversation `id`'s page: `[back, forward]`.
    Window Screen fn site_history(h, id: String) -> [bool; 2] {
        Err(NO_WINDOW.into())
    }
    /// Clears conversation `id`'s browsing data once the person said yes: tabs, closed tabs, sign-ins and the page's own data.
    Window Screen fn site_clear(h, id: String) -> () {
        Err(NO_WINDOW.into())
    }
    /// The Firefox profiles on this computer whose sign-ins the browser pane can bring over.
    Window Person fn cookie_profiles(h) -> Value {
        Err(NO_WINDOW.into())
    }
    /// The sites with cookies in the profile `profile`, under a ticket the import names.
    Window Person fn cookie_sites(h, profile: String) -> Value {
        Err(NO_WINDOW.into())
    }
    /// Brings the person's sign-ins on the sites they picked into conversation `id`'s pages; answers how many sites came over.
    Window Person fn cookie_import(h, id: String, ticket: String, sites: Vec<String>) -> usize {
        Err(NO_WINDOW.into())
    }
    /// The sites whose sign-ins conversation `id` holds.
    Window Person fn cookie_held(h, id: String) -> Vec<String> {
        Err(NO_WINDOW.into())
    }
    /// Forgets the sign-ins conversation `id` holds and signs its page out of them.
    Window Person fn cookie_forget(h, id: String) -> () {
        Err(NO_WINDOW.into())
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
