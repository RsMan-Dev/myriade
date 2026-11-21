//! `host.request_world(folder)`: a server has no title screen, so a native mod's request is answered with a code and
//! nothing is queued (the title's question is client UI, removed with the client).
use super::*;
use mod_api::NativeWorldResult;

/// A request the title's next frame would take: the folder, and the mods it is asked for, by name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Request {
    pub(crate) folder: String,
    pub(crate) mods: String,
}

impl Game<'_> {
    /// For a native mod: no title screen here, so the answer is always `NotTitle`.
    pub(in crate::game) fn request_world(&mut self, _folder: &str) -> NativeWorldResult {
        NativeWorldResult::NotTitle
    }

    /// Nothing is queued on a server: there is no title frame to take a request.
    pub(super) fn drain_native_world_request(&mut self) -> Result<()> {
        self.native_world_request = None;
        Ok(())
    }
}
