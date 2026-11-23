//! What the weapon a creature holds in its main hand adds to its melee blow (`Mob.doHurtTarget`: `EnchantmentHelper.modifyDamage`,
//! `getKnockback` through `modifyKnockback`, `doPostAttackEffects` for Fire Aspect). The equipment holds the whole `ItemStack`, so the levels
//! are there; the arithmetic is the host's one table (it shares the level's enchantment and item tables), read only when a blow is struck.
use crate::cow::{SpeciesId, SpeciesTable};
use crate::targeting::Who;
use bevy_ecs::prelude::*;
use myriade_engine_lib::items::ItemStack;

/// The numbers of a blow: the damage, the knockback (the attribute plus the enchantments; the extra push is half of it) and the seconds of
/// fire the victim is set alight for.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Blow {
    pub damage: f32,
    pub knockback: f32,
    pub ignite_seconds: f32,
}

/// The host's table: the weapon, the attacker's and the victim's entity type ids, and the blow without enchantments; returns the blow with them.
pub type MeleeGearFn = Box<dyn Fn(&ItemStack, &str, &str, Blow) -> Blow + Send + Sync>;

#[derive(Resource)]
pub struct MeleeGear(pub MeleeGearFn);

/// The entity type id of a blow's victim.
pub fn victim_kind(w: &World, who: Who) -> &'static str {
    match who {
        Who::Player(_) => "minecraft:player",
        Who::Entity(t) => w.get::<SpeciesId>(t).map_or("", |s| w.resource::<SpeciesTable>().0[s.0 as usize].name),
    }
}

/// The blow of `e` on `who` with the enchantments of its main-hand weapon; `base` unchanged when there is no table or no weapon.
pub fn blow(w: &World, e: Entity, who: Who, base: Blow) -> Blow {
    let Some(gear) = w.get_resource::<MeleeGear>() else { return base };
    let Some(stack) = crate::equip_stack::get(w, e, 0) else { return base };
    let attacker = w.get::<SpeciesId>(e).map_or("", |s| w.resource::<SpeciesTable>().0[s.0 as usize].name);
    (gear.0)(&stack, attacker, victim_kind(w, who), base)
}
