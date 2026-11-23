//! `finalizeSpawn` of a mob born in the wild or by a generator (`finalize.rs`: `Mob`, `AgeableMob`, `Zombie`): the babies of a group,
//! the baby zombie and its chicken jockey, the follow-range bonus and the left hand. Each draw is a pure function (`*_roll`, in `rules/`) that keeps
//! the source's draw order (a `&&` draws only when it is reached); `finalize` applies the result to the world. The kind is the species'
//! configuration (`Finalize`), never its name. Not here, to port next: the armour and the weapon (`populate`), the enchantments, the
//! zombie's attributes, the variants that read the biome or a structure (`VariantRegistry::select`), the sniffer's baby age.
use crate::cow::{box_at, now_of, Body, Bounds, GrowAt, Index, Pos, SpeciesId, Sizes, Species};
use crate::mount::ride;
pub use crate::rules::spawn_finalize::{
    ageable_roll, mob_roll, select_variant, zombie_roll, Cond, Group, Here, Jockey, MobRoll, Selector, Shape, BABY_CHANCE, BABY_TICKS, FOLLOW_DEVIATION, NEVER,
};
use crate::targeting::Who;
use bevy_ecs::prelude::*;
use horizon_core::random::RandomSource;

/// How a species is finalized at its spawn (its configuration in `species/<name>.rs`).
#[derive(Clone, Copy, Debug)]
pub enum Finalize {
    /// `Mob.finalizeSpawn` alone.
    Mob,
    /// `AgeableMob.finalizeSpawn`: the babies of the group.
    Ageable,
    /// `Zombie.finalizeSpawn`: a baby that stays one, a chicken jockey (the name of the vehicle's species), and the class's default gear.
    Zombie { jockey: &'static str },
    /// A class with its own gear (`spawn_gear::Gear`): `Mob`'s draws and the class's gear, in the class's order.
    Geared(&'static crate::spawn_gear::Kit),
}

/// The difficulty's special multiplier (`DifficultyInstance.getSpecialMultiplier`) the host last set; the draws that scale with it read it
/// where the spawn has no place of its own (a reinforcement). Absent: 0 (no pick-up, no door draw).
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct Special(pub f32);

/// What finalizing a mob decided, for the caller to keep: the group (`SpawnGroupData` for the next mob of the pack), and the draws.
#[derive(Clone, Debug, PartialEq)]
pub struct Outcome {
    pub group: Option<Group>,
    pub mob: MobRoll,
    /// `Zombie.setCanBreakDoors`'s draw (no component holds the flag yet); false for a class that does not draw it.
    pub break_doors: bool,
    /// The pieces put in the slots (armour into empty slots only), already applied.
    pub gear: Vec<(crate::goals::Slot, String)>,
}

/// The kind of a species from its class bits (`cow::tag`): the zombie's class, the ageing one, else `Mob`'s alone. The default
/// gear is the species' own configuration, so it is `None` here.
pub fn kind_of(w: &World, e: Entity) -> Finalize {
    use crate::cow::tag;
    let bits = w.get::<SpeciesId>(e).map_or(0, |id| crate::cow::tags_of_id(*id));
    if bits & tag::ZOMBIE != 0 {
        Finalize::Zombie { jockey: crate::species::lists::CHICKEN }
    } else if let Some(kit) = crate::spawn_gear::kit_of(w, e) {
        Finalize::Geared(kit)
    } else if bits & tag::AGEING != 0 && bits & tag::TADPOLE == 0 {
        Finalize::Ageable
    } else {
        Finalize::Mob
    }
}

/// `finalizeSpawn` of `kind` on the mob `e` (already in the world). The class's draws come in the order of the source: the zombie
/// takes `Mob`'s first, then its pick-up roll (none for a `Conversion`), the group, the door draw, and the default gear (none for a
/// `Conversion`); the ageable one its babies then `Mob`'s. The follow bonus and the left hand are returned for the attribute and the
/// flag of the caller (no component of the prototype holds them yet). `reason` is the spawn's (`EntitySpawnReason`), `special` the
/// difficulty's special multiplier at the spawn. In the jar only the trial spawner keeps its mob (`setPersistenceRequired`).
pub fn finalize(
    w: &mut World,
    e: Entity,
    kind: Finalize,
    group: Option<Group>,
    reason: crate::spawn_reason::SpawnReason,
    special: f32,
    r: &mut impl RandomSource,
) -> Outcome {
    use crate::spawn_reason::SpawnReason as Why;
    let out = finalize_class(w, e, kind, group, reason, special, r);
    if reason == Why::Triggered {
        crate::behaviours::emerge::triggered(w, e);
    }
    // `Mob.finalizeSpawn`: the hand drawn (5% left-handed), synced in the mob flags.
    crate::left_handed::set(w, e, out.mob.left_handed);
    if reason == Why::TrialSpawner {
        crate::despawn::keep(w, e);
    }
    out
}

fn finalize_class(
    w: &mut World,
    e: Entity,
    kind: Finalize,
    group: Option<Group>,
    reason: crate::spawn_reason::SpawnReason,
    special: f32,
    r: &mut impl RandomSource,
) -> Outcome {
    use crate::spawn_reason::SpawnReason as Why;
    let plain = |group, mob| Outcome { group, mob, break_doors: false, gear: Vec::new() };
    match kind {
        Finalize::Mob => {
            crate::raids::finalize_raider(w, e, reason, r);
            plain(group, mob_roll(r))
        }
        Finalize::Ageable => {
            let table = crate::behaviours::variant::table_of(w, e);
            // A species whose group makes later members babies for certain (`FoxGroupData`) has no 5 % roll of its own.
            let group = match (group, table.and_then(|t| t.group_baby)) {
                (None, Some(_)) => Some(Group::Ageable { should_baby: false, size: 0, chance: BABY_CHANCE, variant: None }),
                (g, _) => g,
            };
            let certain = match (group, table.and_then(|t| t.group_baby)) {
                (Some(Group::Ageable { size, .. }), Some(n)) => size >= n,
                _ => false,
            };
            let (rolled, mut g) = ageable_roll(group, r);
            if rolled || certain {
                set_baby(w, e, BABY_TICKS);
            }
            // A registry variant is drawn per mob (`selectVariantToSpawn`); the others are the group's (`Fox`, `Horse`, `Llama`, `Rabbit`).
            let shared = table.is_some_and(|t| !matches!(t.pick, crate::rules::variant::Pick::Registry(_)));
            if let (true, Group::Ageable { variant, .. }, Some(v)) = (shared, &mut g, w.get::<crate::behaviours::variant::Variant>(e)) {
                *variant = Some(v.0);
            }
            plain(Some(g), mob_roll(r))
        }
        Finalize::Zombie { jockey } => {
            let mob = mob_roll(r);
            if reason != Why::Conversion {
                let picks = r.next_float() < 0.55 * special;
                if let Some(mut p) = w.get_mut::<crate::behaviours::item_pickup::Pickup>(e) {
                    p.picks_up = picks;
                }
            }
            let (g, j) = zombie_roll(group, r);
            if let Group::Zombie { baby: true, .. } = g {
                set_baby(w, e, NEVER);
                match j {
                    Jockey::None => {}
                    Jockey::Free => {
                        if let Some(c) = free_vehicle(w, e, jockey) {
                            w.entity_mut(c).insert(crate::loot::RewardOverride(crate::loot::JOCKEY_XP));
                            ride(w, c, Who::Entity(e));
                        }
                    }
                    Jockey::Create => {
                        if let Some(c) = create_vehicle(w, e, jockey) {
                            w.entity_mut(c).insert(crate::loot::RewardOverride(crate::loot::JOCKEY_XP));
                            ride(w, c, Who::Entity(e));
                        }
                    }
                }
            }
            let break_doors = r.next_float() < 0.1 * special;
            let mut put = Vec::new();
            let baby = matches!(g, Group::Zombie { baby: true, .. });
            let kit = crate::spawn_gear::kit_of(w, e);
            if reason != Why::Conversion
                && let Some(kit) = kit
            {
                crate::spawn_gear::populate(w, e, kit, special, baby, r, &mut put);
                crate::spawn_gear::after(w, e, kit, special, r, &mut put);
            }
            // `Zombie.handleAttributes`: the reinforcement base and the leader's bonuses (a leader breaks doors).
            let leader = crate::reinforcements::handle_attributes(w, e, special, reason, r);
            if let Some(kit) = kit {
                crate::spawn_gear::late(w, e, kit, baby, r, &mut put);
            }
            Outcome { group: Some(g), mob, break_doors: break_doors || leader, gear: put }
        }
        Finalize::Geared(kit) => {
            let mut put = Vec::new();
            let ageing = kit.baby.is_none() && w.get::<SpeciesId>(e).is_some_and(|id| crate::cow::tags_of_id(*id) & crate::cow::tag::AGEING != 0);
            let group = if ageing {
                let (baby, g) = ageable_roll(group, r);
                if baby {
                    set_baby(w, e, BABY_TICKS);
                }
                Some(g)
            } else {
                group
            };
            let first = kit.mob_first.then(|| mob_roll(r));
            let baby = crate::spawn_gear::baby_branch(w, e, kit, reason, r, &mut put);
            crate::spawn_gear::populate(w, e, kit, special, baby, r, &mut put);
            crate::spawn_gear::after(w, e, kit, special, r, &mut put);
            // `PatrollingMonster.finalizeSpawn` comes after the species' gear and before `Mob`'s draws.
            crate::raids::finalize_raider(w, e, reason, r);
            let mob = first.unwrap_or_else(|| mob_roll(r));
            Outcome { group, mob, break_doors: false, gear: put }
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
    crate::attr_mod::apply_load(w, e);
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
