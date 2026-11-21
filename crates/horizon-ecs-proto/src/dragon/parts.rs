//! The dragon's eight parts (`EnderDragonPart`): where each stands along the body (`place`, Horizon's `place_dragon_parts`, D:2856)
//! and its box. The parts are data of the dragon, not entities; a hit names one by index (`hurt.rs`).
use super::data::*;
use super::flight::{FlightHistory, YawAccel};
use super::phases::PhaseId;
use crate::cow::{Pos, Yaw};
use crate::rules::dragon_parts as rule;
use bevy_ecs::prelude::*;
use horizon_core::geometry::Aabb;

pub use crate::rules::dragon_parts::wrap;

/// The position of one part, and the one before its last placement.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PartState {
    pub pos: [f64; 3],
    pub prev: [f64; 3],
}

/// Name and size of a part, for the species table (`PARTS`, written by `species/ender_dragon.rs`).
pub struct PartDef {
    pub name: &'static str,
    pub w: f32,
    pub h: f32,
}

/// The eight parts of one dragon, written by `place` at each flight wake and read by the boxes and the hit path.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Parts(pub Box<[PartState; 8]>);

impl Default for Parts {
    fn default() -> Self {
        Self(Box::new([PartState::default(); 8]))
    }
}

/// The box of part `i`: its size from `PART_SIZES`, centred on its position, standing on it.
pub fn part_box(parts: &Parts, i: usize) -> Aabb {
    rule::box_of(parts.0[i].pos, PART_SIZES[i])
}

/// Places the parts of dragon `e` from its position, yaw, yaw acceleration and flight history (`EnderDragon.aiStep`, through
/// `tickPart`). Called by the flight wake only.
pub fn place(w: &mut World, e: Entity) {
    let Some(hist) = w.get::<FlightHistory>(e) else { return };
    let s = [hist.sample(0), hist.sample(5), hist.sample(10)];
    let tail = [hist.sample(12), hist.sample(14), hist.sample(16)];
    let sitting = w.get::<PhaseId>(e).is_some_and(|p| is_sitting(p.0));
    let (Some(p), Some(yaw)) = (w.get::<Pos>(e).map(|p| p.0), w.get::<Yaw>(e).map(|y| y.0)) else { return };
    let accel = w.get::<YawAccel>(e).map_or(0.0, |a| a.0);
    let out = rule::place_points([p.x, p.y, p.z], yaw, accel, sitting, s, tail);

    if let Some(mut parts) = w.get_mut::<Parts>(e) {
        for (part, pos) in parts.0.iter_mut().zip(out) {
            part.prev = part.pos;
            part.pos = pos;
        }
    }
}
