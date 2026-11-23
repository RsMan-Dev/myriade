// game-module-visibility: pub(crate)
//! The `sound_type` Part method (`IBlockExtension.getSoundType`). The client library asks through
//! `native_level_hooks`; only a state whose block has a subscribing part is converted, and only
//! then is its own sound type kept to fall back on.
use super::api_values::{TryIntoApi, block_state};
use super::{Game, GameLevel};
use horizon_core::{identifier::Identifier, position::BlockPos};
use horizon_world::{
    level::data::Sounds, native_load_context::LoopContext, registry::BlockStateId,
};
use mod_api::{BlockSoundCause, BlockSoundType};
use std::sync::Arc;

/// The game's side of `horizon_client::block_sounds::native_sound_type`.
pub(crate) fn sound_type(
    context: &LoopContext,
    pos: BlockPos,
    state: BlockStateId,
    entity: Option<(u32, &str)>,
    cause: BlockSoundCause,
    sound: &mut BlockSoundType,
) {
    if !block_state::block_listens(mod_api::HookId::SoundType, state) {
        return;
    }
    let Ok(api_state) = state.try_into_api() else {
        return;
    };
    let own = sound.clone();
    mod_api::dispatch_sound_type(
        &horizon_client_render::api_context::LoopReader(context).context(),
        pos,
        api_state,
        entity,
        cause,
        sound,
    );
    if !playable(sound) {
        *sound = own;
    }
}

/// Whether a part's answer can be played: a finite volume and pitch of at least 0, and every
/// event a `namespace:path` id (a mod's own included; one the resources lack plays nothing).
fn playable(sound: &BlockSoundType) -> bool {
    let level = |value: f32| value.is_finite() && value >= 0.0;
    let events = [
        &sound.break_sound,
        &sound.step,
        &sound.place,
        &sound.hit,
        &sound.fall,
    ];
    level(sound.volume)
        && level(sound.pitch)
        && events
            .iter()
            .all(|event| Identifier::parse_namespaced(event).is_some())
}

impl Game<'_> {
    /// The sound type of `state` at `pos` for `cause` (`IBlockExtension.getSoundType`), as the
    /// block's parts answer on this side; `entity` is the id and type of the entity stepping,
    /// landing, placing or digging, and None for a break or a placement by no entity.
    pub(in crate::game) fn block_sound_type(
        &self,
        [x, y, z]: [i32; 3],
        state: BlockStateId,
        entity: Option<(u32, &str)>,
        cause: BlockSoundCause,
    ) -> BlockSoundType {
        horizon_client::block_sounds::sound_type(
            &self.loop_context,
            self.res.facts(),
            BlockPos::new(x, y, z),
            state,
            entity,
            cause,
        )
    }
    /// `LevelEventHandler`'s break sound (level event 2001, which a client's predicted break
    /// also plays): the block's break sound at its centre, at `(volume + 1) / 2` and pitch × 0.8.
    /// None for air.
    pub(in crate::game) fn play_break_sound(&mut self, pos: [i32; 3], state: BlockStateId) {
        if self.res.facts().is_air(state) {
            return;
        }
        let sound = self.block_sound_type(pos, state, None, BlockSoundCause::Break);
        self.audio.play(
            &sound.break_sound,
            Some(pos.map(|v| f64::from(v) + 0.5)),
            (sound.volume + 1.0) / 2.0,
            sound.pitch * 0.8,
        );
    }
}

impl GameLevel<'_> {
    /// `LevelAccess::place_sound_type`: the state's own `sounds` unless a part on its block
    /// answers another, asked with the acting player, or none when no player places it (a
    /// dispenser).
    pub(super) fn part_place_sound_type(
        &self,
        pos: BlockPos,
        state: BlockStateId,
        sounds: Arc<Sounds>,
    ) -> Arc<Sounds> {
        if !block_state::block_listens(mod_api::HookId::SoundType, state) {
            return sounds;
        }
        let entity = self
            .native_player_entity
            .map(|id| (id as u32, "minecraft:player"));
        let own = horizon_client::block_sounds::own(&self.facts.get(state).sounds);
        let mut answer = own.clone();
        horizon_client::block_sounds::native_sound_type(
            self.loop_context,
            pos,
            state,
            entity,
            BlockSoundCause::Place,
            &mut answer,
        );
        if answer == own {
            return sounds;
        }
        Arc::new(Sounds {
            volume: answer.volume,
            pitch: answer.pitch,
            place: answer.place,
            break_: answer.break_sound,
            fall: answer.fall,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::native_entity_refs::tests::with_game;
    use super::playable;
    use horizon_client::mobs::{MobEvent, equine};
    use horizon_core::{geometry::Vec3, position::BlockPos};
    use horizon_world::level::LevelAccess;
    use mod_api::BlockSoundCause::{Break, Dig, Fall, Place, Step};
    use std::sync::Arc;

    #[test]
    fn sound_answers_validate_levels_and_names_without_requiring_loaded_events() {
        let stone = horizon_client::blocks::Sounds {
            volume: 1.0,
            pitch: 1.0,
            break_sound: "minecraft:block.stone.break".into(),
            step: "minecraft:block.stone.step".into(),
            place: "minecraft:block.stone.place".into(),
            hit: "minecraft:block.stone.hit".into(),
            fall: "minecraft:block.stone.fall".into(),
        };
        for value in [-1.0, f32::INFINITY, f32::NEG_INFINITY, f32::NAN] {
            let mut sound = horizon_client::block_sounds::own(&stone);
            sound.volume = value;
            assert!(!playable(&sound));
            sound.volume = 1.0;
            sound.pitch = value;
            assert!(!playable(&sound));
        }
        for value in [0.0, 1.0, 2.0, f32::MAX] {
            let mut sound = horizon_client::block_sounds::own(&stone);
            sound.volume = value;
            sound.pitch = value;
            assert!(playable(&sound));
        }
        for event in ["", "not_namespaced", "mod:bad path"] {
            let mut sound = horizon_client::block_sounds::own(&stone);
            sound.step = event.into();
            assert!(!playable(&sound));
        }
        let mut sound = horizon_client::block_sounds::own(&stone);
        sound.step = "unknown:unloaded_event".into();
        assert!(playable(&sound));
    }

}
