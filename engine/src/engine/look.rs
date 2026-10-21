//! Look around (step 3): the engine side of `look.rs`. Once a reply is done, the text its changes removed is looked for in
//! the rest of the conversation's folder, on a blocking thread and within `Limits::AFTER_TURN`, so the turn ends as fast as
//! before. The places found go on the reply's offer, or make one of their own, and the screen gets the reply again.
use super::*;
use crate::look::{self, Limits};

impl Core {
    /// After a reply ended: look for what its changes removed. A reply that removed nothing, or a conversation with no
    /// folder that passes the launch's check, starts no read.
    pub(super) fn look_around(&mut self, conv: &str, reply: &str) {
        let Some(m) = self.convs.get(conv).and_then(|c| c.messages.iter().find(|m| m.id == reply)) else { return };
        let needles = look::removed(m);
        if needles.is_empty() {
            return;
        }
        let Some(root) = self.conversation_folder(conv) else { return };
        let (me, runtime, conv, reply) = (self.me.clone(), self.opts.runtime.clone(), conv.to_string(), reply.to_string());
        self.opts.runtime.spawn_blocking(move || {
            let places = look::search(&root, &needles, Limits::AFTER_TURN);
            if places.is_empty() {
                return;
            }
            let Some(core) = me.upgrade() else { return };
            Engine { core, runtime }.with(|c| c.looked_around(&conv, &reply, places));
        });
    }

    /// The places are known: the reply's offer names them, or they make one, unless the reply changed meanwhile so that it
    /// offers something else.
    fn looked_around(&mut self, conv: &str, reply: &str, places: Vec<String>) {
        let Some(m) = self.convs.get_mut(conv).and_then(|c| c.messages.iter_mut().find(|m| m.id == reply)) else { return };
        let offer = look::with_places(m, places);
        if offer == m.look {
            return;
        }
        m.look = offer;
        self.save(conv);
        self.emit_message(conv, reply);
    }
}
