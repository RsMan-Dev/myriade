use crate::*;


/// Why a read or an edit of an unspawned mob was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnspawnedMobError {
    /// The retained custom name exceeds the component boundary.
    Text(crate::TextFormError),
    /// The hook that lent the handle has returned, the handle belongs to another lend or world,
    /// or there is no game.
    Expired,
    /// This mob's type has no such attribute, or the game keeps no mob behind this lend (the
    /// ender dragon, which it stores apart from mobs, or a position check where the game creates
    /// the mob later from saved data or the world generator).
    Unsupported,
    /// The value is refused: an unknown attribute, a non-finite number, a modifier id the
    /// attribute already has, an empty or full tag list, or invalid item data.
    Invalid,
}

impl From<crate::TextFormError> for UnspawnedMobError {
    fn from(error: crate::TextFormError) -> Self {
        Self::Text(error)
    }
}

impl From<crate::StackWriteError> for UnspawnedMobError {
    fn from(_: crate::StackWriteError) -> Self {
        Self::Invalid
    }
}
