//! The channel over Faber's own command table (`faber_host::Host`), for the process that runs the engine. Behind the
//! `host` feature, so the command line itself never builds the engine.

use crate::server::{Answer, Backend, DetailedAnswer, Who};
use faber_host::{Caller, Host, Kind};
use serde_json::Value;
use std::io;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::broadcast;

/// A command that needs the window is refused here, whatever a front end with a window would do with it: the command line
/// never moves, sizes or reads the window or the browser pane.
impl Backend for Arc<Host> {
    fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").into()
    }
    fn commands(&self) -> Value {
        serde_json::to_value(Host::exported().collect::<Vec<_>>()).unwrap_or_default()
    }
    fn starts_turn(&self, cmd: &str) -> bool {
        Host::kind(cmd) == Some(Kind::StartsTurn)
    }
    fn dispatch(&self, who: &Who, cmd: String, args: Value) -> Answer {
        let host = self.clone();
        let caller = match who {
            Who::Person => Caller::Person,
            Who::Agent { conversation, .. } => Caller::Agent { conversation: conversation.clone() },
        };
        Box::pin(async move {
            match Host::kind(&cmd) {
                Some(Kind::Window) => Err("error.host.noWindow".into()),
                // The command line shows the key; the field a look error names (its detail) stays with the screen.
                _ => host.dispatch_as(&caller, &cmd, args).await.map_err(|failure| failure.key),
            }
        })
    }
    fn dispatch_detailed(&self, who: &Who, cmd: String, args: Value) -> DetailedAnswer {
        let host = self.clone();
        let caller = match who {
            Who::Person => Caller::Person,
            Who::Agent { conversation, .. } => Caller::Agent { conversation: conversation.clone() },
        };
        Box::pin(async move {
            if Host::kind(&cmd) == Some(Kind::Window) {
                return Err("error.host.noWindow".into());
            }
            host.dispatch_as(&caller, &cmd, args).await.map_err(|failure| crate::wire::Error { key: failure.key, data: failure.data })
        })
    }
    #[cfg(windows)]
    fn started_by_an_ai(&self, client: &faber_winproc::PipeClient) -> bool {
        faber_engine::proc::in_a_job_we_started(|job| client.in_job(job))
    }

    fn subscribe(&self) -> broadcast::Receiver<Value> {
        Host::subscribe(self)
    }
}

/// Serves the channel of `data` over `host` until the process ends. Only one Faber per data folder may: a second one gets
/// an error here.
pub async fn serve(data: &Path, host: Arc<Host>) -> io::Result<()> {
    crate::serve(data, Arc::new(host)).await
}
