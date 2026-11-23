//! A block state's sound type as its parts answer (`IBlockExtension.getSoundType`): what the game
//! reads to play a block's step, fall, place or break sound, or a sniffer's dig.
use crate::blocks::{BlockFacts, Sounds};
use crate::mobs::{MobWorld};
use horizon_core::position::BlockPos;
use horizon_world::{native_load_context::LoopContext, registry::BlockStateId};
use myriade_types::BlockSoundCause;
use myriade_types::BlockSoundType;
use std::sync::Arc;

/// `sounds`, a state's own sound type (`BlockState.getSoundType()`), as the mod API's record.
pub fn own(sounds: &Sounds) -> BlockSoundType {
    BlockSoundType {
        volume: sounds.volume,
        pitch: sounds.pitch,
        break_sound: Arc::clone(&sounds.break_sound),
        step: Arc::clone(&sounds.step),
        place: Arc::clone(&sounds.place),
        hit: Arc::clone(&sounds.hit),
        fall: Arc::clone(&sounds.fall),
    }
}

/// The sound type of `state` at `pos` for `cause`: the state's own (`facts`), through
/// [`native_sound_type`].
pub fn sound_type(
    context: &LoopContext,
    facts: &BlockFacts,
    pos: BlockPos,
    state: BlockStateId,
    entity: Option<(u32, &str)>,
    cause: BlockSoundCause,
) -> BlockSoundType {
    let sound = own(&facts.get(state).sounds);
    sound
}

impl MobWorld<'_> {
}
