//! What the pane needs from Faber around it: the conversations' tabs (kept by the engine), the screen it tells, and the
//! person's Firefox. A trait so the pane's rules run without an engine, a window or the person's files.

use super::firefox;
use crate::BoxFuture;
use faber_engine::browser::tabs::Tabs;
use faber_engine::{Engine, TabStep};
use serde_json::{Value, json};
use std::sync::{Arc, OnceLock};

/// Something the screen should hear about the pane.
#[derive(Debug, Clone, PartialEq)]
pub enum PaneEvent {
    /// The page the person sees starts (true) or stops (false) loading: the thin line under the address field.
    Loading(bool),
    /// The page the person sees finished at this address; the screen names the page by it.
    Page(String),
    /// The tabs of this conversation changed.
    Tabs(String),
    /// The agent starts (true) or stops (false) working in this tab.
    Busy { conversation: String, tab: String, busy: bool },
}

impl PaneEvent {
    /// The event as the screen receives it: its name and what it carries.
    pub fn wire(&self) -> (&'static str, Value) {
        match self {
            PaneEvent::Loading(on) => ("faber:site-loading", json!(on)),
            PaneEvent::Page(url) => ("faber:site", json!(url)),
            PaneEvent::Tabs(conversation) => ("faber:site-tabs", json!(conversation)),
            PaneEvent::Busy { conversation, tab, busy } => {
                ("faber:site-busy", json!({ "conversation": conversation, "tab": tab, "busy": busy }))
            }
        }
    }
}

/// Faber around the pane. The methods that touch the conversation's file are asynchronous so an implementation can leave the
/// pane's thread for them.
pub trait Home: Send + Sync + 'static {
    /// A conversation's tabs; none for one that is gone or never had a page.
    fn tabs(&self, conversation: String) -> BoxFuture<'_, Option<Tabs>>;
    /// Does `step` to a conversation's tabs and answers them with the tab it concerns.
    fn tab(&self, conversation: String, step: TabStep) -> BoxFuture<'_, Option<(Tabs, Option<String>)>>;
    /// A page finished in a tab: kept in the conversation. Answers whether the tabs changed.
    fn seen(&self, conversation: String, tab: Option<String>, url: String, title: Option<String>) -> BoxFuture<'_, bool>;
    /// Tells the screen.
    fn tell(&self, event: PaneEvent);
    /// The Firefox profiles whose sign-ins can come over. Reads files: called off the pane's thread.
    fn profiles(&self) -> Vec<firefox::Profile> {
        firefox::profiles()
    }
    /// The sites of a profile that hold cookies. Reads files: called off the pane's thread.
    fn sites(&self, profile: &firefox::Profile, now: i64) -> Result<Vec<firefox::Site>, &'static str> {
        firefox::sites(profile, now)
    }
    /// The cookies of those hosts. Reads files: called off the pane's thread.
    fn cookies(&self, profile: &firefox::Profile, hosts: &[String], now: i64) -> Result<Vec<firefox::Cookie>, &'static str> {
        firefox::cookies(profile, hosts, now)
    }
}

/// The real [`Home`]: the engine's tabs, and the events handed to whoever carries them to the screen. The engine is built
/// after the browser it is given, so it is attached once it exists.
pub struct EngineHome {
    engine: OnceLock<Engine>,
    tell: Box<dyn Fn(PaneEvent) + Send + Sync>,
}

impl EngineHome {
    pub fn new(tell: impl Fn(PaneEvent) + Send + Sync + 'static) -> Arc<EngineHome> {
        Arc::new(EngineHome { engine: OnceLock::new(), tell: Box::new(tell) })
    }

    /// Gives the home its engine; until then it knows no conversation.
    pub fn attach(&self, engine: Engine) {
        let _ = self.engine.set(engine);
    }

    /// Runs `work` on the engine off the async threads; the default answer before the engine is attached or when the work
    /// panicked.
    fn with<T: Default + Send + 'static>(&self, work: impl FnOnce(Engine) -> T + Send + 'static) -> BoxFuture<'_, T> {
        let engine = self.engine.get().cloned();
        Box::pin(async move {
            let Some(engine) = engine else { return T::default() };
            tokio::task::spawn_blocking(move || work(engine)).await.unwrap_or_default()
        })
    }
}

impl Home for EngineHome {
    fn tabs(&self, conversation: String) -> BoxFuture<'_, Option<Tabs>> {
        self.with(move |engine| engine.browser_tabs(&conversation))
    }

    fn tab(&self, conversation: String, step: TabStep) -> BoxFuture<'_, Option<(Tabs, Option<String>)>> {
        self.with(move |engine| engine.browser_tab(&conversation, step))
    }

    fn seen(&self, conversation: String, tab: Option<String>, url: String, title: Option<String>) -> BoxFuture<'_, bool> {
        self.with(move |engine| engine.browser_seen(&conversation, tab.as_deref(), &url, title.as_deref()))
    }

    fn tell(&self, event: PaneEvent) {
        (self.tell)(event);
    }
}
