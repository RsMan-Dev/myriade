//! Raid handles for native server mods (`Raid`, `Raids`). Horizon keeps the
//! raids of current and parked server levels. Earlier opens, closed levels
//! and client reads return `None` or `false`.
use super::{Game, commands::Target};
use horizon_client::{
    lan::playerdata::ints_uuid,
    raid::{Raid, Raids, Status},
};
use mod_api::{EntityRef, LevelRef, MobRef, RaidRef, RaidStatus};

impl Game<'_> {
    // Diverges from upstream: handles resolve the requested server level without
    // swapping it into place, including during hooks for another level.
    fn native_level_raid_collections(&self, level: LevelRef) -> Option<&Raids> {
        self.native_level_world(level)?;
        if level.dimension() == self.dim.key() {
            self.raids_in_place()
        } else {
            let parked = self.levels.parked_of(level.dimension_id())?;
            parked.raiding.raids(level.dimension_id())
        }
    }
    fn native_level_raids_mut(&mut self, level: LevelRef) -> Option<&mut Raids> {
        self.native_level_world(level)?;
        if level.dimension() == self.dim.key() {
            self.raiding.raids_mut(level.dimension_id())
        } else {
            self.levels
                .parked_mut_of(level.dimension_id())?
                .raiding
                .raids_mut(level.dimension_id())
        }
    }
    /// The living raiders of the raid in the level in place: id, wave, health (the ECS holds them; a parked level has none).
    fn native_raid_members(&self, raid: RaidRef) -> Option<std::vec::IntoIter<(u32, i32, f32)>> {
        let raids = self.native_level_raid_collections(raid.level())?;
        raids.get(raid.id())?;
        let here = raid.level().dimension() == self.dim.key();
        Some(if here { self.ecs.raid_members(raid.id()) } else { Vec::new() }.into_iter())
    }
    /// The raid a handle names now, or None for a stale or foreign one.
    fn native_raid_data(&self, raid: RaidRef) -> Option<&Raid> {
        self.native_level_raid_collections(raid.level())?
            .get(raid.id())
    }

    /// `Raids.get(id)`: the raid saved under `id` in the requested level, if
    /// `Raids` still holds it (a stopped raid goes at its next tick).
    pub(super) fn level_raid(&self, level: LevelRef, id: i32) -> Option<RaidRef> {
        let raid = RaidRef::new(level, id);
        self.native_raid_data(raid).map(|_| raid)
    }

    /// `ServerLevel.getRaidAt(pos)`: the closest active raid whose centre is
    /// less than 96 blocks away (`Raids.getNearbyRaid`, 9216 squared).
    pub(super) fn level_raid_at(&self, level: LevelRef, pos: [i32; 3]) -> Option<RaidRef> {
        let (id, _) = self.native_level_raid_collections(level)?.nearby(pos)?;
        Some(RaidRef::new(level, id))
    }

    /// Every raid `Raids` holds, in its iteration order, stopped ones
    /// included until their next tick. None for a closed or stale level.
    pub(super) fn level_raids(&self, level: LevelRef) -> Option<Vec<RaidRef>> {
        Some(
            self.native_level_raid_collections(level)?
                .ids()
                .into_iter()
                .map(|id| RaidRef::new(level, id))
                .collect(),
        )
    }

    /// `Raider.getCurrentRaid`: the raid a raider belongs to, even a stopped
    /// one. None for a mob that is no raider or in no raid.
    pub(super) fn mob_raid(&self, mob: MobRef) -> Option<RaidRef> {
        let Target::Mob(id) = self.native_entity_target(mob.entity())? else {
            return None;
        };
        let id = self.ecs.raider_raid(id)??;
        let level = self.entity_level(mob.entity())?;
        self.level_raid(level, id)
    }

    /// `Raid.getCenter`.
    pub(super) fn raid_center(&self, raid: RaidRef) -> Option<[i32; 3]> {
        Some(self.native_raid_data(raid)?.center)
    }

    /// `Raid.getRaidOmenLevel`: 0 to 5 in play, any value a mod set.
    pub(super) fn raid_omen_level(&self, raid: RaidRef) -> Option<i32> {
        Some(self.native_raid_data(raid)?.omen_level)
    }

    /// `Raid.setRaidOmenLevel`: any value, as vanilla assigns it (only
    /// `absorbRaidOmen` clamps, to 0 to 5). The raid is saved with it.
    pub(super) fn raid_set_omen_level(&mut self, raid: RaidRef, level: i32) -> bool {
        self.native_level_raids_mut(raid.level())
            .and_then(|raids| raids.set_omen_level(raid.id(), level))
            .is_some()
    }

    /// `Raid.getGroupsSpawned`: the waves spawned so far.
    pub(super) fn raid_groups_spawned(&self, raid: RaidRef) -> Option<i32> {
        Some(self.native_raid_data(raid)?.groups_spawned)
    }

    /// `Raid.numGroups`, set from `getNumGroups(difficulty)` when the raid
    /// began: 0, 3, 5 or 7. A raid of omen level 2 or more adds a bonus wave
    /// (`hasBonusWave`).
    pub(super) fn raid_num_groups(&self, raid: RaidRef) -> Option<i32> {
        Some(self.native_raid_data(raid)?.num_groups)
    }

    /// Whether the omen level is above 1, which adds a wave after the last
    /// (`Raid.hasBonusWave`).
    pub(super) fn raid_has_bonus_wave(&self, raid: RaidRef) -> Option<bool> {
        Some(self.native_raid_data(raid)?.omen_level > 1)
    }

    /// `Raid.isStarted`.
    pub(super) fn raid_is_started(&self, raid: RaidRef) -> bool {
        self.native_raid_data(raid).is_some_and(|r| r.started)
    }

    /// `Raid.isActive`: false once stopped.
    pub(super) fn raid_is_active(&self, raid: RaidRef) -> bool {
        self.native_raid_data(raid).is_some_and(|r| r.active)
    }

    /// `Raid.RaidStatus`.
    pub(super) fn raid_status(&self, raid: RaidRef) -> Option<RaidStatus> {
        Some(match self.native_raid_data(raid)?.status {
            Status::Ongoing => RaidStatus::Ongoing,
            Status::Victory => RaidStatus::Victory,
            Status::Loss => RaidStatus::Loss,
            Status::Stopped => RaidStatus::Stopped,
        })
    }

    /// `Raid.getTotalHealth`: the health of every raider that joined, which
    /// the boss bar divides by.
    pub(super) fn raid_total_health(&self, raid: RaidRef) -> Option<f32> {
        Some(self.native_raid_data(raid)?.total_health)
    }

    /// `Raid.getHealthOfLivingRaiders`.
    pub(super) fn raid_living_health(&self, raid: RaidRef) -> Option<f32> {
        Some(
            self.native_raid_members(raid)?
                .fold(0.0, |a, m| a + m.2),
        )
    }

    /// The boss bar's progress, 0 to 1 (`Raid.raidEvent.getProgress`).
    pub(super) fn raid_bar_progress(&self, raid: RaidRef) -> Option<f32> {
        Some(self.native_raid_data(raid)?.bar.progress)
    }

    /// `Raid.getAllRaiders`: the living raiders that joined, in mob-list
    /// order, as entity handles.
    pub(super) fn raid_raiders(&self, raid: RaidRef) -> Option<Vec<EntityRef>> {
        Some(
            self.native_raid_members(raid)?
                .map(|m| EntityRef::new(raid.level().epoch(), m.0))
                .collect(),
        )
    }

    /// The living raiders of one wave (`groupRaiderMap.get(wave)`).
    pub(super) fn raid_wave_raiders(&self, raid: RaidRef, wave: i32) -> Option<Vec<EntityRef>> {
        Some(
            self.native_raid_members(raid)?
                .filter(|m| m.1 == wave)
                .map(|m| EntityRef::new(raid.level().epoch(), m.0))
                .collect(),
        )
    }

    /// `Raid.heroesOfTheVillage`: the UUIDs of the players who got Hero of
    /// the Village, online or not, in the order vanilla's set iterates them.
    pub(super) fn raid_heroes(&self, raid: RaidRef) -> Option<Vec<u128>> {
        Some(
            self.native_raid_data(raid)?
                .heroes()
                .into_iter()
                .map(ints_uuid)
                .collect(),
        )
    }

    /// `Raid.stop`: the raid ends as `stopped` and its boss bar leaves the
    /// players; the `raid_state_change` hook fires. False if the raid is
    /// already stopped.
    pub(super) fn raid_stop(&mut self, raid: RaidRef) -> bool {
        if self.native_raid_data(raid).is_none_or(|r| r.is_stopped()) {
            return false;
        }
        let Some(center) = self
            .native_level_raids_mut(raid.level())
            .and_then(|raids| raids.stop(raid.id()))
        else {
            return false;
        };
        self.native_raid_state_change(raid, center, "stopped");
        true
    }
}

#[cfg(test)]
mod tests {
    use super::super::native_entity_refs::tests::with_game;
    use super::*;
    use mod_api::{Hooks, RaidStateChangeEvent, ServerHost, WorldEpoch};
    use std::sync::{Arc, Mutex};

    type RaidObservation = (RaidRef, String, Option<RaidStatus>, bool);
    struct RaidObserver(Arc<Mutex<Vec<RaidObservation>>>);

    #[mod_api::__subscriptions]
    impl Hooks for RaidObserver {
        fn raid_state_change(
            &self,
            host: &mut dyn ServerHost,
            event: &mut RaidStateChangeEvent<'_>,
        ) {
            let status = event.raid.status(host);
            let wrote = event.state == "stopped" && event.raid.set_omen_level(host, 17);
            self.0
                .lock()
                .unwrap()
                .push((event.raid, event.state.to_owned(), status, wrote));
        }
    }

    #[test]
    #[ignore = "daily"]
    fn raid_events_keep_the_owning_level_with_colliding_ids() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let mods = vec![mod_api::ModEntry::new(
            "raid-observer",
            Box::new(RaidObserver(seen.clone())),
        )]
        .into();
        mod_platform::testing::with_mods(mods, || {
            with_game(|game| {
                let level = game.level(game.dim.id()).unwrap();
                assert!(game.create_or_extend_raid([10, 70, 10], Some(2)).is_some());
                let raid = level.raid_at(game, [10, 70, 10]).unwrap();
                assert!(
                    game.enter_command_level(super::super::Dim::Nether)
                        .is_some()
                );
                game.read_raids();
                let current = game.level(game.dim.id()).unwrap();
                let (id, _) = game
                    .raiding
                    .raids_mut(current.dimension_id())
                    .unwrap()
                    .create_or_extend([100, 70, 100], 2, None, 7);
                assert_eq!(id, raid.id());
                let other = current.raid(game, id).unwrap();
                let before = other.omen_level(game);
                assert!(raid.stop(game));
                assert_eq!(raid.omen_level(game), Some(17));
                assert_eq!(other.omen_level(game), before);
                assert!(other.is_active(game));
                game.raid_command_stop(id);
                let tick_id = game
                    .raiding
                    .raids_mut(current.dimension_id())
                    .unwrap()
                    .create_or_extend([200, 70, 200], 2, None, 7)
                    .0;
                let tick_raid = current.raid(game, tick_id).unwrap();
                game.world.health_rules.difficulty = horizon_world::food::Difficulty::Peaceful;
                game.tick_raids();
                let records = seen.lock().unwrap();
                assert_eq!(records.len(), 4);
                assert_eq!(
                    records[0],
                    (raid, "triggered".into(), Some(RaidStatus::Ongoing), false)
                );
                assert_eq!(
                    records[1],
                    (raid, "stopped".into(), Some(RaidStatus::Stopped), true)
                );
                assert_eq!(
                    records[2],
                    (other, "stopped".into(), Some(RaidStatus::Stopped), true)
                );
                assert_eq!(
                    records[3],
                    (tick_raid, "stopped".into(), Some(RaidStatus::Stopped), true)
                );
            })
        });
    }

    #[test]
    #[ignore = "daily"]
    fn saved_raids_are_found_before_the_first_raid_tick() {
        with_game(|game| {
            let root = std::env::temp_dir().join(format!("raid-early-{}", std::process::id()));
            game.world_root = root.clone();
            assert!(game.raid_command_create([10, 70, 10], 2));
            game.save_raids();
            // The world opens again and reads the raids, as `finish_open_world` does.
            game.raiding = Default::default();
            game.read_raids();
            use horizon_core::random::RandomSource;
            let before = game.world.random.clone().next_long();
            let level = game.level(game.dim.id()).unwrap();
            let raid = game.level_raid_at(level, [10, 70, 10]).unwrap();
            assert_eq!(game.raid_center(raid), Some([10, 70, 10]));
            assert_eq!(game.raid_omen_level(raid), Some(2));
            // Reading drew nothing from the world random; the raid tick seeds.
            assert_eq!(game.world.random.clone().next_long(), before);
            let _ = std::fs::remove_dir_all(root);
        });
    }

    #[test]
    #[ignore = "daily"]
    fn stale_closed_and_client_raid_handles_read_nothing() {
        with_game(|game| {
            let level = game.level(game.dim.id()).unwrap();
            assert!(game.raid_command_create([10, 70, 10], 1));
            let raid = game.level_raid_at(level, [10, 70, 10]).unwrap();
            let later = LevelRef::new(WorldEpoch::new(level.epoch().get() + 1), level.dimension());
            let stale = RaidRef::new(later, raid.id());
            assert_eq!(game.raid_center(stale), None);
            assert!(!game.raid_is_active(stale));
            assert!(!game.raid_stop(stale));
            assert!(!game.raid_set_omen_level(stale, 2));
            assert_eq!(game.level_raids(later), None);
            assert_eq!(game.level_raid_at(later, [10, 70, 10]), None);
            let parked = LevelRef::new(level.epoch(), mod_api::DimensionKey::NETHER);
            assert_eq!(game.raid_status(RaidRef::new(parked, raid.id())), None);
            assert_eq!(game.level_raid(level, raid.id() + 7), None);
            assert_eq!(game.raid_status(raid), Some(RaidStatus::Ongoing));
        });
    }
}
