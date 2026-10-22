//! Look around (step 3): the engine side of `look.rs`. Once a reply is done, the text its changes removed is looked for in
//! the rest of the conversation's folder, on a blocking thread and within `Limits::AFTER_TURN`, so the turn ends as fast as
//! before. The places found go on the reply's offer, or make one of their own, and the screen gets the reply again.
use super::*;
use crate::look::{self, Limits};
use crate::look_log;

impl Core {
    /// Step 4: one line in the local count (`look_log`), written on a blocking thread so no turn waits for the disk.
    pub(super) fn count_look(&self, event: &'static str, sign: &str, area: Area) {
        let (dir, at, sign) = (self.opts.data_dir.clone(), now(), sign.to_string());
        self.opts.runtime.spawn_blocking(move || look_log::note(&dir, at, event, &sign, area));
    }

    /// A message is being sent: when it is the request the "Look around" button writes, and the newest answer carries an
    /// offer, that offer was used. The engine knows it from the text itself, the screen passes nothing.
    pub(super) fn look_used(&self, conv: &str, text: &str) {
        if !look_log::is_request(text) {
            return;
        }
        let Some(c) = self.convs.get(conv) else { return };
        if let Some(look) = c.messages.iter().rev().find(|m| m.role == Role::Assistant).and_then(|m| m.look.as_ref()) {
            self.count_look("used", &look.reason, c.area);
        }
    }

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
        // An offer that only gained its places was counted when the reply ended; one made by the places is new.
        let fresh = if m.look.is_none() { offer.as_ref().map(|o| o.reason.clone()) } else { None };
        m.look = offer;
        if let (Some(sign), Some(area)) = (fresh, self.convs.get(conv).map(|c| c.area)) {
            self.count_look("offered", &sign, area);
        }
        self.save(conv);
        self.emit_message(conv, reply);
    }
}
