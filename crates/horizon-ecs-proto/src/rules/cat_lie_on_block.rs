//! Calculations of the cat's lying on a block: the ticks between two searches, and the block it can lie on.
use crate::rules::cat_state::empty_above;
use horizon_client_lib::mobs::MobWorld;

/// The ticks between two searches: `asks` asks of the `MoveToBlock` countdown, two ticks each, plus one.
pub fn search_ticks(asks: i32) -> i32 {
    2 * (asks + 1)
}

/// `Mob`'s `can_lie_on`: air above a block of the `cats_can_lie_on` tag.
pub fn can_lie_on(mw: &MobWorld, p: [i32; 3]) -> bool {
    empty_above(mw, p) && mw.has_tag("minecraft:cats_can_lie_on", mw.block_name(p[0], p[1], p[2]))
}
