//! Faber's own command table (R459), as a conversation's agent reaches it through the `faber` tool (R585). The host holds the
//! table and the rule of who may send each command (R499); the engine only carries a call there and the answer back, so a
//! command run by an agent passes the very check a command from the command line passes.

use serde_json::Value;
use std::future::Future;
use std::pin::Pin;

/// The most commands one `find` answers: enough to pick from, few enough to cost little.
pub const MAX_FOUND: usize = 8;
/// The most of a `find` request that is read: a few words say what to do, and scoring stays short.
pub const MAX_WORDS: usize = 16;
pub const MAX_WORDS_CHARS: usize = 200;

/// Where the calling agent works, which decides what it may find and run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scope {
    /// Plan and Isolated, which change nothing: only commands that truly only read.
    pub read_only: bool,
    /// A project conversation; a Chat never finds or runs a project's own commands.
    pub project: bool,
}

/// The command table, given to the engine by the front end that holds it (`Engine::set_commands`).
pub trait Commands: Send + Sync {
    /// The commands an agent in `scope` may run that best match `words`, at most [`MAX_FOUND`], each as
    /// `{name, about, kind, reach, inputSchema}`. A command `run` would refuse in that scope is never among them.
    fn find(&self, words: &str, scope: Scope) -> Vec<Value>;

    /// Runs `command` with `args` for the agent of `conversation`, under that conversation's reach and `scope`. A failure
    /// is the failure as the command line receives it: its key, and the request id when the person was asked.
    fn run(
        &self,
        conversation: String,
        command: String,
        args: Value,
        scope: Scope,
    ) -> Pin<Box<dyn Future<Output = Result<Value, String>> + Send>>;
}
