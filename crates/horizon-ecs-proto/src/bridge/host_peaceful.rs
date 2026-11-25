//! Peaceful difficulty through the bridge: `Mob.checkDespawn` discards a creature of a type that is `notInPeaceful` once the difficulty is
//! peaceful. Done by event, not per tick: when the host sets the difficulty (`set_difficulty`) and when such a creature is created
//! (`EcsWorld::spawn_as` refuses it).
use crate::bridge::host::EcsWorld;
use bevy_ecs::prelude::*;

/// The natural spawning's inputs from the host, made with their defaults the first time.
fn spawn_env(w: &mut World) -> Mut<'_, crate::spawn_env::SpawnEnv> {
    w.get_resource_or_insert_with(crate::spawn_env::SpawnEnv::default)
}

/// A creature of a `Def::not_in_peaceful` species (zero-size, sparse: only these carry it).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct NotInPeaceful;

impl EcsWorld {
    /// `SPAWN_MOBS` and `SPAWN_MONSTERS` (`ServerLevel.isSpawningMonsters`): read by the natural spawning and the reinforcements a hurt
    /// creature calls. Written when one changes.
    pub fn set_spawn_rules(&mut self, spawn_mobs: bool, monsters: bool) {
        let mut env = spawn_env(self.world_mut());
        if env.spawn_mobs != spawn_mobs || env.monsters != monsters {
            env.spawn_mobs = spawn_mobs;
            env.monsters = monsters;
        }
    }
    /// A chunk enters (`on`) or leaves the chunks that may take a creature: ticking and not held back by a ticket. Written when it changes.
    pub fn set_chunk_spawnable(&mut self, cx: i32, cz: i32, on: bool) {
        let mut env = spawn_env(self.world_mut());
        let moved = if on { env.spawnable.insert((cx, cz)) } else { env.spawnable.remove(&(cx, cz)) };
        env.changed |= moved;
    }
    /// The simulation distance changed: the chunks that tick are counted again.
    pub fn set_simulation_radius(&mut self, radius: i32) {
        crate::zone::resize(self.world_mut(), radius);
    }
    /// A ticket other than a player's (forced, portal, viewed) keeps chunk `(cx, cz)` entity-ticking (`on`) or stops: the creatures and
    /// objects of the chunk wake, or sleep unless a player covers it. By event, with the ticket changes.
    pub fn set_chunk_pinned(&mut self, cx: i32, cz: i32, on: bool) {
        let moved = self.world_mut().resource_scope(|_, mut idx: Mut<crate::cow::Index>| {
            let ix = &mut *idx;
            ix.zone.pin(&mut ix.chunks, cx, cz, on)
        });
        if moved {
            self.world_mut().resource_mut::<crate::cow::Players>().zone_dirty = true;
        }
    }
    /// The world border `[min_x, min_z, max_x, max_z]`, if any. Written when it changes.
    pub fn set_world_border(&mut self, border: Option<[f64; 4]>) {
        let mut env = spawn_env(self.world_mut());
        if env.border != border {
            env.border = border;
            env.changed = true;
        }
    }
    /// The world spawn, the origin of the 24 blocks a creature may not be born in. Written when it changes.
    pub fn set_world_spawn(&mut self, at: [i32; 3]) {
        let mut env = spawn_env(self.world_mut());
        if env.world_spawn != at {
            env.world_spawn = at;
        }
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
        let gone: Vec<Entity> = crate::queries::entities::<With<NotInPeaceful>>(w);
        let ids = gone.iter().filter_map(|&e| crate::bridge::ids::id_of(w, e)).collect();
        for e in gone {
            crate::despawn::remove(w, e);
        }
        ids
    }
}
