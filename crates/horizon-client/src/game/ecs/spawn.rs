//! Spawns of the ECS level, as the host makes them (command, mod, spawner, trial, packs at generation): the entity is placed in
//! the level, its network id linked at once, and counted by the index. The natural spawns are the level's own (`spawning::step`).
use crate::game::Game;
use bevy_ecs::entity::Entity;
use horizon_core::nbt::{Compound, JavaString, Tag};
use super::super::creatures::SpawnSite;
use horizon_client::mobs::spawn_rules::Reason;
use horizon_ecs_proto::spawn_reason::SpawnReason;

impl Game<'_> {
    /// Spawns `kind` at `at` in the ECS level and links its network id; None when the level is inert or refuses the kind.
    pub(in crate::game) fn ecs_spawn(&mut self, kind: &str, at: [f64; 3], nbt: Option<&Compound>) -> Option<u32> {
        let e = self.ecs_with(|ecs, mw| ecs.spawn(mw, kind, at, nbt))?;
        Some(self.ecs_link(e))
    }
    /// `spawn_mob` on the ECS: the mob is created by P (its own finalize), kept when `persistent`, and turned to `yaw` when one is given.
    /// The site's reason (`EntitySpawnReason`) goes to P with the spawn, and P finalizes the mob for it. `Plain` (the old host's block or item
    /// mob: finalized "as a summoned one", no mod told) takes `MobSummoned`; `Raw` (created without `finalizeSpawn`: a golem built, an egg
    /// hatched, lightning's horse) takes `Triggered` and is not finalized.
    pub(in crate::game) fn ecs_spawn_mob(&mut self, kind: &str, at: [f64; 3], persistent: bool, yaw: Option<f32>, site: SpawnSite) -> Option<u32> {
        self.ecs_spawn_mob_in(kind, at, persistent, yaw, site, None).map(|x| x.0)
    }
    /// `ecs_spawn_mob` for a pack: the group of the previous member goes in (`SpawnGroupData`: its size, and the variant the first
    /// drew) and the next one comes back, None for a site that does not finalize.
    pub(in crate::game) fn ecs_spawn_mob_in(
        &mut self,
        kind: &str,
        at: [f64; 3],
        persistent: bool,
        yaw: Option<f32>,
        site: SpawnSite,
        group: Option<horizon_ecs_proto::spawn_finalize::Group>,
    ) -> Option<(u32, Option<horizon_ecs_proto::spawn_finalize::Group>)> {
        let special = super::super::creatures::local_difficulty_in(&self.world, at).special();
        let (e, next) = match site {
            SpawnSite::Finalizing(r) => {
                let reason = ecs_reason(r);
                let (e, out) = self.ecs_with(|ecs, mw| ecs.spawn_finalized(mw, kind, at, reason, special, group))?;
                (e, out.group)
            }
            SpawnSite::Plain => {
                let (e, out) = self.ecs_with(|ecs, mw| ecs.spawn_finalized(mw, kind, at, SpawnReason::MobSummoned, special, group))?;
                (e, out.group)
            }
            SpawnSite::Raw => (self.ecs_with(|ecs, mw| ecs.spawn_for(mw, kind, at, None, SpawnReason::Triggered))?, None),
        };
        let id = self.ecs_link(e);
        if let Some(e) = self.ecs.entity_of(id) {
            if persistent {
                self.ecs.set_persistent(e);
            }
            if let Some(yaw) = yaw {
                self.ecs.set_rotation(e, yaw, 0.0);
            }
        }
        Some((id, next))
    }
    /// `PatrolSpawner.spawnPatrolMember` on the ECS: a pillager of the patrol, a leader with its target from the origin and its banner.
    pub(in crate::game) fn ecs_spawn_patrol_member(&mut self, kind: &str, at: [f64; 3], leader: bool, reason: Reason) -> Option<u32> {
        let special = super::super::creatures::local_difficulty_in(&self.world, at).special();
        let why = ecs_reason(reason);
        let e = self.ecs_with(|ecs, mw| ecs.spawn_patrol_member(mw, kind, at, leader, why, special))?;
        Some(self.ecs_link(e))
    }
    /// Loads an entity from its saved data into the ECS level and links its network id.
    pub(in crate::game) fn ecs_load(&mut self, c: &Compound) -> Option<u32> {
        let e = self.ecs_with(|ecs, mw| ecs.load_entity(mw, c))?;
        let id = self.ecs_link(e);
        self.restore_portal_cooldown_of(id, c.get(&JavaString::from("PortalCooldown")));
        Some(id)
    }
    /// `ecs_load` of a mount and its riders (`Passengers`): the pairs (id in `old`, new id) of the entities made, `old` being the ids
    /// the compounds were saved from in their NBT order (`EcsLevel::ride_tree`). The saved `PortalCooldown` of each goes back to the host.
    pub(in crate::game) fn ecs_load_tree(&mut self, c: &Compound, old: &[u32]) -> Vec<(u32, u32)> {
        let made = self.ecs_with(|ecs, mw| ecs.load_entity_tree(mw, c));
        let mut out = Vec::new();
        self.pair_loaded(c, true, &mut old.iter().copied(), &mut made.into_iter(), &mut out);
        out
    }
    /// Walks `c` and its `Passengers` in NBT order beside `olds` and the entities the load made (a compound under one that made
    /// nothing was not loaded, and has no entity).
    fn pair_loaded(
        &mut self,
        c: &Compound,
        parent_made: bool,
        olds: &mut dyn Iterator<Item = u32>,
        made: &mut dyn Iterator<Item = Option<Entity>>,
        out: &mut Vec<(u32, u32)>,
    ) {
        let old = olds.next();
        let e = if parent_made { made.next().flatten() } else { None };
        if let Some(e) = e {
            let id = self.ecs_link(e);
            self.restore_portal_cooldown_of(id, c.get(&JavaString::from("PortalCooldown")));
            if let Some(old) = old {
                out.push((old, id));
            }
        }
        if let Some(Tag::List(riders)) = c.get(&JavaString::from("Passengers")) {
            for t in riders {
                if let Tag::Compound(p) = t {
                    self.pair_loaded(p, e.is_some(), olds, made, out);
                }
            }
        }
    }
    /// Test fixture: loads an entity of `kind` at `pos` under network id `id` (the level is opened when there is none); its id.
    #[cfg(test)]
    pub(in crate::game) fn load_other_as(&mut self, id: u32, kind: &str, pos: [f64; 3]) -> u32 {
        use crate::ecs_bridge::EcsLevel;
        use horizon_ecs_proto::bridge::host::HostSetup;
        if !self.ecs.is_active() {
            let level = {
                self.test_ecs_level()
            };
            self.ecs = level;
        }
        let mut c = Compound::new();
        c.insert("id".into(), Tag::String(kind.into()));
        c.insert("Pos".into(), Tag::List(pos.map(|v| Tag::Double(v.to_bits())).to_vec()));
        self.next_entity = id - 1;
        self.ecs_load(&c).expect("the ECS level loads the entity")
    }
    /// The network id of `e`, linked from the game's counter when it has none.
    pub(in crate::game) fn ecs_link(&mut self, e: Entity) -> u32 {
        let next = &mut self.next_entity;
        self.ecs.id_for(e, &mut || {
            *next += 1;
            *next
        })
    }
}

/// P's `SpawnReason` of the mod API's (`EntitySpawnReason`); a reason the API adds later spawns as a command until it is mapped.
fn ecs_reason(r: Reason) -> SpawnReason {
    match r {
        Reason::Natural => SpawnReason::Natural,
        Reason::ChunkGeneration => SpawnReason::ChunkGeneration,
        Reason::Spawner => SpawnReason::Spawner,
        Reason::TrialSpawner => SpawnReason::TrialSpawner,
        Reason::Reinforcement => SpawnReason::Reinforcement,
        Reason::Patrol => SpawnReason::Patrol,
        Reason::Structure => SpawnReason::Structure,
        Reason::Event => SpawnReason::Event,
        Reason::Command => SpawnReason::Command,
        Reason::SpawnItemUse => SpawnReason::SpawnItemUse,
        Reason::MobSummoned => SpawnReason::MobSummoned,
        Reason::Conversion => SpawnReason::Conversion,
        Reason::Breeding => SpawnReason::Breeding,
        Reason::Triggered => SpawnReason::Triggered,
        Reason::Jockey => SpawnReason::Jockey,
        Reason::Dispenser => SpawnReason::Dispenser,
        Reason::Bucket => SpawnReason::Bucket,
        Reason::Load => SpawnReason::Load,
        Reason::DimensionTravel => SpawnReason::DimensionTravel,
        _ => SpawnReason::Command,
    }
}

#[cfg(test)]
mod tests {
    use crate::ecs_bridge::EcsLevel;
    use crate::game::native_entity_refs::tests::with_game;
    use horizon_ecs_proto::bridge::host::HostSetup;

    #[test]
    fn a_command_spawn_is_an_id_the_bridge_reads_and_the_count_agrees() {
        with_game(|game| {
            let level = {
                game.test_ecs_level()
            };
            game.ecs = level;
            let at = [0.5, 64.0, 0.5];
            let first = game.ecs_spawn("minecraft:cow", at, None).unwrap();
            let second = game.ecs_spawn("minecraft:cow", [1.5, 64.0, 0.5], None).unwrap();
            assert!(game.ecs.exists(first) && game.ecs.exists(second));
            assert_eq!(game.ecs.kind(first), Some("minecraft:cow"));
            assert_eq!(game.ecs_count("minecraft:cow", at, 8.0), 2);
            assert_eq!(game.ecs_count("minecraft:cow", [100.0, 64.0, 100.0], 8.0), 0);
        });
    }
}
