//! The host's answers that the world keeps until the host asks: a lethal hit held by `DeathPolicy` or by a totem's question, and the network ids
//! removed since the last call (`Removed`, filled by the hook of `NetId`). Both are read by the host's own call, never by a scan.
use crate::bridge::host::{EcsWorld, HurtSource};
use crate::bridge::ids::Removed;
use crate::cow::{DeathPolicy, Dying, Health, enter_index};
use crate::goals::Events;
use bevy_ecs::{entity::Entity, prelude::*};
use myriade_engine_lib::mobs::MobWorld;

impl EcsWorld {
    /// `DeathPolicy.vetoable`: a lethal hit leaves the entity as `Dying` until `resolve_death`, instead of removing it.
    pub fn set_death_vetoable(&mut self, on: bool) {
        self.world_mut().insert_resource(DeathPolicy { vetoable: on });
    }

    /// While `on`, a lethal hit on a holder of a totem waits for the host (`TotemAsk`, answered by `resolve_totem`).
    pub fn set_totem_watched(&mut self, on: bool) {
        self.world_mut().insert_resource(crate::using::TotemWatched(on));
    }

    /// While `on`, a hit of an attacker is said (`HitDealt`): the host's `hurt_enemy` hook hears it.
    pub fn set_hit_watched(&mut self, on: bool) {
        self.world_mut().insert_resource(crate::cow::HitWatched(on));
    }

    /// The answer to a `TotemAsk`: `cancel` leaves the totem unused and the entity dies. True when the entity was saved.
    pub fn resolve_totem(&mut self, mw: &MobWorld<'_>, e: Entity, cancel: bool) -> bool {
        self.lent(mw, |w| crate::using::resolve_totem(w, e, cancel))
    }

    /// While `on`, a conversion waits for the host (`conversion::convert_or_ask`): its mods can refuse it and see both entities.
    pub fn set_conversion_watched(&mut self, on: bool) {
        self.world_mut().insert_resource(crate::conversion::ConversionWatched(on));
    }

    /// While `on`, a new target waits for the host (`start_attacking::TargetAsk`) before the brain takes it: set when a mod hears `living_change_target`.
    pub fn set_target_watched(&mut self, on: bool) {
        self.world_mut().insert_resource(crate::brain::behaviours::start_attacking::TargetWatched(on));
    }

    /// While `on`, a walking entity emits `step`, `swim`, `hit_ground` and `splash` (`move_events`): set when a vibration listener may hear.
    pub fn set_move_events(&mut self, on: bool) {
        self.world_mut().insert_resource(crate::move_events::MoveEventsOn(on));
    }

    /// The host's table of what a stack on the ground resists (`item::ItemResists`), set once at the level's creation.
    pub fn set_item_resists_table(&mut self, f: crate::item::ItemResistsFn) {
        self.world_mut().insert_resource(crate::item::ItemResists(f));
    }

    /// The host's table of the attribute modifiers an item grants in a slot, set once at the level's creation.
    pub fn set_item_modifiers_table(&mut self, f: crate::equip_attrs::ItemModifiersFn) {
        self.world_mut().insert_resource(crate::equip_attrs::ItemModifiers(f));
    }

    /// The answer to a `ConversionAsk`: the new entity, born while the old one stays (None when `cancel`).
    pub fn resolve_conversion(&mut self, e: Entity, cancel: bool) -> Option<Entity> {
        crate::conversion::resolve(self.world_mut(), e, cancel)
    }

    /// The answer to a `TargetAsk`: the brain takes the target it names, or none (`TargetAnswer::Keep`).
    pub fn resolve_target(&mut self, e: Entity, answer: crate::brain::behaviours::start_attacking::TargetAnswer) {
        crate::brain::behaviours::start_attacking::resolve_target(self.world_mut(), e, answer);
    }

    /// The old entity of a resolved conversion leaves.
    pub fn finish_conversion(&mut self, e: Entity) {
        crate::conversion::finish(self.world_mut(), e);
    }

    /// The network ids removed since the last call, in order; empty at rest.
    pub fn drain_removed(&mut self) -> Vec<u32> {
        self.world_mut().get_resource_mut::<Removed>().map_or_else(Vec::new, |mut r| std::mem::take(&mut r.0))
    }

    /// The end of a lethal hit held by the policy (`resolve_death`).
    pub fn resolve_death(&mut self, mw: &MobWorld<'_>, e: Entity, veto: Option<f32>) {
        self.lent(mw, |w| resolve_death(w, e, veto));
    }

    /// The first half of a death that goes on: the loot leaves, the entity stays (no longer `Dying`) for the host's hook, which then
    /// removes it with `despawn`. False when `e` is not `Dying`.
    pub fn loot_dying(&mut self, mw: &MobWorld<'_>, e: Entity) -> bool {
        self.lent(mw, |w| {
            if w.get::<Dying>(e).is_none() {
                return false;
            }
            w.entity_mut(e).remove::<Dying>();
            crate::loot::on_death(w, e);
            crate::item::spill_destroyed(w, e);
            crate::spawn_trial::forget(w, e);
            true
        })
    }
}

/// The end of a lethal hit held by `DeathPolicy`: `None` the death goes on (loot, despawn); `Some(v)` the entity lives on with health `v`.
/// Nothing happens to an entity that is not `Dying`.
pub fn resolve_death(w: &mut World, e: Entity, veto: Option<f32>) {
    if w.get::<Dying>(e).is_none() {
        return;
    }
    w.entity_mut(e).remove::<Dying>();
    match veto {
        Some(v) => {
            if let Some(mut h) = w.get_mut::<Health>(e) {
                h.0 = v;
            }
            crate::bridge::view::mark(w, e);
            enter_index(w, e);
        }
        None => {
            crate::loot::on_death(w, e);
            crate::item::spill_destroyed(w, e);
            crate::spawn_trial::forget(w, e);
            if crate::remove_killed::silent(w, e) {
                if let Ok(m) = w.get_entity_mut(e) {
                    m.despawn();
                }
            } else {
                crate::tear_down::linger(w, e);
            }
        }
    }
}

/// `Spawned`: the creature `e` has just appeared (the host's spawn calls it once the entity is armed and indexed).
pub fn said_spawned(w: &mut World, e: Entity, natural: bool, reason: crate::spawn_reason::SpawnReason) {
    // A creature past its despawn distance may be gone before the host reports it.
    let Ok(mut m) = w.get_entity_mut(e) else { return };
    m.insert(crate::spawn_reason::Spawn(reason));
    w.resource_mut::<Events>().host.spawned(e, natural, reason);
}

impl HurtSource {
    /// The vanilla damage type's name of the hurt (`DamageTypes`), as the host names its source.
    pub fn damage_type(&self) -> &'static str {
        match self {
            HurtSource::Generic => "generic",
            HurtSource::Typed { damage, .. } => *damage,
            HurtSource::Player(_) => "player_attack",
            HurtSource::Entity(_) => "mob_attack",
            HurtSource::Fall => "fall",
            HurtSource::Fire => "in_fire",
            HurtSource::Explosion => "explosion",
            HurtSource::IndirectMagic { .. } => "indirect_magic",
            HurtSource::Void => "out_of_world",
        }
    }
}
