//! Peaceful difficulty through the bridge: `Mob.checkDespawn` discards a creature of a type that is `notInPeaceful` once the difficulty is
//! peaceful. Done by event, not per tick: when the host sets the difficulty (`set_difficulty`) and when such a creature is created
//! (`EcsWorld::spawn_as` refuses it).
use crate::bridge::host::EcsWorld;
use bevy_ecs::prelude::*;

/// A creature of a `Def::not_in_peaceful` species (zero-size, sparse: only these carry it).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct NotInPeaceful;

impl EcsWorld {
    /// Whether monsters may spawn (`ServerLevel.isSpawningMonsters`: the `spawn_mobs` and `spawn_monsters` rules both on); read by the
    /// reinforcements a hurt creature calls. Written when either rule changes.
    pub fn set_monster_spawning(&mut self, on: bool) {
        self.world_mut().insert_resource(crate::reinforcements::MonsterSpawning(on));
    }
    /// Alerts the entities that guard something within reach of `player` (`PiglinAi.angerNearbyPiglins`): the ones at rest turn on it, and
    /// with `only_if_seen` only those with a line of sight. Returns how many were alerted.
    pub fn alert_guards(&mut self, player: u32, only_if_seen: bool) -> usize {
        crate::guard_alert::alert(self.world_mut(), player, only_if_seen)
    }
    /// The host's table of what a melee weapon's enchantments add to a blow (`melee_gear::MeleeGearFn`).
    pub fn set_melee_gear(&mut self, f: crate::melee_gear::MeleeGearFn) {
        self.world_mut().insert_resource(crate::melee_gear::MeleeGear(f));
    }
    /// The level's difficulty (0 peaceful, 1 easy, 2 normal, 3 hard): the creatures and the raids read it. Peaceful removes the creatures
    /// of the types that are not allowed there; the ids that left are returned for the host to forget.
    pub fn set_difficulty(&mut self, id: u8) -> Vec<u32> {
        let w = self.world_mut();
        if w.get_resource::<crate::effects::Difficulty>().is_some_and(|d| d.0 == id) {
            return Vec::new();
        }
        w.insert_resource(crate::effects::Difficulty(id));
        w.get_resource_or_insert_with(crate::raids::RaidState::default).difficulty = i32::from(id);
        w.resource_mut::<crate::hostile::PlayerState>().peaceful = id == 0;
        if id != 0 {
            return Vec::new();
        }
        let gone: Vec<Entity> = w.query_filtered::<Entity, With<NotInPeaceful>>().iter(w).collect();
        let ids = gone.iter().filter_map(|&e| crate::bridge::ids::id_of(w, e)).collect();
        for e in gone {
            crate::despawn::remove(w, e);
        }
        ids
    }
}
