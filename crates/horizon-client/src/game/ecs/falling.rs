//! The outputs of P's falling blocks (`FallingEvent`) as the host applies them: the landing's `fallOn` and HIT_GROUND, the break's
//! sound, level event and item. `ecs_drop_block` is the host's call to P for a block that falls.
use crate::game::Game;
use horizon_core::{geometry::Vec3, nbt::{Compound, Tag}, position::BlockPos};
use horizon_ecs_proto::{host_out_falling::FallingEvent, targeting::Who};
use horizon_world::{level::LevelEvent, registry::BlockStateId};

impl Game<'_> {
    /// One `FallingEvent` of P, applied by the host's own functions.
    pub(in crate::game) fn ecs_falling(&mut self, ev: FallingEvent) {
        match ev {
            FallingEvent::Landed { entity, at, fall, data } => {
                let id = self.ecs_id_of(entity);
                self.ecs_landed(id, at, fall);
                if let Some(carried) = data {
                    self.ecs_landed_data(at, carried);
                }
            }
            FallingEvent::Broke { at, state, drop } => self.falling_broke(at, state, drop),
            // Only a player is hurt here: P applies a hit on an entity itself.
            FallingEvent::Hurt { who: Who::Player(id), at, amount, kind, .. } => {
                self.with_player(id, |g| g.falling_hurt_player(Vec3::new(at[0], at[1], at[2]), amount, kind));
            }
            FallingEvent::Hurt { .. } => {}
        }
    }

    /// The block entity of a falling block that lands with `carried` data: its saved defaults, with the carried fields over them.
    fn ecs_landed_data(&mut self, at: [f64; 3], carried: Compound) {
        let pos = [at[0].floor() as i32, at[1].floor() as i32, at[2].floor() as i32];
        let key = (pos[0], pos[1], pos[2]);
        let name = self.data_block_name(pos);
        let Some(entity) = self.world.block_entities.get(&key) else { return };
        let mut tag = horizon_client::save::block_entity_encode::encode(key, entity, &name);
        if let Tag::Compound(fields) = &mut tag {
            fields.extend(carried);
        }
        self.load_block_entity_tag(pos, tag);
    }

    /// `Fallable.onBrokenAfterFall` of `state` at `at`, then its item when `drop` and the gamerule allows it.
    fn falling_broke(&mut self, at: [f64; 3], state: BlockStateId, drop: bool) {
        let name = self.res.facts().get(state).name.clone();
        let cell = BlockPos::new(at[0].floor() as i32, at[1].floor() as i32, at[2].floor() as i32);
        let sound_event = match &*name {
            "minecraft:pointed_dripstone" => Some(1045),
            "minecraft:sulfur_spike" => Some(1052),
            _ if name.ends_with("anvil") => Some(1029),
            _ => None,
        };
        if let Some(id) = sound_event {
            self.level_events.push(LevelEvent::LevelEvent { id, pos: cell, data: 0, source: None });
        } else if name.starts_with("minecraft:suspicious_") {
            // BrushableBlock.onBrokenAfterFall.
            self.level_events.push(LevelEvent::LevelEvent { id: 2001, pos: cell, data: state.0 as i32, source: None });
            self.level_events.push(LevelEvent::GameEvent { pos: cell, event: "minecraft:block_destroy", state: None });
        }
        if drop && self.session.rules.bool_at(horizon_client::gamerules::vanilla::ENTITY_DROPS) {
            self.drop_falling_block(state, Vec3::new(at[0], at[1], at[2]));
        }
    }
}
