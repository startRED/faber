use super::*;

pub(super) fn start_conversation_agy(
    identity: Arc<crate::proc::ConversationIdentity>,
    opts: crate::agy::session::AgyOptions,
    servers: &[crate::tools::Launch],
    sink: EventSink,
) -> Result<Session, String> {
    let on_exit = identity.clone();
    let sink: EventSink = Arc::new(move |event| {
        if matches!(&event, EngineEvent::WriterExit { confirmed: true }) {
            on_exit.revoke();
        }
        sink(event);
    });
    let sink = identity.token.as_ref().map_or_else(|| sink.clone(), |token| crate::codex::redact_sink(sink.clone(), token.clone()));
    let session = crate::proc::as_conversation(&identity, || crate::agy::session::start(opts, servers, sink))?;
    Ok(session.with_identity(identity))
}

/// A person stopping Antigravity may require ending its process; that is a stop, not an error.
pub(super) fn interrupted(vendor: Vendor, message: &str) -> bool {
    vendor == Vendor::Gemini && message == "reason.interrupted"
}

impl Core {
    /// Out of quota is the one ending that also marks the account.
    pub(super) fn agy_reason(&mut self, vendor: Vendor, key: &str) {
        if vendor == Vendor::Gemini && key == "reason.fail.limit" {
            self.antigravity_out(None);
        }
    }
}
