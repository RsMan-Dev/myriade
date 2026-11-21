//! `Mob.convertTo` with `ConversionType.SINGLE` (socle S8): the entity becomes the species `into` in its place. The new one is
//! born by `cow::spawn_species`, the old one leaves by `despawn::remove` (no loot). Callers: `behaviours/convert_timer.rs`.
use crate::cow::{self, Bounds, Health, Pos, Yaw};
use crate::despawn;
use crate::goals::{Equipment, HandStack};
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

/// Converts `e` into the species `into`. `None` when `e` has no position or `into` is not a species of the registry (nothing changes).
pub fn convert(w: &mut World, e: Entity, into: &'static str, keep: ConvertKeep) -> Option<Entity> {
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
    despawn::remove(w, e);
    Some(n)
}
