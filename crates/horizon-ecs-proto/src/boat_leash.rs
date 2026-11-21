//! The lead of a boat: `AbstractBoat implements Leashable`, so a boat takes a lead as a creature does, with the same `Leash` state (`leash.rs`).
//! What is specific to the boat is its box: a boat spawns with no `Bounds`, so one is set from its position when it is leashed.
//! The pull of the rope is `tick`, the leash's own pull (`elastic_pull`) on the boat's velocity and yaw. A slack lead does not walk a boat.
use crate::boat::bounds_at;
use crate::cow::{Bounds, Pos, Vel, Yaw};
use crate::goals::{impulse, Online};
use crate::leash::{self, Holder, LeashParams, Leash, Leashable};
use crate::rules::leash::{center_of, elastic_pull, is_stretched, knot_state, spin_friction, spin_step, HolderState};
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;

pub use crate::leash::{detach, holder_of};

/// Puts a lead from `holder` on boat `e`; false when `e` has no place, is already leashed, or the holder is out of reach.
pub fn attach(w: &mut World, e: Entity, holder: Holder) -> bool {
    let Some(at) = w.get::<Pos>(e).map(|p| p.0) else { return false };
    if w.get::<Leashable>(e).is_none() {
        let p = LeashParams::DEFAULT;
        w.entity_mut(e).insert(Leashable { elastic: p.elastic, snap: p.snap, follow_speed: p.follow_speed });
    }
    if w.get::<Bounds>(e).is_none() {
        w.entity_mut(e).insert(Bounds(bounds_at(at)));
    }
    leash::attach(w, e, holder)
}

/// The place of the holder for the pull; None when a player is not online.
fn holder_state(w: &World, h: Holder) -> Option<HolderState> {
    match h {
        Holder::Player(id) => w.get_resource::<Online>()?.get(id).map(|p| HolderState {
            position: Vec3::new(p.x, p.y, p.z),
            center: Vec3::new(p.x, p.y + 0.9, p.z),
            width: 0.6,
            height: 1.8,
            yaw: 0.0,
            velocity: Vec3::ZERO,
        }),
        Holder::Knot(p) => Some(knot_state(p)),
        Holder::Entity(h) => {
            let (p, b) = (w.get::<Pos>(h)?.0, w.get::<Bounds>(h)?.0);
            let (lo, hi) = (b.min_position(), b.max_position());
            Some(HolderState {
                position: p,
                center: center_of(b),
                width: hi.x - lo.x,
                height: hi.y - lo.y,
                yaw: w.get::<Yaw>(h).map_or(0.0, |y| y.0),
                velocity: w.get::<Vel>(h).map_or(Vec3::ZERO, |v| v.0),
            })
        }
    }
}

/// `Leashable.tickLeash` for boat `e`: the lead snaps beyond its snapping distance (the lead comes off), pulls the boat beyond its
/// elastic distance, and turns its yaw by the spin the pull gave. The holder gone (not a player) drops the lead.
pub fn tick(w: &mut World, e: Entity) {
    let Some(l) = w.get::<Leash>(e).copied() else { return };
    let Some(k) = w.get::<Leashable>(e).copied() else { return };
    let Some(state) = holder_state(w, l.holder) else {
        if !matches!(l.holder, Holder::Player(_)) {
            detach(w, e);
        }
        return;
    };
    let (Some(b), Some(p), Some(v), Some(y)) = (w.get::<Bounds>(e).map(|b| b.0), w.get::<Pos>(e).map(|p| p.0), w.get::<Vel>(e).map(|v| v.0), w.get::<Yaw>(e).map(|y| y.0)) else {
        return;
    };
    let mine = center_of(b);
    let distance = mine.distance(state.center);
    if distance > k.snap {
        detach(w, e);
        return;
    }
    let (lo, hi) = (b.min_position(), b.max_position());
    let (width, height) = (hi.x - lo.x, hi.y - lo.y);
    if is_stretched(distance, k.elastic, state.width, width as f32) {
        if let Some((force, torque)) = elastic_pull(p, y, (width, height), &state, v, k.elastic) {
            let rel = Vec3::new(state.velocity.x - v.x, state.velocity.y - v.y, state.velocity.z - v.z);
            let dv = Vec3::new(force[0] * 0.8 + rel.x * 0.11, force[1] * 0.2 + rel.y * 0.11, force[2] * 0.8 + rel.z * 0.11);
            impulse(w, e, dv);
            if let Some(mut l) = w.get_mut::<Leash>(e) {
                l.spin += 10.0 * torque;
            }
        }
    }
    let Some(mut l) = w.get_mut::<Leash>(e) else { return };
    if l.spin != 0.0 {
        let spin = l.spin;
        l.spin = spin_step(spin, spin_friction(None, false));
        drop(l);
        if let Some(mut yaw) = w.get_mut::<Yaw>(e) {
            yaw.0 = (f64::from(yaw.0) - spin) as f32;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cow::{Clock, Wheel};
    use crate::goals::{Events, Player};

    #[test]
    fn a_boat_takes_a_lead_and_the_holder_is_read_back() {
        let mut w = World::new();
        w.init_resource::<Clock>();
        w.init_resource::<Wheel>();
        w.init_resource::<Events>();
        w.init_resource::<Online>();
        w.resource_mut::<Online>().set(Player { id: 2, x: 4.0, y: 64.0, z: 4.0, main: 0, off: 0, head: 0, chest: 0, legs: 0, feet: 0 });
        let e = crate::boat_state::spawn(&mut w, 0, false, [4.0, 64.0, 4.0], 0.0);
        assert!(attach(&mut w, e, Holder::Player(2)));
        assert_eq!(holder_of(&w, e), Some(Holder::Player(2)));
        assert!(!attach(&mut w, e, Holder::Player(2)));
    }
}
