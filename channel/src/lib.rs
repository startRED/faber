//! The local channel between a running Faber and its command line (`faber`, in `cli/`). Faber serves it with whatever
//! hosts its commands (the `Backend`); a client connects, sends the same commands the screen sends, with the same names and
//! arguments, and may listen to the same events. It is a named pipe on Windows and a Unix socket elsewhere, and only the
//! signed-in user can open it: never a network port.
//!
//! The wire: one JSON object per line. A request is `{"id":1,"cmd":"state","args":{}}`, its reply `{"id":1,"ok":...}` or
//! `{"id":1,"err":"error.key"}`, and an event `{"event":{...}}`. Two commands belong to the channel itself and never to the
//! command table: `hello` (who is asking, and what Faber offers) and `subscribe` (events start on this connection).

pub mod client;
pub mod guard;
#[cfg(feature = "host")]
pub mod host;
pub mod place;
pub mod server;
pub mod wire;

pub use client::Client;
pub use place::Address;
pub use server::{Backend, Hello, serve};

/// The protocol level this build speaks. A newer Faber keeps answering older clients; a change that cannot raises it.
pub const PROTOCOL: u32 = 1;
