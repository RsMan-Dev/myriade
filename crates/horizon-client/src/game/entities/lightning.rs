//! Lightning (`ServerLevel.tickThunder`, `LightningBolt`,
//! `LightningBoltRenderer`): strikes on the highest motion-blocking block or
//! a living entity under open sky nearby, skeleton trap horses, fire,
//! `thunderHit` (damage and burning, charged creepers, zombified piglins,
//! witches, mooshroom colours, turtles, burnt items), lightning rods, the sky
//! flash, the sounds and the branching additive bolt.
use super::Game;
use horizon_client::random_source;

use horizon_core::{
    position::BlockPos,
    random::{EntityRandom, RandomSource},
};
use horizon_world::level::Ticks;

/// A server's bolt, as this client shows it: its flashes and sound. The server's bolts run in P.
pub(super) struct Bolt {
    /// The entity id a server's players know it by.
    pub id: u32,
    pub at: [f64; 3],
    life: i32,
    flashes: i32,
    seed: i64,
    /// The bolt's own random (`Entity.random`): its seed, flashes, flash
    /// timing and thunder pitch draw from it, never the level's.
    random: EntityRandom,
}

/// `ServerLevel.tickThunder` -> `Level.getBlockRandomPos(minX, 0, minZ, 15)`:
/// picks the strike column from the level's separate `randValue` LCG.
fn thunder_strike_column(ticks: &mut Ticks, cx: i32, cz: i32) -> (i32, i32) {
    let r = ticks.block_random_pos(cx * 16, 0, cz * 16, 15);
    (r.x, r.z)
}

impl Game<'_> {
    /// `ServerChunkCache.tickSpawningChunk`: one `tickThunder` roll per
    /// spawning chunk (within 128 blocks of a player) that is in entity
    /// ticking range.
    pub(super) fn tick_thunder(&mut self) {
        // isRaining (rain level > 0.2) and isThundering (> 0.9) both need
        // Level.canHaveWeather: sky light, no ceiling and not the End.
        let (rain, thunder) = self.world.weather.levels(1.0);
        let weather = self.world.shape.sky_light
            && !self.world.shape.has_ceiling
            && self.dim != super::Dim::End;
        if rain <= 0.2 || thunder <= 0.9 || !weather || self.world_debug {
            return;
        }
        let mut scratch = std::mem::take(&mut self.spawn_scratch);
        self.spawning_chunks(&self.level_players(), &mut scratch);
        let levels = self.tickets.levels();
        scratch
            .candidates
            .retain(|&(x, z)| levels.random_ticking(x, z));
        for (cx, cz) in scratch.candidates.drain(..) {
            if self.world.random.next_int_bound(100_000).unwrap_or(1) != 0 {
                continue;
            }
            // getBlockRandomPos(minX, 0, minZ, 15)
            let (x, z) = thunder_strike_column(&mut self.block_ticks, cx, cz);
            let Some(pos) = self.lightning_target(x, z) else {
                continue;
            };
            let p = BlockPos::new(pos[0], pos[1], pos[2]);
            if !self.with_level(|b, level, _| b.is_raining_at(level, p)) {
                continue;
            }
            // Skeleton trap: the local effective difficulty / 100
            // (`getCurrentDifficultyAt(pos)`).
            let difficulty = f64::from(
                self.local_difficulty([f64::from(pos[0]), f64::from(pos[1]), f64::from(pos[2])])
                    .effective,
            );
            let below = &self
                .res
                .facts()
                .get(self.world.block(pos[0], pos[1] - 1, pos[2]))
                .name;
            let trap = self
                .ui
                .rules
                .bool_at(horizon_client::gamerules::vanilla::SPAWN_MOBS)
                && self.world.random.next_double() < difficulty * 0.01
                && !below.ends_with("lightning_rod");
            if !self.lightning_strike_allowed(None, pos, "weather") {
                continue;
            }
            if trap {
                let at = [f64::from(pos[0]), f64::from(pos[1]), f64::from(pos[2])];
                // ServerLevel.tickChunk: a trap horse waits for a player.
                if self.ecs.is_active() {
                    // The trap flag and age are not on P's horse yet (QUESTION to the referent).
                    self.ecs_spawn_trap_horse(at);
                } else if let Some(id) = self.spawn_mob_raw("minecraft:skeleton_horse", at, false)
                    && let Some(h) = self.remote_entities.iter_mut().find(|m| m.id == id)
                {
                    h.extra.trap = true;
                    h.extra.age = 0;
                }
            }
            let at = [
                f64::from(pos[0]) + 0.5,
                f64::from(pos[1]),
                f64::from(pos[2]) + 0.5,
            ];
            self.spawn_bolt(at, trap);
        }
        self.spawn_scratch = scratch;
    }
    /// `ServerLevel.findLightningRod`: `PoiManager.findClosest` over the
    /// lightning rod points of interest within 128 blocks of `center` whose
    /// block is the top (`WORLD_SURFACE`) of its column; the strike lands one
    /// above it. Horizon keeps no point-of-interest index, so this scans the
    /// loaded chunks the way `/locate poi` does.
    fn find_lightning_rod(&self, center: [i32; 3]) -> Option<[i32; 3]> {
        const RANGE: i32 = 128;
        let facts = self.res.facts();
        let mut is_rod: std::collections::HashMap<u32, bool> = std::collections::HashMap::new();
        let mut best: Option<(i64, [i32; 3])> = None;
        let (cx0, cz0) = ((center[0] - RANGE) >> 4, (center[2] - RANGE) >> 4);
        let (cx1, cz1) = ((center[0] + RANGE) >> 4, (center[2] + RANGE) >> 4);
        for cx in cx0..=cx1 {
            for cz in cz0..=cz1 {
                let Some(chunk) = self.world.chunks.get(&(cx, cz)) else {
                    continue;
                };
                for (i, section) in chunk.sections.iter().enumerate() {
                    let mut rod = |state: u32| {
                        *is_rod.entry(state).or_insert_with(|| {
                            facts
                                .get(horizon_world::registry::BlockStateId(state))
                                .name
                                .ends_with("lightning_rod")
                        })
                    };
                    if !section.blocks.maybe_has(&mut rod) {
                        continue;
                    }
                    let base_y = self.world.bottom_y() + 16 * i as i32;
                    for j in 0..4096 {
                        if !rod(section.get(j)) {
                            continue;
                        }
                        let p = [
                            cx * 16 + (j & 15) as i32,
                            base_y + (j >> 8) as i32,
                            cz * 16 + ((j >> 4) & 15) as i32,
                        ];
                        let d = [p[0] - center[0], p[1] - center[1], p[2] - center[2]]
                            .map(|d| i64::from(d) * i64::from(d))
                            .iter()
                            .sum::<i64>();
                        if d > i64::from(RANGE * RANGE) || best.is_some_and(|(b, _)| d >= b) {
                            continue;
                        }
                        let surface = self.world.world_surface(p[0], p[2], |s| facts.is_air(s));
                        if p[1] == surface - 1 {
                            best = Some((d, p));
                        }
                    }
                }
            }
        }
        best.map(|(_, p)| [p[0], p[1] + 1, p[2]])
    }
    /// `findLightningTargetAround`: the closest lightning rod, else a living
    /// entity under open sky in the column (3 blocks around), else the top of
    /// the column.
    pub(in crate::game) fn lightning_target(&mut self, x: i32, z: i32) -> Option<[i32; 3]> {
        if !self.world.loaded(x, z) {
            return None;
        }
        let top =
            self.with_level(|b, level, _| b.motion_blocking_top(level, BlockPos::new(x, 0, z)));
        if let Some(rod) = self.find_lightning_rod([x, top.y, z]) {
            return Some(rod);
        }
        let sees_sky = |g: &Self, p: [i32; 3]| g.world.light(p[0], p[1], p[2]) >> 4 == 15;
        let mut candidates: Vec<[i32; 3]> = self
            .remote_entities
            .iter()
            .filter(|m| m.alive())
            .map(|m| {
                [
                    m.position.x.floor() as i32,
                    m.position.y.floor() as i32,
                    m.position.z.floor() as i32,
                ]
            })
            .filter(|p| (p[0] - x).abs() <= 3 && (p[2] - z).abs() <= 3 && p[1] >= top.y - 3)
            .filter(|p| sees_sky(self, *p))
            .collect();
        if let Some(p) = self
            .player
            .as_ref()
            .filter(|p| !p.health.dead_or_dying())
            .map(|p| {
                [
                    p.position.x.floor() as i32,
                    p.position.y.floor() as i32,
                    p.position.z.floor() as i32,
                ]
            })
            && (p[0] - x).abs() <= 3
            && (p[2] - z).abs() <= 3
            && p[1] >= top.y - 3
            && sees_sky(self, p)
        {
            candidates.push(p);
        }
        if !candidates.is_empty() {
            let i = self
                .world
                .random
                .next_int_bound(candidates.len() as i32)
                .unwrap_or(0) as usize;
            return Some(candidates[i]);
        }
        Some([x, top.y, z])
    }
    pub(super) fn spawn_bolt(&mut self, at: [f64; 3], visual_only: bool) {
        if !visual_only {
            let ctx = horizon_client::sculk::Context::default();
            self.post_game_event("minecraft:lightning_strike", at, ctx);
        }
        self.lightning_seen[0] += 1;
        self.ecs_strike(at, visual_only);
    }
    pub(super) fn lightning_strike_allowed(
        &mut self,
        player: Option<u128>,
        pos: [i32; 3],
        cause: &str,
    ) -> bool {
        let mut cancel = false;
        self.native_lightning_strike(player, pos, cause, &mut cancel);
        !cancel
    }
    /// `ClientLevel.addEntity` for a server's lightning bolt: on a client it
    /// only flashes, sounds and draws (`LightningBolt.tick` off the server).
    pub(super) fn spawn_remote_bolt(&mut self, id: u32, at: [f64; 3]) {
        self.bolts.retain(|b| b.id != id);
        // The client's own copy, constructed as on the server.
        let mut random = EntityRandom::own(random_source::entity());
        let seed = random.next_long();
        let flashes = random.next_int_bound(3).unwrap_or(0) + 1;
        self.lightning_seen[0] += 1;
        self.bolts.push(Bolt { id, at, life: 2, flashes, seed, random });
    }
    /// `spawnFire`: fire at the strike point and `extra` random neighbours
    /// (from the bolt's own `random`), wherever `BaseFireBlock.getState` fire
    /// can survive, when a player is within the fire-spread radius. Returns how
    /// many blocks it set on fire.
    pub(in crate::game) fn bolt_fire(&mut self, at: [f64; 3], extra: i32, random: &mut EntityRandom) -> i32 {
        let mut set = 0;
        let base = [
            at[0].floor() as i32,
            at[1].floor() as i32,
            at[2].floor() as i32,
        ];
        // ServerLevel.canSpreadFireAround at the bolt: no fire at all, and no
        // draws for the extra sources, away from every player (#11376).
        let origin = BlockPos::new(base[0], base[1], base[2]);
        if !self.with_level(|_, level, _| {
            horizon_world::level::LevelAccess::can_spread_fire_around(level, origin)
        }) {
            return 0;
        }
        let mut spots = vec![base];
        for _ in 0..extra {
            let mut d = || random.next_int_bound(3).unwrap_or(1) - 1;
            spots.push([base[0] + d(), base[1] + d(), base[2] + d()]);
        }
        for p in spots {
            let state = self.world.block(p[0], p[1], p[2]);
            if !self.res.facts().is_air(state) {
                continue;
            }
            // BaseFireBlock.getState and FireBlock/SoulFireBlock.canSurvive.
            let at = BlockPos::new(p[0], p[1], p[2]);
            let fire = self.with_level(|b, level, ticks| {
                let fire = b.fire_state_for(level, ticks, at);
                b.can_survive(level, ticks, at, fire).then_some(fire)
            });
            if let Some(fire) = fire {
                if !self.native_block_ignite_allowed(at, "lightning") {
                    continue;
                }
                let _ = self.level_set(p, fire);
                set += 1;
            }
        }
        set
    }
    /// `LightningBolt.tick` for the server's bolts this client shows: the flashes and the sounds. Their strikes are P's.
    pub(super) fn tick_bolts(&mut self) {
        let _save_lease = horizon_client::join_seq::visibility::PassLease::enter();
        let mut bolts = std::mem::take(&mut self.bolts);
        bolts.retain_mut(|b| {
            if b.life == 2 {
                // One bolt stands for the server's copy and the client's, both unseeded.
                let pitch = 0.8 + b.random.next_float() * 0.2;
                self.audio.play("minecraft:entity.lightning_bolt.thunder", Some(b.at), 10000.0, pitch);
                let pitch = 0.5 + b.random.next_float() * 0.2;
                self.audio.play("minecraft:entity.lightning_bolt.impact", Some(b.at), 2.0, pitch);
            }
            b.life -= 1;
            if b.life < 0 {
                if b.flashes == 0 {
                    return false;
                }
                if b.life < -b.random.next_int_bound(10).unwrap_or(0) {
                    b.flashes -= 1;
                    b.life = 1;
                    b.seed = b.random.next_long();
                }
            }
            if b.life >= 0 {
                if self.flash == 0 {
                    self.lightning_seen[1] += 1;
                }
                self.flash = 2;
            }
            true
        });
        bolts.append(&mut self.bolts);
        self.bolts = bolts;
    }
    /// `LightningRodBlock.onLightningStrike`: powered for 8 ticks.
    pub(in crate::game) fn strike_rod(&mut self, at: [f64; 3]) {
        let p = [
            at[0].floor() as i32,
            (at[1] - 1.0e-6).floor() as i32,
            at[2].floor() as i32,
        ];
        let state = self.world.block(p[0], p[1], p[2]);
        if !self.res.facts().get(state).name.ends_with("lightning_rod") {
            return;
        }
        let registry = self.pack.assets.registry;
        if let Some(mut s) = registry.state(state).cloned() {
            s.properties.insert("powered".into(), "true".into());
            if let Some(powered) = registry.id(&s) {
                let _ = self.level_set(p, powered);
                self.with_level(|b, level, ticks| {
                    // `level.scheduleTick(pos, this, 8)`: the unpowering. A rod struck
                    // is the same block as before, so its placement does not schedule
                    // it, and without this it stayed powered for good.
                    b.schedule_tick(level, ticks, BlockPos::new(p[0], p[1], p[2]), 8);
                    // `updateNeighbours`: unconditional; the experiments only decide
                    // whether the update carries an orientation.
                    b.lightning_rod_update_neighbors(
                        level,
                        ticks,
                        BlockPos::new(p[0], p[1], p[2]),
                        state,
                    );
                });
            }
            // Level event 3002 with the rod's axis: sparks along it.
            let axis = match s.properties.get("facing").map(String::as_str) {
                Some("east" | "west") => 0,
                Some("north" | "south") => 2,
                _ => 1,
            };
            self.level_event_at(3002, p, axis);
        }
    }
    /// `LightningBolt.clearCopperOnLightningStrike` at the block the bolt struck.
    pub(in crate::game) fn bolt_clear_copper(&mut self, at: [f64; 3]) {
        let p = BlockPos::new(
            at[0].floor() as i32,
            (at[1] - 1.0e-6).floor() as i32,
            at[2].floor() as i32,
        );
        self.with_level(|b, level, ticks| b.lightning_clear_copper(level, ticks, p));
        let _ = self.process_level_events();
    }
    /// `LightningBolt.shouldRenderAtSqrDistance`: a bolt is drawn within 64
    /// blocks times the view scale of the camera, and past that only its sky
    /// flash and thunder remain. Simulation-order render inputs; seeded
    /// geometry lives in the renderer.
    pub(super) fn bolt_instances(
        &self,
        camera: glam::DVec3,
    ) -> Vec<horizon_client::gfx::lightning::Bolt> {
        let range = 64.0 * self.view_scale();
        self.bolts
            .iter()
            .filter(|b| {
                let at = glam::DVec3::new(b.at[0], b.at[1], b.at[2]);
                at.distance_squared(camera) < range * range
            })
            .map(|b| horizon_client::gfx::lightning::Bolt {
                at: b.at,
                seed: b.seed,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_core::geometry::Vec3;
    use horizon_core::random::LegacyRandom;
    use std::sync::Arc;

    /// `LightningBolt.spawnFire` places nothing, and draws no extra sources,
    /// unless `canSpreadFireAround` passes at the bolt (#11376).
    #[test]
    #[ignore = "daily"]
    fn a_bolt_away_from_players_starts_no_fire() {
        use horizon_client::gamerules::Value;
        super::super::native_entity_refs::tests::with_game(|game| {
            game.world
                .chunks
                .insert((0, 0), Arc::new(horizon_client::chunk::Chunk::empty(0, 0)));
            game.block_ticks
                .add_empty_chunk(horizon_core::position::ChunkPos::new(0, 0))
                .unwrap();
            let stone = game
                .pack
                .assets
                .registry
                .default_state("minecraft:stone")
                .unwrap();
            let air = game.res.facts().air;
            game.world.set(8, 64, 8, stone).unwrap();
            let bolt = [8.5, 65.0, 8.5];
            // (radius, player's x): fires expected at the strike.
            for (radius, player_x, fires) in [
                (128, 12.5, true),
                (1, 12.5, false),
                (0, 8.5, false),
                (-1, 1000.0, true),
            ] {
                game.world.set(8, 65, 8, air).unwrap();
                game.ui
                    .rules
                    .set("fire_spread_radius_around_player", Value::Int(radius));
                game.player = Some(
                    horizon_client::motion::PlayerMotion::new(
                        game.motion,
                        Vec3::new(player_x, 65.0, 8.5),
                    )
                    .unwrap(),
                );
                let random = game.world.random.clone();
                let mut bolt_random = EntityRandom::own(LegacyRandom::new(0));
                let set = game.bolt_fire(bolt, 4, &mut bolt_random);
                let burning = game.world.block(8, 65, 8) != air;
                assert_eq!((set > 0, burning), (fires, fires), "radius {radius}");
                if !fires {
                    // Neither the bolt nor level RNG draws when fire is disallowed.
                    assert_eq!(bolt_random.next_long(), LegacyRandom::new(0).next_long());
                    let mut before = random;
                    assert_eq!(game.world.random.next_long(), before.next_long());
                }
            }
        });
    }

    /// `LightningRodBlock.onLightningStrike` updates the neighbours of the block
    /// in front of the rod whether or not redstone experiments are on (#14371):
    /// a wire beside that block is strongly powered by the struck rod.
    #[test]
    #[ignore = "daily"]
    fn a_struck_rod_updates_the_wire_beside_the_block_behind_it() {
        let mut resources = None;
        let mut game = crate::game::game_tests::tests::server_game(&mut resources);
        let registry = game.pack.assets.registry;
        let stone = registry.default_state("minecraft:stone").unwrap();
        let wire = registry.default_state("minecraft:redstone_wire").unwrap();
        let mut rod = registry
            .state(registry.default_state("minecraft:lightning_rod").unwrap())
            .cloned()
            .unwrap();
        rod.properties.insert("facing".into(), "up".into());
        let rod = registry.id(&rod).unwrap();
        // The rod on stone at (8, 64, 8); wire on stone beside that block.
        game.world.set(8, 64, 8, stone).unwrap();
        game.world.set(9, 63, 8, stone).unwrap();
        game.world.set(9, 64, 8, wire).unwrap();
        game.world.set(8, 65, 8, rod).unwrap();
        assert!(
            !game
                .level
                .enabled_features
                .iter()
                .any(|feature| feature == "minecraft:redstone_experiments")
        );
        game.strike_rod([8.5, 66.0, 8.5]);
        let power = registry
            .state(game.world.block(9, 64, 8))
            .unwrap()
            .properties
            .get("power")
            .cloned();
        assert_eq!(power.as_deref(), Some("15"));
    }

    /// `ServerLevel.tickThunder` picks its strike column from `Level.randValue`
    /// (`Ticks::block_random_pos`), using bits `2..=5` for X and `10..=13` for Z
    /// rather than drawing from `Level.random`.
    #[test]
    fn thunder_strike_column_uses_block_random_pos_lcg_bits() {
        let mut ticks = Ticks::new(12345);
        let mut expected_lcg = 12345i32;
        for (cx, cz) in [(0, 0), (-3, 5), (12, -8)] {
            expected_lcg = expected_lcg.wrapping_mul(3).wrapping_add(1_013_904_223);
            let r = expected_lcg >> 2;
            let expected = (cx * 16 + (r & 15), cz * 16 + ((r >> 8) & 15));
            assert_eq!(thunder_strike_column(&mut ticks, cx, cz), expected);
        }
    }
}

// A conversion keeps the old mob's state; lives here for the lightning's.
#[cfg(test)]
#[path = "tests/mob_conversion_state_tests.rs"]
mod mob_conversion_state_tests;
