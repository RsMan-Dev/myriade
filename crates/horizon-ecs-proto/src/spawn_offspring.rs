//! `SPAWN_BY_EGG`: a spawn egg of the entity's own type used on it makes a baby of that type (`Mob.checkAndHandleImportantInteractions`,
//! `SpawnEggItem.spawnOffspringFromSpawnEgg`). The baby is born at the parent's place facing 0/0, takes the egg's custom name, and the egg is
//! used up; a type that cannot be a baby (`setBaby` leaves it an adult) is refused. The egg names the type (`<type>_spawn_egg`); nothing here
//! names a species. The parent may itself be a baby (the jar does not ask for an adult).
use crate::behaviours::breed::BreedData;
use crate::behaviours::breed_love::GROWN;
use crate::behaviours::follow_parent::{adopt, make_baby};
use crate::behaviours::trust::Trust;
use crate::behaviours::variant::Variant;
use crate::cow::*;
use crate::goals::*;
use crate::rules::{no_attrs, Rule};
use crate::targeting::Who;
use bevy_ecs::prelude::*;

/// Not in any species list: `goals::interact` calls it for every creature, as the jar does in `Mob.interact`.
pub static SPAWN_BY_EGG: Rule = Rule { name: "spawn_by_egg", equip: |_, _, _| {}, attrs: no_attrs, interact };

pub(crate) fn interact(w: &mut World, e: Entity, _: &'static (dyn std::any::Any + Sync), i: &crate::behaviours::Interaction) -> bool {
    let Some(item) = crate::items::item_name(i.item) else { return false };
    let Some(kind) = item.strip_suffix("_spawn_egg") else { return false };
    let Some(id) = w.get::<SpeciesId>(e).map(|s| s.0 as usize) else { return false };
    let sp = &w.resource::<SpeciesTable>().0[id];
    // `SpawnEggItem.spawnsEntity`: the egg's type is the entity's; `setBaby(true)` must leave a baby (a form of it exists).
    if sp.name != kind || sp.tags & (tag::AGEING | tag::BABY_FLAG) == 0 {
        return false;
    }
    let flips = matches!(crate::species::lists::struck(sp.name), crate::species::lists::Struck::Flips);
    let Some(at) = w.get::<Pos>(e).map(|p| p.0) else { return false };
    let mut variant = w.get::<Variant>(e).copied();
    // `MushroomCow.getOffspringVariant` with the parent as both parents: a chance in 1024 of the other variant.
    if flips && with_rng(w, e, |r| r.next_int(1024) == 0) {
        variant = variant.map(|v| Variant(1 - v.0.min(1)));
    }
    let age = w.get::<BreedData>(e).map_or(GROWN, |d| d.baby_age);
    let seed = with_rng(w, e, |r| u64::from(r.next_u32()) << 32 | u64::from(r.next_u32()));
    let baby = spawn_like(w, e, at.x, at.z, seed);
    if let Some(v) = variant {
        w.entity_mut(baby).insert(v);
    }
    // `snapTo(x, y, z, 0, 0)`.
    if let Some(mut p) = w.get_mut::<Pos>(baby) {
        p.0.y = at.y;
    }
    w.entity_mut(baby).insert(Yaw(0.0));
    make_baby(w, baby, age);
    crate::attr_mod::apply_load(w, baby);
    // `applyComponentsFromItemStack`: the custom name of the egg (the only one the jar carries over besides `custom_data`).
    if let Some(name) = crate::used_stack::of(w, i.item).and_then(|u| u.stack.custom_name()) {
        crate::custom_name::set(w, baby, Some(name));
    }
    // `Entity.applyImplicitComponents`: the egg's `custom_data` is the baby's `data`.
    if let Some(d) = crate::used_stack::of(w, i.item).and_then(|u| u.stack.custom_data().cloned()) {
        let mut c = horizon_core::nbt::Compound::new();
        c.insert("data".into(), horizon_core::nbt::Tag::Compound(d));
        crate::nbt::load(w, baby, &c);
    }
    // `onOffspringSpawnedFromEgg` of a creature that trusts (`Fox`): the baby trusts the player who used the egg.
    if w.get::<crate::behaviours::defend_trusted::DefendTrusted>(e).is_some() {
        let mut t = Trust::default();
        t.add(Who::Player(i.player));
        w.entity_mut(baby).insert(t);
    }
    adopt(w, e, baby);
    arm_timers(w, baby);
    crate::brain::start(w, baby);
    crate::bridge::host_events_more::said_spawned(w, baby, false, crate::spawn_reason::SpawnReason::SpawnItemUse);
    crate::give_item::take_from_hand(w, i);
    // `Mob.interact`: the result consumes the action, so `ENTITY_INTERACT` follows (the egg has no statistic or advancement of its own here).
    w.resource_mut::<Events>().host.game_event_by(e, "minecraft:entity_interact", [at.x, at.y, at.z], false, false, false);
    true
}
