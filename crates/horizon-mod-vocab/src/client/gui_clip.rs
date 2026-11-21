//! What `push_clip` and `pop_clip` answer when they
//! change nothing.

/// Why `push_clip` or `pop_clip` changed nothing.
/// The clip stack is as it was before the call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
#[derive(horizon_mod_api_macros::Wire)]
pub enum GuiClipRefusal {
    /// A coordinate or size of the rectangle is NaN or infinite.
    NotFinite,
    /// 64 clips are already open in this hook call.
    TooDeep,
    /// `pop_clip` with no clip open in this hook call.
    NoneOpen,
}
