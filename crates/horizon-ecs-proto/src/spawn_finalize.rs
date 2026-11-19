//! `finalizeSpawn` of a mob born in the wild or by a generator (`finalize.rs`: `Mob`, `AgeableMob`, `Zombie`): the babies of a group,
//! the baby zombie and its chicken jockey, the follow-range bonus and the left hand. Each draw is a pure function (`*_roll`) that keeps
//! the source's draw order (a `&&` draws only when it is reached); `finalize` applies the result to the world. The kind is the species'
//! configuration (`Finalize`), never its name. Not here, to port next: the armour and the weapon (`populate`), the enchantments, the
//! zombie's attributes, the variants that read the biome or a structure (`VariantRegistry::select`), the sniffer's baby age.
use crate::cow::{box_at, now_of, Body, Bounds, GrowAt, Index, Pos, SpeciesId, Sizes, Species};
use crate::mount::ride;
use crate::targeting::Who;
use bevy_ecs::prelude::*;
use horizon_core::random::RandomSource;

/// `Ageable::babies`: the chance of a baby in a group of animals.
pub const BABY_CHANCE: f32 = 0.05;
/// `AgeableMob`'s baby age (-24000) as ticks to grow up.
pub const BABY_TICKS: u64 = 24_000;
/// The deviation of the triangle of the follow-range bonus (`Mob.finalizeSpawn`, `AddMultipliedBase`).
pub const FOLLOW_DEVIATION: f64 = 0.114_850_000_000_000_01;
/// A baby that never grows up (a zombie's): the sentinel of the wheel, as for an egg that is not laid.
pub const NEVER: u64 = u64::MAX;

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

/// The group a cluster of spawns shares (`AgeableMobGroupData`, `Zombie`'s group).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Group {
    Ageable { should_baby: bool, size: u32, chance: f32 },
    Zombie { baby: bool, jockey: bool },
}
impl Group {
    /// `new AgeableMobGroupData(true)`: a fresh group, the first mob never a baby (its size is 0).
    pub const AGEABLE: Self = Self::Ageable { should_baby: true, size: 0, chance: BABY_CHANCE };
}

/// The chicken draws of a baby zombie: an existing free vehicle first, else a new one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Jockey {
    None,
    Free,
    Create,
}

/// The draws of `Mob.finalizeSpawn` that every class makes: the follow-range bonus and the left hand.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MobRoll {
    pub follow_bonus: f64,
    pub left_handed: bool,
}
pub fn mob_roll(r: &mut impl RandomSource) -> MobRoll {
    let follow_bonus = r.triangle(0.0, FOLLOW_DEVIATION);
    MobRoll { follow_bonus, left_handed: r.next_float() < 0.05 }
}

/// `AgeableMob.finalizeSpawn`: whether this mob is a baby, and the group after it (its size grows by one).
pub fn ageable_roll(group: Option<Group>, r: &mut impl RandomSource) -> (bool, Group) {
    let g = group.unwrap_or(Group::AGEABLE);
    let Group::Ageable { should_baby, size, chance } = g else { return (false, g) };
    let baby = should_baby && size > 0 && r.next_float() <= chance;
    (baby, Group::Ageable { should_baby, size: size + 1, chance })
}

/// `Zombie.finalizeSpawn`: a new group draws its baby (5 %) and takes a jockey; the chicken draws follow when it is a baby.
pub fn zombie_roll(group: Option<Group>, r: &mut impl RandomSource) -> (Group, Jockey) {
    let g = group.unwrap_or_else(|| Group::Zombie { baby: r.next_float() < 0.05, jockey: true });
    let Group::Zombie { baby, jockey } = g else { return (g, Jockey::None) };
    if !baby || !jockey {
        return (g, Jockey::None);
    }
    if f64::from(r.next_float()) < 0.05 {
        (g, Jockey::Free)
    } else if f64::from(r.next_float()) < 0.05 {
        (g, Jockey::Create)
    } else {
        (g, Jockey::None)
    }
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
        if near && free && best.is_none_or(|b: u32| c.index() < b) {
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

/// A condition of a variant's `spawn_conditions` (`VariantRegistry`): the biome list (`#tag` for a tag), the structures, the moon.
#[derive(Clone, Debug, PartialEq)]
pub enum Cond {
    Biome(Vec<String>),
    Structure(Vec<String>),
    Moon(Option<f64>, Option<f64>),
}

/// One entry's condition at a priority; `None`: always true.
#[derive(Clone, Debug, PartialEq)]
pub struct Selector {
    pub entry: usize,
    pub cond: Option<Cond>,
    pub priority: i32,
}

/// What the spawn's place is: its biome, the biome tags it has (without `#`), the structures with a piece there, the moon's brightness.
pub struct Here<'a> {
    pub biome: &'a str,
    pub biome_tags: &'a [&'a str],
    pub structures: &'a [String],
    pub moon: f64,
}

/// `VariantUtils.selectVariantToSpawn`: the entries of the highest priority whose condition holds, one of them drawn (the
/// priorities are read from the highest, as `VariantRegistry::select` does).
pub fn select_variant(sel: &[Selector], here: &Here, r: &mut impl RandomSource) -> Option<usize> {
    let mut order: Vec<&Selector> = sel.iter().collect();
    order.sort_by_key(|s| std::cmp::Reverse(s.priority));
    let mut highest = i32::MIN;
    let mut picked = Vec::new();
    for s in order {
        if s.priority < highest {
            continue;
        }
        let ok = match &s.cond {
            None => true,
            Some(Cond::Biome(list)) => list.iter().any(|b| match b.strip_prefix('#') {
                Some(tag) => here.biome_tags.contains(&tag),
                None => here.biome == b,
            }),
            Some(Cond::Structure(list)) => here.structures.iter().any(|x| list.contains(x)),
            Some(Cond::Moon(min, max)) => min.is_none_or(|m| here.moon >= m) && max.is_none_or(|m| here.moon <= m),
        };
        if ok {
            highest = s.priority;
            picked.push(s.entry);
        }
    }
    if picked.is_empty() {
        return None;
    }
    Some(picked[r.next_int_bound(picked.len() as i32).unwrap_or(0) as usize])
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
