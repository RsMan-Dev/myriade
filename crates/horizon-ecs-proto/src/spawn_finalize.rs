//! `finalizeSpawn` of a mob born in the wild or by a generator (`finalize.rs`: `Mob`, `AgeableMob`, `Zombie`): the babies of a group,
//! the baby zombie and its chicken jockey, the follow-range bonus and the left hand. Each draw is a pure function (`*_roll`, in `rules/`) that keeps
//! the source's draw order (a `&&` draws only when it is reached); `finalize` applies the result to the world. The kind is the species'
//! configuration (`Finalize`), never its name. Not here, to port next: the armour and the weapon (`populate`), the enchantments, the
//! zombie's attributes, the variants that read the biome or a structure (`VariantRegistry::select`), the sniffer's baby age.
use crate::cow::{box_at, now_of, Body, Bounds, GrowAt, Index, Pos, SpeciesId, Sizes, Species};
use crate::mount::ride;
pub use crate::rules::spawn_finalize::{
    ageable_roll, mob_roll, select_variant, zombie_roll, Cond, Group, Here, Jockey, MobRoll, Selector, BABY_CHANCE, BABY_TICKS, FOLLOW_DEVIATION, NEVER,
};
use crate::targeting::Who;
use bevy_ecs::prelude::*;
use horizon_core::random::RandomSource;

/// How a species is finalized at its spawn (its configuration in `species/<name>.rs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Finalize {
    /// `Mob.finalizeSpawn` alone.
    Mob,
    /// `AgeableMob.finalizeSpawn`: the babies of the group.
    Ageable,
    /// `Zombie.finalizeSpawn`: a baby that stays one, and a chicken jockey (the name of the vehicle's species).
    Zombie { jockey: &'static str },
}

/// What finalizing a mob decided, for the caller to keep: the group, and the `Mob` draws.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Outcome {
    pub group: Option<Group>,
    pub mob: MobRoll,
}

/// `finalizeSpawn` of `kind` on the mob `e` (already in the world). The class's draws come in the order of the source: the zombie
/// takes `Mob`'s first, the ageable one its babies then `Mob`'s. The follow bonus and the left hand are returned for the attribute
/// and the flag of the caller (no component of the prototype holds them yet).
pub fn finalize(w: &mut World, e: Entity, kind: Finalize, group: Option<Group>, r: &mut impl RandomSource) -> Outcome {
    match kind {
        Finalize::Mob => Outcome { group, mob: mob_roll(r) },
        Finalize::Ageable => {
            let (baby, g) = ageable_roll(group, r);
            if baby {
                set_baby(w, e, BABY_TICKS);
            }
            Outcome { group: Some(g), mob: mob_roll(r) }
        }
        Finalize::Zombie { jockey } => {
            let mob = mob_roll(r);
            let (g, j) = zombie_roll(group, r);
            if let Group::Zombie { baby: true, .. } = g {
                set_baby(w, e, NEVER);
                match j {
                    Jockey::None => {}
                    Jockey::Free => {
                        if let Some(c) = free_vehicle(w, e, jockey) {
                            ride(w, c, Who::Entity(e));
                        }
                    }
                    Jockey::Create => {
                        if let Some(c) = create_vehicle(w, e, jockey) {
                            ride(w, c, Who::Entity(e));
                        }
                    }
                }
            }
            Outcome { group: Some(g), mob }
        }
    }
}

/// The baby's size and its growth (`make_baby`: the baby box, one wheel entry at `ticks` ahead; `NEVER` for a baby that stays one).
pub fn set_baby(w: &mut World, e: Entity, ticks: u64) {
    let (Some(s), Some(p)) = (w.get::<Sizes>(e).copied(), w.get::<Pos>(e).map(|p| p.0)) else { return };
    if let Some(mut b) = w.get_mut::<Body>(e) {
        b.width = s.baby.0;
        b.height = s.baby.1;
    }
    if let Some(mut b) = w.get_mut::<Bounds>(e) {
        b.0 = box_at(p, s.baby.0, s.baby.1);
    }
    let at = if ticks == NEVER { NEVER } else { now_of(w, e).saturating_add(ticks) };
    w.entity_mut(e).insert(GrowAt(at));
}

/// `Zombie`'s search: a free creature of the vehicle's species within 5 blocks sideways and 3 high (the inflated box of the rider).
fn free_vehicle(w: &World, rider: Entity, name: &str) -> Option<Entity> {
    let id = species_id(name)?;
    let p = w.get::<Pos>(rider)?.0;
    let (mut found, mut best) = (None, None);
    w.resource::<Index>().for_near_touching(p.x, p.z, (p.x - 5.0, p.z - 5.0), (p.x + 5.0, p.z + 5.0), |c| {
        let near = w.get::<SpeciesId>(c).is_some_and(|s| s.0 == id)
            && w.get::<Pos>(c).is_some_and(|q| (q.0.y - p.y).abs() <= 3.0);
        let free = w.get::<crate::mount::Riding>(c).is_none() && crate::mount::rider_of(w, c).is_none();
        if near && free && best.is_none_or(|b| c.index() < b) {
            best = Some(c.index());
            found = Some(c);
        }
    });
    found
}

/// A new chicken jockey: the vehicle's species at the rider's place (`spawn_species` puts it in the index and its brain).
fn create_vehicle(w: &mut World, rider: Entity, name: &str) -> Option<Entity> {
    let at = w.get::<Pos>(rider)?.0;
    let sp: Species = crate::cow::species_table().0.into_iter().find(|s| s.name == name)?;
    let seed = w.get::<crate::cow::Rng>(rider).map_or(0, |g| g.0);
    let c = crate::cow::spawn_species(w, &sp, at.x, at.z, seed);
    w.get_mut::<Pos>(c)?.0 = at;
    Some(c)
}

fn species_id(name: &str) -> Option<u8> {
    crate::cow::species_table().0.iter().find(|s| s.name == name).map(|s| s.id)
}

/// The gear of a spawn: `equipment_spawn::populate` rolls it (armour, hand); the caller sets the slots (`equipment::set_slot`,
/// on the equipment branch, not in `src/` yet).
pub fn gear_roll(
    spawn: &crate::equipment_spawn::Spawn,
    special: f32,
    hard: bool,
    worn: impl Fn(crate::goals::Slot) -> bool,
    r: &mut impl RandomSource,
) -> Vec<(crate::goals::Slot, String)> {
    crate::equipment_spawn::populate(spawn, special, hard, worn, r)
}
