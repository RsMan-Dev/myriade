//! `Mob.convertTo` with `ConversionType.SINGLE` (socle S8): the entity becomes the species `into` in its place. The new one is
//! born by `cow::spawn_species`, the old one leaves by `despawn::remove` (no loot). The callers go through `convert_or_ask`
//! (`behaviours/convert_timer.rs`): while the host listens (`ConversionWatched`) it asks first (`HostEvent::ConversionAsk`) and the
//! host answers with `resolve` (the new entity is born, the old one stays) then `finish` (the old one leaves); else it converts at once.
use crate::cow::{self, Bounds, Health, Pos, Yaw};
use crate::despawn;
use crate::goals::{Equipment, Events, HandStack};
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;

/// What the new entity keeps of the old one (`ConversionParams`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConvertKeep {
    /// `keepEquipment`: the worn and held items move to the new entity.
    pub equipment: bool,
}
impl ConvertKeep {
    pub const ALL: Self = Self { equipment: true };
    pub const NONE: Self = Self { equipment: false };
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

/// The host's answer: `cancel` keeps the entity as it is; otherwise the new entity is born and returned while the old one stays, for
/// the `living_conversion_post` hook, until `finish`. None when cancelled, not asked, or `into` unknown.
pub fn resolve(w: &mut World, e: Entity, cancel: bool) -> Option<Entity> {
    let asked = w.get_entity_mut(e).ok()?.take::<ConvertAsked>()?;
    if cancel {
        return None;
    }
    build(w, e, asked.into, asked.keep)
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
    let health = w.get::<Health>(e).map(|h| h.0);
    let worn = if keep.equipment { w.get::<Equipment>(e).copied() } else { None };
    let held = if keep.equipment { w.get::<HandStack>(e).cloned() } else { None };
    let seed = w.get::<cow::Rng>(e).map_or(1, |r| r.0);

    let n = cow::spawn_species(w, &sp, pos.x as f64, pos.z as f64, seed);
    w.get_mut::<Pos>(n)?.0 = pos;
    if let Some(y) = yaw {
        w.get_mut::<Yaw>(n)?.0 = y;
    }
    if let Some(h) = health {
        if let Some(mut c) = w.get_mut::<Health>(n) {
            c.0 = h;
        }
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
    crate::goals::arm_timers(w, n);
    Some(n)
}
