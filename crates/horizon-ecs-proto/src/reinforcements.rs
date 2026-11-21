//! A hurt creature that calls another of its kind (`Zombie.hurtServer`): on the hardest difficulty, with the chance of its
//! `spawn_reinforcements` attribute and while monsters may spawn, a copy of its species appears 7 to 40 blocks away, out of sight of the
//! players, and takes the same target. Both then call less often. Nothing runs per tick: the damage step calls `on_hurt` for each hit, and a
//! creature without `Reinforces` (set by the species table) returns at once.
use crate::attr_mod::{self, AttrMods, Modifier, Operation};
use crate::cow::{Rng, SpeciesId, SpeciesTable};
use crate::rules::{no_attrs, no_interact, params, Rule};
use crate::spawn_finalize::{finalize, Finalize};
use crate::targeting::{self, Target, Who};
use bevy_ecs::prelude::*;
use horizon_core::geometry::{Aabb, Vec3};
use horizon_core::random::LegacyRandom;

/// `Difficulty.HARD`'s id.
const HARD: u8 = 3;
const ATTR: &str = "spawn_reinforcements";
const CALLER_ID: &str = "minecraft:reinforcement_caller_charge";
const CALLEE_ID: &str = "minecraft:reinforcement_callee_charge";

/// The level's `spawn_mobs` and `spawn_monsters` rules (`ServerLevel.isSpawningMonsters`), written by the host; absent: both on.
#[derive(Resource, Clone, Copy, Debug)]
pub struct MonsterSpawning(pub bool);

/// How a species calls reinforcements.
#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The attribute's base when none was written over it (`Attributes.SPAWN_REINFORCEMENTS_CHANCE`'s default).
    pub base: f64,
    /// `Zombie.randomizeReinforcementsChance`: the base drawn at the spawn is `nextDouble * spread` (0: none drawn, `ZombifiedPiglin`).
    pub spread: f64,
    /// `REINFORCEMENT_ATTEMPTS`, `REINFORCEMENT_RANGE_MIN` and `REINFORCEMENT_RANGE_MAX`.
    pub attempts: u32,
    pub min: i32,
    pub max: i32,
    /// What a call takes off the caller's chance (`REINFORCEMENT_CALLER_CHARGE_ID`) and off the callee's.
    pub caller_charge: f64,
    pub callee_charge: f32,
    /// No living player within this many blocks of the spot.
    pub clear_radius: f64,
    /// How the copy is finalized (`finalizeSpawn` with the reinforcement reason).
    pub finalize: Finalize,
}
impl Params {
    pub const ZOMBIE: Self = Self {
        base: 0.0,
        spread: 0.100_000_001_490_116_12,
        attempts: 50,
        min: 7,
        max: 40,
        caller_charge: 0.05,
        callee_charge: 0.05,
        clear_radius: 7.0,
        finalize: Finalize::Zombie { jockey: "minecraft:chicken" },
    };
    /// `ZombifiedPiglin.randomizeReinforcementsChance` sets the base to 0 and draws nothing.
    pub const PIGLIN: Self = Self { spread: 0.0, ..Self::ZOMBIE };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Reinforces(pub &'static Params);
/// The rule `reinforcements`, parameters `Params`.
pub static RULE: Rule = Rule {
    name: "reinforcements",
    equip: |w, e, p| {
        w.entity_mut(e).insert(Reinforces(params::<Params>(p)));
    },
    attrs: no_attrs,
    interact: no_interact,
};

/// The chance of `e`: its base (written over, else the species') with its modifiers, within the attribute's range.
pub fn chance(w: &World, e: Entity, p: &Params) -> f64 {
    let entry = w.get::<AttrMods>(e).and_then(|m| m.0.iter().find(|x| x.0 == ATTR));
    let base = entry.map(|x| x.1).filter(|b| !b.is_nan()).unwrap_or(p.base);
    attr_mod::value(base, entry.map_or(&[][..], |x| &x.2[..])).clamp(0.0, 1.0)
}
/// Replaces the modifier `id` of `e`'s chance by the one `f` makes of the amount it had (0 when absent).
fn charge(w: &mut World, e: Entity, id: &str, f: impl FnOnce(f64) -> f64) {
    if w.get::<AttrMods>(e).is_none() {
        w.entity_mut(e).insert(AttrMods::default());
    }
    let mut list = w.get_mut::<AttrMods>(e).unwrap();
    let i = match list.0.iter().position(|x| x.0 == ATTR) {
        Some(i) => i,
        None => {
            list.0.push((ATTR.to_owned(), f64::NAN, Vec::new()));
            list.0.len() - 1
        }
    };
    let mods = &mut list.0[i].2;
    let old = mods.iter().find(|m| m.id == id).map_or(0.0, |m| m.amount);
    mods.retain(|m| m.id != id);
    mods.push(Modifier { id: id.to_owned(), amount: f(old), operation: Operation::AddValue, permanent: true });
    attr_mod::touched(w, e, ATTR);
}
/// Writes `f` over the entry of attribute `attr` of `e` (created without a base written over the species').
fn edit(w: &mut World, e: Entity, attr: &str, f: impl FnOnce(&mut (String, f64, Vec<Modifier>))) {
    if w.get::<AttrMods>(e).is_none() {
        w.entity_mut(e).insert(AttrMods::default());
    }
    let mut list = w.get_mut::<AttrMods>(e).unwrap();
    let i = list.0.iter().position(|x| x.0 == attr).unwrap_or_else(|| {
        list.0.push((attr.to_owned(), f64::NAN, Vec::new()));
        list.0.len() - 1
    });
    f(&mut list.0[i]);
    attr_mod::touched(w, e, attr);
}
fn put(w: &mut World, e: Entity, attr: &str, id: &str, amount: f64, operation: Operation) {
    edit(w, e, attr, |x| {
        x.2.retain(|m| m.id != id);
        x.2.push(Modifier { id: id.to_owned(), amount, operation, permanent: true });
    });
}
/// `Zombie.handleAttributes` (the end of `finalizeSpawn`, `special` the difficulty's special multiplier): the reinforcement chance's base,
/// the random knockback resistance and follow range, and a leader's bonuses. True when the zombie became a leader (it breaks doors).
pub fn handle_attributes(w: &mut World, e: Entity, special: f32, reason: crate::spawn_reason::SpawnReason, r: &mut impl horizon_core::random::RandomSource) -> bool {
    let Some(p) = w.get::<Reinforces>(e).map(|x| x.0) else { return false };
    if p.spread > 0.0 {
        let base = r.next_double() * p.spread;
        edit(w, e, ATTR, |x| x.1 = base);
    }
    let kb = r.next_double() * 0.050_000_000_745_058_06;
    put(w, e, "knockback_resistance", "minecraft:random_spawn_bonus", kb, Operation::AddValue);
    let follow = r.next_double() * 1.5 * f64::from(special);
    if follow > 1.0 {
        put(w, e, "follow_range", "minecraft:zombie_random_spawn_bonus", follow, Operation::AddMultipliedTotal);
    }
    if r.next_float() >= special * 0.05 {
        return false;
    }
    let bonus = r.next_double() * 0.25 + 0.5;
    put(w, e, ATTR, "minecraft:leader_zombie_bonus", bonus, Operation::AddValue);
    let health = r.next_double() * 3.0 + 1.0;
    put(w, e, "max_health", "minecraft:leader_zombie_bonus", health, Operation::AddMultipliedTotal);
    // `setHealth(getMaxHealth())`, except for a zombie that was converted, loaded or travelled (it keeps the health it had).
    use crate::spawn_reason::SpawnReason as Why;
    if !matches!(reason, Why::Conversion | Why::Load | Why::DimensionTravel) {
        let max = crate::cow::max_health(w, e);
        if let Some(mut h) = w.get_mut::<crate::cow::Health>(e) {
            h.0 = max;
        }
    }
    true
}
/// `ServerLevel.hasNearbyAlivePlayer`.
fn player_near(w: &World, at: Vec3, r: f64) -> bool {
    let area = Aabb::new(Vec3::new(at.x - r - 2.0, at.y - r - 2.0, at.z - r - 2.0), Vec3::new(at.x + r + 2.0, at.y + r + 2.0, at.z + r + 2.0));
    crate::area::players_in(w, area).into_iter().any(|id| {
        targeting::info(w, Who::Player(id)).is_some_and(|i| (i.pos.x - at.x).powi(2) + (i.pos.y - at.y).powi(2) + (i.pos.z - at.z).powi(2) < r * r)
    })
}
/// `Mth.nextInt(random, min, max)`.
fn between(r: &mut Rng, min: i32, max: i32) -> i32 {
    min + r.next_int(max - min + 1)
}

/// A hit landed on `e` (`by` the attacker, a player or a creature): the target is taken, then a copy may be called.
pub fn on_hurt(w: &mut World, e: Entity, by: Option<Who>) {
    let Some(p) = w.get::<Reinforces>(e).map(|r| r.0) else { return };
    if w.get::<Target>(e).is_none() && let Some(who) = by {
        targeting::set_target(w, e, Target::new(who, 0, u8::MAX));
    }
    let Some(target) = w.get::<Target>(e).map(|t| t.who) else { return };
    if crate::effects::difficulty(w) != HARD {
        return;
    }
    let chance = chance(w, e, p);
    let Some(mut rng) = w.get::<Rng>(e).copied() else { return };
    let roll = rng.next_f64();
    *w.get_mut::<Rng>(e).unwrap() = rng;
    if roll >= chance || !w.get_resource::<MonsterSpawning>().is_none_or(|m| m.0) {
        return;
    }
    let (Some(from), Some(sid)) = (w.get::<crate::cow::Pos>(e).map(|p| p.0), w.get::<SpeciesId>(e).map(|s| s.0)) else { return };
    let species = w.resource::<SpeciesTable>().0[usize::from(sid)];
    let mut at = None;
    let mut seed = 0u64;
    for _ in 0..p.attempts {
        let spot = {
            let mut r = w.get_mut::<Rng>(e).unwrap();
            let mut axis = |v: f64| v.floor() as i32 + between(&mut r, p.min, p.max) * between(&mut r, -1, 1);
            let x = axis(from.x);
            let y = axis(from.y);
            let z = axis(from.z);
            seed = (u64::from(r.next_u32()) << 32) | u64::from(r.next_u32());
            [x, y, z]
        };
        let mut draws = LegacyRandom::new(seed as i64);
        let corner = Vec3::new(f64::from(spot[0]), f64::from(spot[1]), f64::from(spot[2]));
        if crate::spawning::fits_at_corner(w, &species, &mut draws, spot) && !player_near(w, corner, p.clear_radius) {
            at = Some((spot, corner));
            break;
        }
    }
    let Some((_, corner)) = at else { return };
    let near = crate::spawn_spawner::nearest_sqr(w, [corner.x, corner.y, corner.z]);
    let copy = crate::spawning::born(w, &species, [corner.x, corner.y, corner.z], seed, near);
    let mut t = Target::new(target, 0, u8::MAX);
    t.must_see = false;
    targeting::set_target(w, copy, t);
    let special = w.get_resource::<crate::spawn_finalize::Special>().map_or(0.0, |s| s.0);
    finalize(w, copy, p.finalize, None, crate::spawn_reason::SpawnReason::Reinforcement, special, &mut LegacyRandom::new(seed as i64));
    charge(w, copy, CALLEE_ID, |_| -f64::from(p.callee_charge));
    charge(w, e, CALLER_ID, |old| old - p.caller_charge);
}
