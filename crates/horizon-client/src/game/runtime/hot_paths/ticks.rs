//! The server level's tick over mobs and over hoppers, at two sizes each: a cost that grows
//! faster than the count shows as a higher cost per entity at the larger size.
use super::{super::Game, Recorder, world::FLOOR};
use horizon_client::Result;
use horizon_core::random::{LegacyRandom, RandomSource};

/// Ticks run before the measured ones, so one-time work (first paths, grown buffers) is done.
const WARMUP: u64 = 20;

impl Game<'_> {
    pub(super) fn hot_ticks(&mut self, recorder: &mut Recorder) -> Result<()> {
        if recorder.wants("server.tick.empty") {
            self.hot_level(1, 2)?;
            self.hot_tick_region(recorder, "server.tick.empty", 200, 200, false)?;
        }
        for (name, count, radius, ticks) in [
            ("server.tick.mobs_128", 128, 2, 100),
            ("server.tick.mobs_2048", 2048, 10, 20),
        ] {
            if recorder.wants(name) {
                self.hot_level(2, radius)?;
                self.hot_mobs(count, radius)?;
                self.hot_tick_region(recorder, name, ticks, ticks * count, true)?;
            }
        }
        for (name, count, radius, ticks) in [
            ("server.tick.hoppers_128", 128, 2, 100),
            ("server.tick.hoppers_2048", 2048, 6, 20),
        ] {
            if recorder.wants(name) {
                self.hot_level(3, radius)?;
                self.hot_hoppers(count, radius)?;
                self.hot_tick_region(recorder, name, ticks, ticks * count, true)?;
            }
        }
        Ok(())
    }

    /// `count` cows, sheep, pigs and chickens at seeded places on the floor, each with its own
    /// seeded random (a summoned mob's is drawn from the system's).
    pub(super) fn hot_mobs(&mut self, count: u64, radius: i32) -> Result<()> {
        let mut places = LegacyRandom::new(20261112);
        let (low, span) = (-radius * 16 + 2, (radius * 2 + 1) * 16 - 4);
        for index in 0..count {
            let kind = ["cow", "sheep", "pig", "chicken"][index as usize % 4];
            let x = low + places.next_int_bound(span)?;
            let z = low + places.next_int_bound(span)?;
            self.hot_command(&format!(
                "summon minecraft:{kind} {x}.5 {} {z}.5",
                FLOOR + 1
            ))?;
        }
        let summoned = self.hot_mob_ids();
        for (seed, id) in (1u64..).zip(&summoned) {
            self.ecs.set_random_seed(*id, seed);
        }
        if summoned.len() as u64 != count {
            return Err(format!("{} of {count} mobs were summoned", summoned.len()).into());
        }
        // The summoned mobs join the level as its next tick opens. A level that opened has
        // bound its entities to their holders; this one does it here.
        self.hot_tick()?;
        self.reset_entity_residency();
        Ok(())
    }

    /// The network ids of the level's ECS entities, each linked when it has none (`ecs_id_of`).
    pub(super) fn hot_mob_ids(&mut self) -> Vec<u32> {
        self.hot_ecs_entities().into_iter().map(|e| self.ecs_id_of(e)).collect()
    }

    /// `count` hoppers in rows of 16, each holding 32 cobblestone and facing the next; a row's
    /// last one faces a chest. Every hopper hands an item on whenever its cooldown ends, so none
    /// of them sleeps.
    pub(super) fn hot_hoppers(&mut self, count: u64, radius: i32) -> Result<()> {
        let low = -radius * 16;
        let columns = ((radius * 2 + 1) * 16 / 18) as u64;
        let y = FLOOR + 1;
        for row in 0..count / 16 {
            let x = low + (row % columns) as i32 * 18;
            let z = low + (row / columns) as i32 * 2;
            self.hot_command(&format!("setblock {} {y} {z} minecraft:chest", x + 16))?;
            for hopper in (0..16).rev() {
                let at = format!("{} {y} {z}", x + hopper);
                self.hot_command(&format!("setblock {at} minecraft:hopper[facing=east]"))?;
                self.hot_command(&format!(
                    "item replace block {at} container.0 with minecraft:cobblestone 32"
                ))?;
            }
        }
        Ok(())
    }

    /// Tick the level `ticks` times as the measured region `name`, after the warm-up ticks.
    fn hot_tick_region(
        &mut self,
        recorder: &mut Recorder,
        name: &str,
        ticks: u64,
        operations: u64,
        changes: bool,
    ) -> Result<()> {
        for _ in 0..WARMUP {
            self.hot_tick()?;
        }
        let before = self.hot_level_state();
        let mut failed = Ok(());
        let mut changed = false;
        recorder.measure(name, operations, || {
            for _ in 0..ticks {
                if let Err(error) = self.hot_tick() {
                    failed = Err(error);
                    break;
                }
            }
            let after = self.hot_level_state();
            changed = after != before;
            after
        })?;
        failed?;
        // Mobs that stand still or hoppers that hand nothing on would measure an idle level.
        if changes && !changed {
            return Err(format!("{name}: the ticks changed nothing").into());
        }
        Ok(())
    }

    fn hot_tick(&mut self) -> Result<()> {
        self.with_residency_tick(|game| game.tick_world(true))
    }

    /// What the ticks left: where every mob stands and what every container holds.
    fn hot_level_state(&self) -> (Vec<[u64; 3]>, Vec<((i32, i32, i32), u32)>) {
        let world = self.ecs.world();
        let mut mobs: Vec<[u64; 3]> = self
            .hot_ecs_entities()
            .into_iter()
            .filter_map(|e| horizon_ecs_proto::bridge::view::position(world?, e))
            .map(|p| p.map(f64::to_bits))
            .collect();
        mobs.sort_unstable();
        let mut held: Vec<_> = self
            .world
            .block_entities
            .iter()
            .map(|(at, entity)| {
                let count = entity
                    .slots()
                    .iter()
                    .flatten()
                    .map(|stack| stack.count)
                    .sum();
                (*at, count)
            })
            .collect();
        held.sort_unstable();
        (mobs, held)
    }
}
