//! Statut du raid vu par le villageois (`villager.rs:394-404`, `RAID_*` 328-341). Le statut est écrit par l'hôte aux
//! débuts et fins de raid (mémoire `RAID_STATUS`, entier) : interface déclarée, à brancher sur `game/mobs/raids/mod.rs`.
use crate::brain::*;
use bevy_ecs::prelude::*;

pub static RAID_STATUS: MemKey = MemKey::new("horizon:raid_status", Shape::Int);

pub const RAID_NONE: i64 = 0;
pub const RAID_ACTIVE: i64 = 1;
pub const RAID_VICTORY: i64 = 2;
pub const RAID_LOSS: i64 = 3;
pub const RAID_BEFORE: i64 = 4;

pub fn status(w: &World, e: Entity) -> i64 {
    int(w, e, &RAID_STATUS).unwrap_or(RAID_NONE)
}

/// A raid is on and the villager is in it (`raid_nearby`).
pub fn active(w: &World, e: Entity) -> bool {
    status(w, e) == RAID_ACTIVE
}

/// The host says a raid began / ended near `at`: the creatures with a brain within `radius` take the status.
pub fn set_status(w: &mut World, at: [f64; 3], radius: f64, status: i64) {
    let mut near = Vec::new();
    w.resource::<crate::cow::Index>().for_radius(at[0], at[2], radius, |o| near.push(o));
    for o in near {
        if w.get::<Brain>(o).is_some() {
            set(w, o, &RAID_STATUS, MemValue::Int(status), None);
        }
    }
}
