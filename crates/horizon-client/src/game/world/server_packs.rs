//! A server's resource packs are the client's (the prompt, the downloads, the reload with the packs on top): they
//! left with the client. The server keeps only the slot `Game` holds for them.

/// The game's slot for a server's packs: none is tracked on the server.
#[derive(Default)]
pub(super) struct ServerPackState;
