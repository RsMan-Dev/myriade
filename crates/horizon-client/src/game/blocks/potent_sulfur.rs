//! Potent sulfur geysers: the tickers `PotentSulfurBlock.getTicker` gives
//! `PotentSulfurBlockEntity` by the block's state. The server's make living
//! entities near the gas nauseous, count a dormant or erupting geyser down to
//! its next change of state and push the entities an eruption throws up. The
//! client's draw the gas clouds and the eruption's plume, play its sound and
//! push the local player, who moves on its own client and not on the server.
use super::Game;
use horizon_client::{
    Result,
    block_entities::Pos,
    containers::BlockEntity,
    effects::EffectInstance,
    potent_sulfur::{
        Level, can_be_reached_by_noxious_gas, find_noxious_gas_source_block,
        unobstructed_block_count,
    },
};
use horizon_core::{
    geometry::{Aabb, Vec3},
    position::BlockPos,
    random::{PositionalRandom, RandomSource, XoroshiroRandom},
};
use horizon_world::{level::flags, registry::BlockStateId};

/// The per-pass state of the block entity pass; the mobs are read from the ECS.
#[derive(Default)]
pub(super) struct PassMobs;

/// `PotentSulfurBlockEntity.GEYSER_SALT`.
const GEYSER_SALT: i64 = -904_011_478;
/// `PotentSulfurBlockEntity.EFFECT_APPLICATION_FREQUENCY_TICKS`.
const EFFECT_APPLICATION_FREQUENCY_TICKS: i64 = 10;
/// `PotentSulfurBlockEntity.EFFECT_DURATION_IN_TICKS`.
const EFFECT_DURATION_IN_TICKS: i32 = 80;

/// `PotentSulfurState`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Geyser {
    Dry,
    Wet,
    Dormant,
    Erupting,
    Continuous,
}

/// `PotentSulfurBlockEntity.geyserPositional`: the random of the sulfur block
/// at `pos`, forked from the level seed and the geyser salt.
fn geyser_positional(seed: i64, pos: Pos) -> XoroshiroRandom {
    let mut base = XoroshiroRandom::new(seed ^ GEYSER_SALT);
    base.fork_positional()
        .at(BlockPos::new(pos.0, pos.1, pos.2))
}

/// `PotentSulfurBlockEntity.getNearbyLivingEntities`: the box of the source
/// block grown 2.5 blocks sideways.
fn nearby_living_area(source: [i32; 3]) -> Aabb {
    let [x, y, z] = source.map(f64::from);
    Aabb::new(
        Vec3::new(x - 2.5, y, z - 2.5),
        Vec3::new(x + 1.0 + 2.5, y + 1.0, z + 1.0 + 2.5),
    )
}

/// The box `PotentSulfurBlockEntity.LAUNCH_ENTITY_TICKER` pushes entities in:
/// the block above the sulfur, `expandTowards` the unobstructed height.
fn launch_area(pos: Pos, height: i32) -> Aabb {
    let (x, y, z) = (f64::from(pos.0), f64::from(pos.1 + 1), f64::from(pos.2));
    let up = f64::from(height - 1);
    let (min_y, max_y) = if up < 0.0 {
        (y + up, y + 1.0)
    } else {
        (y, y + 1.0 + up)
    };
    Aabb::new(Vec3::new(x, min_y, z), Vec3::new(x + 1.0, max_y, z + 1.0))
}

impl Game<'_> {
    /// The `potent_sulfur_state` of the block at `pos`, with its block state.
    fn potent_sulfur_state(&self, pos: Pos) -> Option<(BlockStateId, Geyser)> {
        let state = self.world.block(pos.0, pos.1, pos.2);
        let value = self
            .pack
            .assets
            .registry
            .state(state)?
            .properties
            .get("potent_sulfur_state")?;
        let geyser = match value.as_str() {
            "dry" => Geyser::Dry,
            "wet" => Geyser::Wet,
            "dormant" => Geyser::Dormant,
            "erupting" => Geyser::Erupting,
            "continuous" => Geyser::Continuous,
            _ => return None,
        };
        Some((state, geyser))
    }

    /// `PotentSulfurBlockEntity.findNoxiousGasSourceBlock` of the sulfur at
    /// `pos`.
    fn potent_sulfur_source(&self, pos: Pos) -> Option<[i32; 3]> {
        let level = Level {
            world: &self.world,
            facts: self.res.facts(),
            collision: self.collision,
        };
        find_noxious_gas_source_block(&level, [pos.0, pos.1, pos.2])
    }

    /// `PotentSulfurBlock.getTicker` on the server, for the block entity at
    /// `pos`: a wet block gives nausea; a dormant one counts down, then gives
    /// nausea; an erupting one pushes entities, then counts down; a continuous
    /// one pushes entities. A dry block has no ticker.
    pub(super) fn tick_potent_sulfur(&mut self, pos: Pos) -> Result<()> {
        let Some((state, geyser)) = self.potent_sulfur_state(pos) else {
            return Ok(());
        };
        match geyser {
            Geyser::Dry => {}
            Geyser::Wet => self.server_nausea_effect_ticker(pos),
            Geyser::Dormant => {
                self.server_waiting_countdown_ticker(pos, state, geyser)?;
                self.server_nausea_effect_ticker(pos);
            }
            Geyser::Erupting => {
                self.launch_entity_ticker(pos);
                self.server_waiting_countdown_ticker(pos, state, geyser)?;
            }
            Geyser::Continuous => self.launch_entity_ticker(pos),
        }
        Ok(())
    }

    /// `PotentSulfurBlockEntity.SERVER_NAUSEA_EFFECT_TICKER`: every 10 ticks,
    /// the living entities near the source block that its gas reaches (at
    /// their eyes) get nausea for 4 seconds.
    fn server_nausea_effect_ticker(&mut self, pos: Pos) {
        if self.world.game_time % EFFECT_APPLICATION_FREQUENCY_TICKS != 0 {
            return;
        }
        let Some(source) = self.potent_sulfur_source(pos) else {
            return;
        };
        let area = nearby_living_area(source);
        self.for_each_player(|g| {
            if g.player.as_ref().is_some_and(|p| !p.health.dead_or_dying() && p.bounds().intersects(area)) {
                g.apply_nausea_effect_to_player(source);
            }
        });
        for (id, kind) in self.ecs_gas_reached(source, area) {
            if self.can_be_affected(Some(id as i32), &kind, "nausea") {
                self.ecs_add_effect(id, "nausea", EFFECT_DURATION_IN_TICKS, 0);
            }
        }
    }

    /// `PotentSulfurBlockEntity.applyNauseaEffect` on the player acting (a
    /// guest is swapped in as the player) when the gas reaches their eyes;
    /// spectators are not affected.
    fn apply_nausea_effect_to_player(&mut self, source: [i32; 3]) {
        if self.spectator() {
            return;
        }
        let Some(p) = self.player.as_ref() else {
            return;
        };
        let eye = [
            p.position.x,
            p.position.y + f64::from(p.dimensions.eye_height),
            p.position.z,
        ];
        let reached = {
            let level = Level {
                world: &self.world,
                facts: self.res.facts(),
                collision: self.collision,
            };
            can_be_reached_by_noxious_gas(&level, source, eye)
        };
        if reached {
            // MobEffectInstance(NAUSEA, 80, 0, true, true).
            let mut nausea = EffectInstance::new("nausea", EFFECT_DURATION_IN_TICKS, 0);
            nausea.ambient = true;
            self.add_player_effect(nausea);
        }
    }

    /// `PotentSulfurBlockEntity.SERVER_WAITING_COUNTDOWN_TICKER`: every second
    /// the countdown runs down; a dormant geyser waits `10 * (water - 1)` plus
    /// 15 to 30, an erupting one `water - 1` plus 1 or 2, and at zero the
    /// block turns erupting (dormant) or dormant (erupting). `state` is the
    /// block state the tick began with.
    fn server_waiting_countdown_ticker(
        &mut self,
        pos: Pos,
        state: BlockStateId,
        geyser: Geyser,
    ) -> Result<()> {
        if self.world.game_time % 20 != 0 {
            return Ok(());
        }
        let Some(source) = self.potent_sulfur_source(pos) else {
            return Ok(());
        };
        let Some(BlockEntity::PotentSulfur(sulfur)) = self.world.block_entities.get_mut(&pos)
        else {
            return Ok(());
        };
        if sulfur.waiting_countdown <= 0 {
            let water_blocks = source[1] - pos.1 - 1;
            let mut positional = geyser_positional(self.level.seed, pos);
            sulfur.waiting_countdown = if geyser == Geyser::Dormant {
                10 * (water_blocks - 1)
                    + positional.next_int_between_inclusive(15, 30).unwrap_or(15)
            } else {
                positional.next_int();
                water_blocks - 1 + positional.next_int_between_inclusive(1, 2).unwrap_or(1)
            };
        }
        if sulfur.waiting_countdown > 0 {
            sulfur.waiting_countdown -= 1;
        }
        if sulfur.waiting_countdown != 0 {
            return Ok(());
        }
        let to_set = if geyser == Geyser::Dormant {
            "erupting"
        } else {
            "dormant"
        };
        let registry = self.pack.assets.registry;
        let Some(mut next) = registry.state(state).cloned() else {
            return Ok(());
        };
        next.properties
            .insert("potent_sulfur_state".into(), to_set.into());
        let Some(next) = registry.id(&next) else {
            return Ok(());
        };
        let at = BlockPos::new(pos.0, pos.1, pos.2);
        self.with_level(|b, level, ticks| b.set_block(level, ticks, at, next, flags::UPDATE_ALL));
        self.world.modified.insert((pos.0 >> 4, pos.2 >> 4));
        if to_set == "dormant" {
            self.post_block_game_event(
                "minecraft:block_deactivate",
                [pos.0, pos.1, pos.2],
                None,
                Some(state),
            );
        }
        self.process_level_events()
    }

    /// `PotentSulfurBlockEntity.LAUNCH_ENTITY_TICKER` on the server: mobs, items,
    /// primed TNT, projectiles, boats and minecarts in the geyser's column with less than
    /// `0.3 + 0.1 * water` upward speed get 0.2 more, unless they ride
    /// something or are of a type geysers do not move. Players are moved by
    /// their own client.
    fn launch_entity_ticker(&mut self, pos: Pos) {
        let Some(source) = self.potent_sulfur_source(pos) else {
            return;
        };
        let water_blocks = source[1] - pos.1 - 1;
        let level = Level {
            world: &self.world,
            facts: self.res.facts(),
            collision: self.collision,
        };
        let height = unobstructed_block_count(&level, [pos.0, pos.1 + 1, pos.2], water_blocks);
        let area = launch_area(pos, height);
        let limit = f64::from(0.3f32) + f64::from(water_blocks) * 0.1;
        let push = f64::from(0.2f32);
        self.ecs_geyser_launch(area, limit, push);
    }


    /// `PotentSulfurBlock.triggerEvent`: the eruption block event stamps the
    /// game time the plume counts from; false when `pos` holds no potent
    /// sulfur block entity.
    pub(super) fn potent_sulfur_eruption_event(&mut self, pos: Pos) -> bool {
        let time = self.world.game_time;
        match self.world.block_entities.get_mut(&pos) {
            Some(BlockEntity::PotentSulfur(sulfur)) => {
                sulfur.eruption_tick = time;
                true
            }
            _ => false,
        }
    }
}

