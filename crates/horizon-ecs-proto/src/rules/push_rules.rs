//! The push of the entities by a vehicle, as values: `Entity.push(Entity)` (the impulse one entity gets from another) and `AbstractBoat.push`
//! (which entities a boat pushes). Read on the 26.3 server jar: the 0.01 threshold and the 0.05 scale of `Entity.push`. The cart's push of a
//! creature is `minecart::push_from`; the state and the index are in `vehicle_push.rs`.
use horizon_core::geometry::{Aabb, Vec3};

/// The reach a vehicle's box gets on x and z before the entities it touches are looked at (`AbstractBoat.tick`: `inflate(0.2, -0.01, 0.2)`).
pub const TOUCH_REACH: f64 = 0.2;

/// `Entity.push(Entity)`: the impulse `other` gets from `this` (the opposite one goes to `this`), None when the two stand within 0.01 of
/// each other on their largest horizontal offset.
pub fn push_impulse(this: Vec3, other: Vec3) -> Option<Vec3> {
    let (mut xa, mut za) = (other.x - this.x, other.z - this.z);
    let mut dd = xa.abs().max(za.abs());
    if dd < f64::from(0.01_f32) {
        return None;
    }
    dd = dd.sqrt();
    xa /= dd;
    za /= dd;
    let pow = (1.0 / dd).min(1.0);
    let k = f64::from(0.05_f32);
    Some(Vec3::new(xa * pow * k, 0.0, za * pow * k))
}

/// `AbstractBoat.push`: another boat is pushed while its feet are below this boat's top, anything else while its feet are no higher than
/// the boat's feet.
pub fn boat_pushes(boat: Aabb, other: Aabb, other_is_boat: bool) -> bool {
    if other_is_boat {
        other.min_position().y < boat.max_position().y
    } else {
        other.min_position().y <= boat.min_position().y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_push_of_two_units_is_the_full_scale_along_its_axis() {
        let v = push_impulse(Vec3::new(0.0, 0.0, 0.0), Vec3::new(2.0, 0.0, 0.0)).unwrap();
        assert!((v.x - 0.05).abs() < 1e-7 && v.z.abs() < 1e-12);
        let back = push_impulse(Vec3::new(2.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 0.0)).unwrap();
        assert!((back.x + 0.05).abs() < 1e-7);
    }

    #[test]
    fn entities_on_the_same_spot_are_not_pushed() {
        assert!(push_impulse(Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.005, 0.0, 0.0)).is_none());
    }

    #[test]
    fn a_boat_pushes_a_boat_below_its_top_and_anything_at_its_feet_or_below() {
        let boat = Aabb::new(Vec3::new(-1.0, 0.0, -1.0), Vec3::new(1.0, 1.0, 1.0));
        let at = |y: f64| Aabb::new(Vec3::new(-0.5, y, -0.5), Vec3::new(0.5, y + 1.8, 0.5));
        assert!(boat_pushes(boat, at(0.5), true));
        assert!(!boat_pushes(boat, at(1.5), true));
        assert!(boat_pushes(boat, at(0.0), false));
        assert!(!boat_pushes(boat, at(0.2), false));
    }
}
