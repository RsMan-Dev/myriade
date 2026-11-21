//! Les champs NBT de créatures restantes : la laine d'un mouton (`Sheared`), la durée de vie d'un vex (`life_ticks`), la température d'un
//! cheval (`Temper`), le dauphin qui a mangé un poisson (`GotFish`) et la patrouille et la vague d'un pillard (`Raider`, `Patrol`).
//! Chaque clé n'est déclarée que pour le composant qui la porte ; sans lui, elle reste libre.
use crate::behaviours::fox_sleep::Sleeping;
use crate::behaviours::fox_state::Crouch;
use crate::behaviours::limited_life::Lifespan;
use crate::behaviours::swim_to_treasure::GotFish;
use crate::cow::now_of;
use crate::farm::Wool;
use crate::mount::Mount;
use crate::nbt::{accept, owner, Field};
use crate::raids::{Patrol, Raider};
use bevy_ecs::prelude::*;
use horizon_core::nbt::{Compound, Tag};
use std::borrow::Cow;

fn is_byte(t: &Tag) -> bool {
    matches!(t, Tag::Byte(_))
}

fn is_int_array(t: &Tag) -> bool {
    matches!(t, Tag::IntArray(_))
}

/// The keys that apply to an entity holding their component.
pub const FIELDS: &[Field] = &[
    Field { key: Cow::Borrowed("Sheared"), write: write_sheared, read: read_sheared, accepts: is_byte, owner: Some(owner::<Wool>) },
    Field { key: Cow::Borrowed("Temper"), write: write_temper, read: read_temper, accepts: accept::int, owner: Some(owner::<Mount>) },
    Field { key: Cow::Borrowed("Crouching"), write: write_crouching, read: read_crouching, accepts: is_byte, owner: Some(owner::<Crouch>) },
];

/// The keys whose presence poses their component (`nbt::register_creators`).
pub const CREATORS: &[Field] = &[
    Field { key: Cow::Borrowed("life_ticks"), write: write_life, read: read_life, accepts: accept::int, owner: Some(owner::<Lifespan>) },
    Field { key: Cow::Borrowed("GotFish"), write: write_got_fish, read: read_got_fish, accepts: is_byte, owner: Some(owner::<GotFish>) },
    Field { key: Cow::Borrowed("Sleeping"), write: write_sleeping, read: read_sleeping, accepts: is_byte, owner: Some(owner::<Sleeping>) },
    Field { key: Cow::Borrowed("RaidId"), write: write_raid_id, read: read_raid_id, accepts: accept::int, owner: Some(owner::<Raider>) },
    Field { key: Cow::Borrowed("Wave"), write: write_wave, read: read_wave, accepts: accept::int, owner: Some(owner::<Raider>) },
    Field { key: Cow::Borrowed("PatrolLeader"), write: write_patrol_leader, read: read_patrol_leader, accepts: is_byte, owner: Some(owner::<Patrol>) },
    Field { key: Cow::Borrowed("Patrolling"), write: write_patrolling, read: read_patrolling, accepts: is_byte, owner: Some(owner::<Patrol>) },
    Field { key: Cow::Borrowed("patrol_target"), write: write_patrol_target, read: read_patrol_target, accepts: is_int_array, owner: Some(owner::<Patrol>) },
];

/// `Sheared`: a byte, 1 while the sheep's wool is off.
fn write_sheared(w: &World, e: Entity, c: &mut Compound) {
    if let Some(s) = w.get::<Wool>(e) {
        c.insert("Sheared".into(), Tag::Byte(i8::from(s.sheared)));
    }
}

fn read_sheared(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    let Some(mut s) = w.get_mut::<Wool>(e) else { return false };
    s.sheared = *v != 0;
    true
}

/// `Temper`: the horse's temper, an int, as `Mount.temper`.
fn write_temper(w: &World, e: Entity, c: &mut Compound) {
    if let Some(m) = w.get::<Mount>(e).filter(|m| m.max_temper > 0) {
        c.insert("Temper".into(), Tag::Int(m.temper));
    }
}

fn read_temper(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Int(v) = t else { return false };
    let Some(mut m) = w.get_mut::<Mount>(e).filter(|m| m.max_temper > 0) else { return false };
    m.temper = *v;
    true
}

/// `life_ticks`: the ticks left before the creature is hurt each tick, an int; from `Lifespan` (the tick it ends at).
fn write_life(w: &World, e: Entity, c: &mut Compound) {
    let Some(l) = w.get::<Lifespan>(e) else { return };
    let left = l.0.saturating_sub(now_of(w, e));
    c.insert("life_ticks".into(), Tag::Int(i32::try_from(left).unwrap_or(i32::MAX)));
}

/// Zero or less: the creature is hurt from the next tick on. The read sets the component back, as a creator.
fn read_life(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Int(v) = t else { return false };
    let at = now_of(w, e) + u64::try_from(*v).unwrap_or(0);
    w.entity_mut(e).insert(Lifespan(at));
    true
}

/// `Crouching`: a byte, 1 while the fox crouches, from `Crouch` (the time it was set); the read sets the time to now.
fn write_crouching(w: &World, e: Entity, c: &mut Compound) {
    if let Some(Crouch(Some(_))) = w.get::<Crouch>(e) {
        c.insert("Crouching".into(), Tag::Byte(1));
    }
}

fn read_crouching(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    let now = now_of(w, e);
    let Some(mut c) = w.get_mut::<Crouch>(e) else { return false };
    c.0 = (*v != 0).then_some(now);
    true
}

/// `Sleeping`: a byte, 1 while the fox's sleep goal runs (the marker is there); 0 takes it away.
fn write_sleeping(w: &World, e: Entity, c: &mut Compound) {
    if w.get::<Sleeping>(e).is_some() {
        c.insert("Sleeping".into(), Tag::Byte(1));
    }
}

fn read_sleeping(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    if *v != 0 {
        w.entity_mut(e).insert(Sleeping);
    } else {
        w.entity_mut(e).remove::<Sleeping>();
    }
    true
}

/// `GotFish`: a byte, 1 while the dolphin has eaten its fish (the marker is there).
fn write_got_fish(w: &World, e: Entity, c: &mut Compound) {
    if w.get::<GotFish>(e).is_some() {
        c.insert("GotFish".into(), Tag::Byte(1));
    }
}

fn read_got_fish(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    if *v != 0 {
        w.entity_mut(e).insert(GotFish);
    }
    true
}

/// `RaidId`: the raid the raider is in, an int, from `Raider.raid`.
fn write_raid_id(w: &World, e: Entity, c: &mut Compound) {
    if let Some(id) = w.get::<Raider>(e).and_then(|r| r.raid) {
        c.insert("RaidId".into(), Tag::Int(id));
    }
}

fn read_raid_id(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Int(v) = t else { return false };
    if w.get::<Raider>(e).is_none() {
        w.entity_mut(e).insert(Raider { raid: None, wave: 0, leader: false });
    }
    if let Some(mut r) = w.get_mut::<Raider>(e) {
        r.raid = Some(*v);
    }
    true
}

/// `Wave`: the wave the raider came with, an int, from `Raider.wave`.
fn write_wave(w: &World, e: Entity, c: &mut Compound) {
    if let Some(r) = w.get::<Raider>(e) {
        c.insert("Wave".into(), Tag::Int(r.wave));
    }
}

fn read_wave(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Int(v) = t else { return false };
    if w.get::<Raider>(e).is_none() {
        w.entity_mut(e).insert(Raider { raid: None, wave: 0, leader: false });
    }
    if let Some(mut r) = w.get_mut::<Raider>(e) {
        r.wave = *v;
    }
    true
}

/// `PatrolLeader`: a byte, from `Patrol.leader`.
fn write_patrol_leader(w: &World, e: Entity, c: &mut Compound) {
    if let Some(p) = w.get::<Patrol>(e) {
        c.insert("PatrolLeader".into(), Tag::Byte(i8::from(p.leader)));
    }
}

fn read_patrol_leader(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    patrol(w, e).leader = *v != 0;
    true
}

/// `Patrolling`: a byte, from `Patrol.patrolling`.
fn write_patrolling(w: &World, e: Entity, c: &mut Compound) {
    if let Some(p) = w.get::<Patrol>(e) {
        c.insert("Patrolling".into(), Tag::Byte(i8::from(p.patrolling)));
    }
}

fn read_patrolling(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    patrol(w, e).patrolling = *v != 0;
    true
}

/// `patrol_target`: the block the patrol walks to, three ints, from `Patrol.target` (absent when none).
fn write_patrol_target(w: &World, e: Entity, c: &mut Compound) {
    if let Some(p) = w.get::<Patrol>(e).and_then(|p| p.target) {
        c.insert("patrol_target".into(), Tag::IntArray(p.to_vec()));
    }
}

fn read_patrol_target(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::IntArray(v) = t else { return false };
    let [x, y, z] = v.as_slice() else { return false };
    patrol(w, e).target = Some([*x, *y, *z]);
    true
}

/// The patrol of the entity, made with its defaults when it has none (the creator's read).
fn patrol(w: &mut World, e: Entity) -> Mut<'_, Patrol> {
    if w.get::<Patrol>(e).is_none() {
        w.entity_mut(e).insert(Patrol::default());
    }
    w.get_mut::<Patrol>(e).expect("the patrol was just made")
}
