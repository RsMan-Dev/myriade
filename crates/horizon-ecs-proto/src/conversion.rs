//! `Mob.convertTo` with `ConversionType.SINGLE` (socle S8): the entity becomes the species `into` in its place. The new one is
//! born by `cow::spawn_species`, the old one leaves by `despawn::remove` (no loot). The callers go through `convert_or_ask`
//! (`behaviours/convert_timer.rs`): while the host listens (`ConversionWatched`) it asks first (`HostEvent::ConversionAsk`) and the
//! host answers with `resolve` (the new entity is born, the old one stays) then `finish` (the old one leaves); else it converts at once.
use crate::cow::{self, Bounds, Pos, Yaw};
use crate::despawn;
use crate::goals::{self, Equipment, Events, HandStack};
use crate::targeting::Who;
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;

/// What the new entity keeps of the old one (`ConversionParams`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConvertKeep {
    /// `keepEquipment`: the worn and held items move to the new entity.
    pub equipment: bool,
    /// The step the conversion tracker runs on the new entity (`ConversionTracker.doConversion`).
    pub post: Post,
}
/// What follows `convertTo` in a tracker's conversion (`postConversionStep`, then the level event unless the creature is silent).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Post {
    None,
    /// `handleAttributes(specialMultiplier, CONVERSION)` on the new entity, and the level event `sound`.
    Tracker { sound: i32 },
}
impl ConvertKeep {
    pub const ALL: Self = Self { equipment: true, post: Post::None };
    pub const NONE: Self = Self { equipment: false, post: Post::None };
}

/// Set by the host while a mod hears `living_conversion_pre` or `_post`: a conversion then waits for the host (`convert_or_ask`).
#[derive(Resource, Default)]
pub struct ConversionWatched(pub bool);

/// A conversion of `e` that the host has been asked about; `e` is unchanged until `resolve`.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct ConvertAsked {
    pub into: &'static str,
    pub keep: ConvertKeep,
}

/// `convert`, or, while the host listens (`ConversionWatched`), the question `HostEvent::ConversionAsk` first. True when the entity was
/// converted or the question is pending (a second call while pending changes nothing); false when nothing could be done.
pub fn convert_or_ask(w: &mut World, e: Entity, into: &'static str, keep: ConvertKeep) -> bool {
    if w.get::<ConvertAsked>(e).is_some() {
        return true;
    }
    if w.get_resource::<ConversionWatched>().is_some_and(|c| c.0) {
        w.entity_mut(e).insert(ConvertAsked { into, keep });
        w.resource_mut::<Events>().host.conversion_ask(e, into);
        return true;
    }
    convert(w, e, into, keep).is_some()
}

/// `Zombie.killedEntity`: a zombie that kills a villager on normal or hard turns it into a zombie villager (on normal, half the time).
/// True when the villager is converted or asked about, so that its death does not follow.
pub fn zombie_kill(w: &mut World, victim: Entity, by: Option<Who>) -> bool {
    let Some(Who::Entity(killer)) = by else { return false };
    if !matches!(crate::effects::difficulty(w), 2 | 3)
        || species_name(w, killer) != Some("minecraft:zombie")
        || species_name(w, victim) != Some(crate::species::lists::VILLAGER)
    {
        return false;
    }
    if crate::effects::difficulty(w) != 3 && goals::chance(w, killer, 0.5) {
        return false;
    }
    convert_or_ask(w, victim, "minecraft:zombie_villager", ConvertKeep::NONE)
}

fn species_name(w: &World, e: Entity) -> Option<&'static str> {
    let s = w.get::<cow::SpeciesId>(e)?;
    w.get_resource::<cow::SpeciesTable>().map(|t| t.0[s.0 as usize].name)
}

/// The host's answer: `cancel` keeps the entity as it is; otherwise the new entity is born and returned while the old one stays, for
/// the `living_conversion_post` hook, until `finish`. None when cancelled, not asked, or `into` unknown.
pub fn resolve(w: &mut World, e: Entity, cancel: bool) -> Option<Entity> {
    let asked = w.get_entity_mut(e).ok()?.take::<ConvertAsked>()?;
    // A shearing's burst and loot follow the conversion (`finalizeConversion`) and are lost with a refusal.
    let shear = w.entity_mut(e).take::<crate::behaviours::shear::PendingShear>();
    if cancel {
        return None;
    }
    let born = build(w, e, asked.into, asked.keep)?;
    if let Some(shear) = shear {
        shear.finish(w);
    }
    Some(born)
}

/// The old entity of a conversion leaves (no loot), once the host has told its mods.
pub fn finish(w: &mut World, e: Entity) {
    despawn::remove(w, e);
}

/// Converts `e` into the species `into`. `None` when `e` has no position or `into` is not a species of the registry (nothing changes).
pub fn convert(w: &mut World, e: Entity, into: &'static str, keep: ConvertKeep) -> Option<Entity> {
    let n = build(w, e, into, keep)?;
    despawn::remove(w, e);
    Some(n)
}

fn build(w: &mut World, e: Entity, into: &'static str, keep: ConvertKeep) -> Option<Entity> {
    let sp = cow::species_table().0.into_iter().find(|s| s.name == into)?;
    let pos = w.get::<Pos>(e)?.0;
    let yaw = w.get::<Yaw>(e).map(|y| y.0);
    let vel = w.get::<cow::Vel>(e).map(|v| v.0);
    let silent = w.get::<crate::behaviours::elder_curse::Silent>(e).is_some();
    let worn = if keep.equipment { w.get::<Equipment>(e).copied() } else { None };
    let held = if keep.equipment { w.get::<HandStack>(e).cloned() } else { None };
    let seed = w.get::<cow::Rng>(e).map_or(1, |r| r.0);

    let n = cow::spawn_species(w, &sp, pos.x as f64, pos.z as f64, seed);
    w.get_mut::<Pos>(n)?.0 = pos;
    if let Some(y) = yaw {
        w.get_mut::<Yaw>(n)?.0 = y;
    }
    // `ConversionType.SINGLE`: the position, rotation and motion carry over; the health does not (the new creature is born at its own maximum).
    if let Some(v) = vel
        && let Some(mut c) = w.get_mut::<cow::Vel>(n)
    {
        c.0 = v;
    }
    if let Some(mut b) = w.get_mut::<Bounds>(n) {
        b.0 = cow::box_at(Vec3::new(pos.x, pos.y, pos.z), sp.width, sp.height);
    }
    if let Some(q) = worn {
        w.entity_mut(n).insert(q);
    }
    if let Some(h) = held {
        w.entity_mut(n).insert(h);
    }
    copy_common(w, e, n);
    if let Post::Tracker { sound } = keep.post {
        let special = w.get_resource::<crate::spawn_finalize::Special>().map_or(0.0, |s| s.0);
        crate::reinforcements::handle_attributes(w, n, special, crate::spawn_reason::SpawnReason::Conversion, &mut horizon_core::random::LegacyRandom::new(seed as i64));
        if !silent {
            w.resource_mut::<Events>().host.level_event(sound, [pos.x.floor() as i32, pos.y.floor() as i32, pos.z.floor() as i32], 0);
        }
    }
    crate::goals::arm_timers(w, n);
    Some(n)
}

/// `ConversionType.convertCommon`: the name, the flags, the effects, the age, the drop chances and the persistence carry over
/// whatever the type (only a component the old creature has moves; an absent one stays absent on the new one).
fn copy_common(w: &mut World, e: Entity, n: Entity) {
    use crate::absorption::Absorption;
    use crate::custom_name::{CustomName, NameVisible};
    use crate::despawn::Persistent;
    use crate::effects::Effects;
    use crate::equipment_drop::DropChances;
    use crate::invulnerable::Invulnerable;
    use crate::left_handed::LeftHanded;
    use crate::no_gravity::NoGravity;
    use crate::cow::GrowAt;
    if let Some(c) = w.get::<CustomName>(e).cloned() {
        w.entity_mut(n).insert(c);
    }
    if w.get::<NameVisible>(e).is_some() {
        w.entity_mut(n).insert(NameVisible);
    }
    if w.get::<Persistent>(e).is_some() {
        w.entity_mut(n).insert(Persistent);
    }
    if let Some(c) = w.get::<Effects>(e).cloned() {
        w.entity_mut(n).insert(c);
    }
    if let Some(a) = w.get::<GrowAt>(e).copied() {
        w.entity_mut(n).insert(a);
    }
    if w.get::<LeftHanded>(e).is_some() {
        w.entity_mut(n).insert(LeftHanded);
    }
    if let Some(d) = w.get::<DropChances>(e).copied() {
        w.entity_mut(n).insert(d);
    }
    if let Some(a) = w.get::<Absorption>(e).copied() {
        w.entity_mut(n).insert(a);
    }
    if w.get::<NoGravity>(e).is_some() {
        w.entity_mut(n).insert(NoGravity);
    }
    if w.get::<Invulnerable>(e).is_some() {
        w.entity_mut(n).insert(Invulnerable);
    }
    if let Some(p) = w.get::<crate::behaviours::item_pickup::Pickup>(e).copied() {
        w.entity_mut(n).insert(p);
    }
}
