//! The boats at the bridge: their apparition by wood name, the wood a boat has, and the boat a ray enters first (by the spatial index).
//! The state and the NBT are `boat_state`'s.
use crate::bridge::host::EcsWorld;
use crate::bridge::view;
use crate::boat_state::{self, BoatMarker};
use crate::boat_tick::{OutOfControl, TIME_TO_EJECT};
use crate::cow::{enter_index, now_of, Bounds, Cell, Index, Pos, Yaw};
use crate::rules::boat::bounds_at;
use crate::rules::seat_rules::{capacity, seat_height, seat_offset, seat_point, SeatKind};
use crate::species::boats::index;
use crate::zone;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use horizon_client_lib::mobs::MobWorld;
use horizon_core::geometry::Vec3;
use std::collections::HashSet;

/// The step of the ray through the index, as the carts' aim (`cart_more.rs`).
const STEP: f64 = 0.5;

impl EcsWorld {
    /// A boat of `wood` (`oak`, `bamboo`...), with a chest when `chest`, at `at`, facing `yaw`. None when `wood` is no wood of `WOODS`.
    /// The boat is in the spatial index from its apparition.
    pub fn spawn_boat(&mut self, mw: &MobWorld<'_>, wood: &str, chest: bool, at: [f64; 3], yaw: f32) -> Option<Entity> {
        let wood = index(wood)?;
        Some(self.lent(mw, |w| {
            let e = boat_state::spawn(w, wood, chest, at, yaw);
            w.entity_mut(e).insert((Bounds(bounds_at(Vec3::new(at[0], at[1], at[2]))), Cell(u32::MAX, [i32::MIN; 8], u32::MAX)));
            enter_index(w, e);
            zone::admit(w, &[e]);
            view::mark(w, e);
            e
        }))
    }

    /// The wood name and chest flag of boat `e`, None when `e` is no boat.
    pub fn boat_wood(&self, e: Entity) -> Option<(&'static str, bool)> {
        boat_state::wood_of(self.world(), e)
    }

    /// The boat the ray from `eye` along `dir` (a unit vector) enters first within `reach`, with the distance to its entry face; `skip` (the
    /// boat the aimer rides) is left out. The candidates come from the index along the ray, each clipped exactly against its box.
    pub fn boat_pick(&self, eye: [f64; 3], dir: [f64; 3], reach: f64, skip: Option<Entity>) -> Option<(Entity, f64)> {
        aimed_boat(self.world(), eye, dir, reach, skip)
    }

    /// Whether boat `e` is out of control: `AbstractBoat.outOfControlTicks` has reached `TIME_TO_EJECT` (60) under water in a row.
    pub fn boat_out_of_control(&self, e: Entity) -> bool {
        let w = self.world();
        w.get::<OutOfControl>(e).is_some_and(|o| now_of(w, e).saturating_sub(o.0) >= TIME_TO_EJECT)
    }

    /// The boats whose position lies in `min..=max`, by the spatial index; empty without the index.
    pub fn boats_in(&self, min: Vec3, max: Vec3) -> Vec<Entity> {
        let w = self.world();
        let Some(index) = w.get_resource::<Index>() else { return Vec::new() };
        let (cx, cz) = ((min.x + max.x) / 2.0, (min.z + max.z) / 2.0);
        let reach = (max.x - min.x).hypot(max.z - min.z) / 2.0;
        let mut out = Vec::new();
        index.for_radius(cx, cz, reach, |e| {
            let inside = |p: Vec3| p.x >= min.x && p.x <= max.x && p.y >= min.y && p.y <= max.y && p.z >= min.z && p.z <= max.z;
            if w.get::<BoatMarker>(e).is_some() && w.get::<Pos>(e).is_some_and(|p| inside(p.0)) {
                out.push(e);
            }
        });
        out
    }

    /// The position of the seat `slot` of boat `e` among `count` riders (`seat_rules`: the boat's seat offset and ride height), None when
    /// `e` is no boat or `slot` is not among `count` riders the boat takes.
    pub fn boat_seat(&self, e: Entity, slot: usize, count: usize) -> Option<Vec3> {
        let w = self.world();
        let k = w.get::<crate::boat::Kind>(e)?;
        let kind = SeatKind::Boat { chest: k.chest, raft: k.raft };
        if slot >= count || count > capacity(kind) {
            return None;
        }
        let (pos, yaw) = (w.get::<Pos>(e)?.0, w.get::<Yaw>(e)?.0);
        Some(seat_point(pos, yaw, seat_offset(kind, slot, count), seat_height(kind)))
    }

    /// The damage of boat `e` (`Hurt`'s, 0 at rest); None when `e` is no boat.
    pub fn boat_damage(&self, e: Entity) -> Option<f32> {
        let w = self.world();
        w.get::<BoatMarker>(e)?;
        Some(w.get::<boat_state::Hurt>(e).map_or(0.0, |h| h.damage))
    }

    /// The `hurtTime` of boat `e` (its shake's ticks left, 0 at rest); None when `e` is no boat.
    pub fn boat_hurt_time(&self, e: Entity) -> Option<i32> {
        let w = self.world();
        w.get::<BoatMarker>(e)?;
        Some(w.get::<boat_state::Hurt>(e).map_or(0, |h| h.time))
    }

    /// The riders boat `e` takes (`seat_rules::capacity`); None when `e` is no boat.
    pub fn boat_max_passengers(&self, e: Entity) -> Option<usize> {
        let k = self.world().get::<crate::boat::Kind>(e)?;
        Some(capacity(SeatKind::Boat { chest: k.chest, raft: k.raft }))
    }
}

/// `boat_pick` on the world: the boats (`BoatMarker`) the index yields along the ray, the nearest entry.
fn aimed_boat(w: &World, eye: [f64; 3], dir: [f64; 3], reach: f64, skip: Option<Entity>) -> Option<(Entity, f64)> {
    let index = w.get_resource::<Index>()?;
    let (eye_v, end) = (Vec3::new(eye[0], eye[1], eye[2]), Vec3::new(eye[0] + dir[0] * reach, eye[1] + dir[1] * reach, eye[2] + dir[2] * reach));
    let mut seen = HashSet::new();
    let mut best: Option<(Entity, f64)> = None;
    for k in 0..=(reach / STEP) as u32 {
        let t = f64::from(k) * STEP;
        index.for_near(eye[0] + dir[0] * t, eye[2] + dir[2] * t, |e| {
            if skip == Some(e) || !seen.insert(e) || w.get::<BoatMarker>(e).is_none() {
                return;
            }
            let Some(b) = w.get::<Bounds>(e) else { return };
            if let Some(hit) = b.0.clip(eye_v, end) {
                let d = ((hit.x - eye[0]).powi(2) + (hit.y - eye[1]).powi(2) + (hit.z - eye[2]).powi(2)).sqrt();
                if best.map_or(true, |(_, bt)| d < bt) {
                    best = Some((e, d));
                }
            }
        });
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> World {
        let mut w = World::new();
        w.insert_resource(Index::new(-32.0, 32));
        w
    }

    /// A still oak boat at `(x, 64, z)`, in the index as `spawn_boat` puts it.
    fn boat_at(w: &mut World, x: f64, z: f64) -> Entity {
        let at = [x, 64.0, z];
        let e = boat_state::spawn(w, 0, false, at, 0.0);
        w.entity_mut(e).insert((Bounds(bounds_at(Vec3::new(x, 64.0, z))), Cell(u32::MAX, [i32::MIN; 8], u32::MAX)));
        enter_index(w, e);
        e
    }

    #[test]
    fn a_ray_meets_the_box_at_its_edges_and_not_past_them() {
        let mut w = world();
        let e = boat_at(&mut w, 0.0, 0.0);
        let b = bounds_at(Vec3::new(0.0, 64.0, 0.0));
        let (lo, hi) = (b.min_position(), b.max_position());
        let mid_z = (lo.z + hi.z) / 2.0;
        let eye = [lo.x - 5.0, 64.5, mid_z];
        let (hit, d) = aimed_boat(&w, eye, [1.0, 0.0, 0.0], 10.0, None).expect("the boat is in the way");
        assert_eq!(hit, e);
        assert!((d - 5.0).abs() < 1.0e-9, "entry {d}");
        let inside_edge = [lo.x - 5.0, 64.5, hi.z - 0.01];
        assert!(aimed_boat(&w, inside_edge, [1.0, 0.0, 0.0], 10.0, None).is_some());
        let past_edge = [lo.x - 5.0, 64.5, hi.z + 0.01];
        assert!(aimed_boat(&w, past_edge, [1.0, 0.0, 0.0], 10.0, None).is_none());
        assert!(aimed_boat(&w, eye, [1.0, 0.0, 0.0], 4.9, None).is_none(), "out of reach");
        assert!(aimed_boat(&w, eye, [1.0, 0.0, 0.0], 10.0, Some(e)).is_none(), "the skipped boat");
    }
}
