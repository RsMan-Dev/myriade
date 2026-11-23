//! Spawns of the ECS level, as the host makes them (command, mod, spawner, trial, packs at generation): the entity is placed in
//! the level, its network id linked at once, and counted by the index. The natural spawns are the level's own (`spawning::step`).
use crate::game::Game;
use bevy_ecs::entity::Entity;
use horizon_core::nbt::{Compound, JavaString, Tag};
use super::super::creatures::{Finalized, SpawnSite};
use myriade_engine::mobs::spawn_rules::Reason;
use horizon_ecs_proto::spawn_reason::SpawnReason;
use horizon_ecs_proto::spawn_sheet::SpawnSheet;

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
        self.ecs_spawn_mob_at(kind, at, persistent, yaw, site, None)
    }
    /// `ecs_spawn_mob` whose variant draw reads the place of `finalize_at` (None: `at`).
    pub(in crate::game) fn ecs_spawn_mob_at(&mut self, kind: &str, at: [f64; 3], persistent: bool, yaw: Option<f32>, site: SpawnSite, finalize_at: Option<[f64; 3]>) -> Option<u32> {
        self.ecs_spawn_mob_in(kind, at, persistent, yaw, site, None, finalize_at).map(|x| x.0)
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
        finalize_at: Option<[f64; 3]>,
    ) -> Option<(u32, Option<horizon_ecs_proto::spawn_finalize::Group>)> {
        let special = super::super::creatures::local_difficulty_in(&self.world, at).special();
        let mut sheet = SpawnSheet::default();
        let (id, next) = match site {
            SpawnSite::Finalizing(r) => {
                let reason = ecs_reason(r);
                match self.finalize_spawn_at(at).0 {
                    Finalized::Cancelled => return None,
                    Finalized::Skipped => self.ecs_spawn_sheet(kind, at, None, reason, None, &sheet)?,
                    Finalized::Done => self.ecs_spawn_sheet(kind, at, finalize_at, reason, Some((special, group)), &sheet)?,
                }
            }
            SpawnSite::Plain => self.ecs_spawn_sheet(kind, at, None, SpawnReason::MobSummoned, Some((special, group)), &sheet)?,
            SpawnSite::Raw => {
                let e = self.ecs_with(|ecs, mw| ecs.spawn_for(mw, kind, at, None, SpawnReason::Triggered))?;
                (self.ecs_link(e), None)
            }
        };
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
    /// The ECS spawn of `kind` at `at` from `sheet` (its edits made before the finalize), finalized as `reason` with `special` and the pack
    /// `group` when `finalize` is given, then announced; links its network id and gives back the group the finalize made.
    pub(in crate::game) fn ecs_spawn_sheet(
        &mut self,
        kind: &str,
        at: [f64; 3],
        finalize_at: Option<[f64; 3]>,
        reason: SpawnReason,
        finalize: Option<(f32, Option<horizon_ecs_proto::spawn_finalize::Group>)>,
        sheet: &SpawnSheet,
    ) -> Option<(u32, Option<horizon_ecs_proto::spawn_finalize::Group>)> {
        let (e, next) = self.ecs_with(|ecs, mw| {
            let e = ecs.spawn_prepared(mw, kind, at, finalize_at, reason, sheet)?;
            let next = match finalize {
                Some((special, group)) => ecs.finalize_prepared(mw, e, reason, special, group)?.group,
                None => None,
            };
            ecs.announce_prepared(mw, e, reason);
            Some((e, next))
        })?;
        Some((self.ecs_link(e), next))
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
