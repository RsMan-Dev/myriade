use crate::*;


/// Why the server refused a client's crossing: it put the player back where it has them,
/// with a position packet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PassageCrossRefusal {
    /// The player's level has no passage with this number.
    UnknownPassage,
    /// A teleport, or an earlier crossing, waits for the client's answer.
    Pending,
    /// The client has not loaded the level around the player yet.
    NotLoaded,
    /// The player is dead.
    Dead,
    /// The player sleeps.
    Sleeping,
    /// The player rides something.
    Riding,
    /// The move to where the client crossed from failed the server's move checks.
    MoveRefused,
    /// Where the client crossed from is more than `limit` blocks from the nearest point of
    /// the opening.
    TooFar {
        /// How far, in blocks: 20, Immersive Portals' limit.
        limit: u32,
    },
    /// The opening's chunk is not loaded or not tracked for the player.
    NearEndNotTracked,
    /// The chunk the player comes out in is not loaded or not tracked for the player.
    FarEndNotTracked,
    /// A mod's `player_passage_cross` cancelled the crossing.
    Vetoed,
    /// The passage leads into another level, which a crossing cannot reach yet.
    FarLevel,
}
