//! The dragon's eight parts (`EnderDragonPart`): where each stands along the body (`place`, Horizon's `place_dragon_parts`, D:2856)
//! and its box. The parts are data of the dragon, not entities; a hit names one by index (`hurt.rs`).
use super::data::*;
use super::flight::{FlightHistory, YawAccel};
use super::phases::PhaseId;
use crate::cow::{Pos, Yaw};
use bevy_ecs::prelude::*;
use horizon_core::geometry::{trig, Aabb, Vec3};

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
    let (w, h) = PART_SIZES[i];
    let p = parts.0[i].pos;
    Aabb::new(Vec3::new(p[0] - w / 2.0, p[1], p[2] - w / 2.0), Vec3::new(p[0] + w / 2.0, p[1] + h, p[2] + w / 2.0))
}

/// `Mth.wrapDegrees(float)` as Horizon writes it (D:1092).
pub fn wrap(d: f32) -> f32 {
    let mut r = d % 360.0;
    if r >= 180.0 {
        r -= 360.0;
    }
    if r < -180.0 {
        r += 360.0;
    }
    r
}

/// Places the parts of dragon `e` from its position, yaw, yaw acceleration and flight history (`EnderDragon.aiStep`, through
/// `tickPart`), step for step in the `f32`/`f64` mix of Horizon's `place_dragon_parts` (D:2856). Called by the flight wake only.
pub fn place(w: &mut World, e: Entity) {
    let Some(hist) = w.get::<FlightHistory>(e) else { return };
    let (s0, s5, s10) = (hist.sample(0), hist.sample(5), hist.sample(10));
    let tail = [hist.sample(12), hist.sample(14), hist.sample(16)];
    let sitting = w.get::<PhaseId>(e).is_some_and(|p| is_sitting(p.0));
    let (Some(p), Some(yaw)) = (w.get::<Pos>(e).map(|p| p.0), w.get::<Yaw>(e).map(|y| y.0)) else { return };
    let accel = w.get::<YawAccel>(e).map_or(0.0, |a| a.0);
    let (msin, mcos) = (|a: f32| trig::sin(f64::from(a)), |a: f32| trig::cos(f64::from(a)));
    let mut out = [[0.0_f64; 3]; 8];
    let mut set = |i: usize, x: f32, y: f64, z: f32| out[i] = [p.x + f64::from(x), p.y + y, p.z + f64::from(z)];

    let tilt = (s5.0 - s10.0) as f32 * 10.0 * trig::DEG_TO_RAD;
    let (ct, st) = (mcos(tilt), msin(tilt));
    let r1 = yaw * trig::DEG_TO_RAD;
    let (s1, c1) = (msin(r1), mcos(r1));
    set(2, s1 * 0.5, 0.0, -c1 * 0.5);
    set(6, c1 * 4.5, 2.0, s1 * 4.5);
    set(7, c1 * -4.5, 2.0, s1 * -4.5);

    let r2 = yaw * trig::DEG_TO_RAD - accel * 0.01;
    let (s2, c2) = (msin(r2), mcos(r2));
    let y_off = if sitting { -1.0 } else { s5.0 - s0.0 } as f32;
    set(0, s2 * 6.5 * ct, f64::from(y_off + st * 6.5), -c2 * 6.5 * ct);
    set(1, s2 * 5.5 * ct, f64::from(y_off + st * 5.5), -c2 * 5.5 * ct);

    for (i, p0) in tail.iter().enumerate() {
        let rot = yaw * trig::DEG_TO_RAD + wrap(p0.1 - s5.1) * trig::DEG_TO_RAD;
        let (ss, cc) = (msin(rot), mcos(rot));
        let dd = (i as f32 + 1.0) * 2.0;
        let x = -(s1 * 1.5 + ss * dd) * ct;
        let y = p0.0 - s5.0 - f64::from((dd + 1.5) * st) + 1.5;
        let z = (c1 * 1.5 + cc * dd) * ct;
        set(3 + i, x, y, z);
    }

    if let Some(mut parts) = w.get_mut::<Parts>(e) {
        for (part, pos) in parts.0.iter_mut().zip(out) {
            part.prev = part.pos;
            part.pos = pos;
        }
    }
}
