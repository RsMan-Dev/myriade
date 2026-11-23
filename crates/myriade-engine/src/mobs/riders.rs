//! Mobs riding mobs (`Entity.getPassengers`): where a rider sits on its
//! vehicle (`EntityAttachments`), the rider steering the vehicle toward its
//! target (`Mob.getControllingPassenger`, whose navigation and move control
//! the rider's goals use), and the saved jockey fields. The game links the
//! two (`game/entities/passengers.rs`) and spawns the jockeys.
use super::{
    MobWorld, Seen, Species,
    goals::{JUMP, LOOK, MOVE, TARGET},
};
use crate::join_seq::JoinList;
use horizon_core::geometry::{Aabb, Vec3};
use horizon_core::nbt::{Compound, Tag};

/// The goal flags a controlling passenger takes from its vehicle.
const CONTROLLED: u8 = MOVE | JUMP | LOOK;
/// `Entity.removePassenger`: a passenger that got off cannot board for 60 ticks.
const BOARDING_COOLDOWN: i32 = 60;
/// `Piglin.startRiding`: how many baby piglins stack on one hoglin.
const MAX_PASSENGERS_ON_ONE_HOGLIN: i32 = 3;

pub(crate) fn parse_passenger_attachments(value: &serde_json::Value) -> Vec<[f32; 3]> {
    value.as_array().map_or_else(Vec::new, |a| {
        a.iter()
            .filter_map(|p| match p {
                serde_json::Value::Array(c) if c.len() == 3 => Some([
                    c[0].as_f64()? as f32,
                    c[1].as_f64()? as f32,
                    c[2].as_f64()? as f32,
                ]),
                serde_json::Value::Number(y) => Some([0.0, y.as_f64()? as f32, 0.0]),
                _ => None,
            })
            .collect()
    })
}
/// An `EntityType` attachment point from `passengerAttachments(float... offsetYs)`:
/// the builder takes floats, so the double the position math sees is the
/// float's widening (`0.86875F` is `0.8687499761581421`).
fn up(y: f32) -> [f64; 3] {
    [0.0, f64::from(y), 0.0]
}
/// An attachment point the type attaches from `Vec3` doubles
/// (`passengerAttachments(new Vec3(0.0, 1.37, -0.3))` keeps them).
fn pt(x: f64, y: f64, z: f64) -> [f64; 3] {
    [x, y, z]
}
/// An attachment point from the builder's float form
/// (`attach(PASSENGER, 0.0F, 0.81F, -0.15F)`), widened like `up`.
fn pt32(x: f32, y: f32, z: f32) -> [f64; 3] {
    [f64::from(x), f64::from(y), f64::from(z)]
}
/// `EntityType.HAPPY_GHAST` `passengerAttachments`: four seats at the corners
/// of the back (front, left, back, right), picked by the rider's place in
/// `getPassengers` (`EntityAttachments.get` clamps the index to the last).
pub fn happy_ghast_attachment(index: usize) -> [f64; 3] {
    const SEATS: [[f64; 3]; 4] = [
        [0.0, 4.0, 1.7],
        [-1.7, 4.0, 0.0],
        [0.0, 4.0, -1.7],
        [1.7, 4.0, 0.0],
    ];
    let [x, y, z] = SEATS[index.min(SEATS.len() - 1)];
    pt(x, y, z)
}
/// `Vec3.yRot(-yaw)` of an attachment point.
fn rotated([x, y, z]: [f64; 3], yaw: f32) -> [f64; 3] {
    let a = -f64::from(yaw).to_radians();
    let (sin, cos) = (a.sin(), a.cos());
    [x * cos + z * sin, y, z * cos - x * sin]
}
/// `Entity.positionRider`: the vehicle's position plus its rotated passenger
/// attachment, less the rider's own vehicle attachment.
pub fn seat(attachment: [f64; 3], rider_offset: f64, at: Vec3, yaw: f32) -> Vec3 {
    let [x, y, z] = rotated(attachment, yaw);
    Vec3::new(at.x + x, at.y + y - rider_offset, at.z + z)
}



/// Ranged riders keep this far from their target instead of closing in.
const RANGED_DISTANCE: f64 = 8.0;
fn ranged(s: Species) -> bool {
    matches!(
        s,
        Species::Skeleton
            | Species::Stray
            | Species::Bogged
            | Species::Parched
            | Species::Pillager
            | Species::Witch
            | Species::Piglin
    )
}
/// `LivingEntityRenderer.solveBodyRot` for the rider of a living vehicle:
/// the body drawn from the vehicle's, at most 85 degrees off the head, and
/// past 50 degrees turned a fifth of the way back toward it.
pub fn solve_body_rot(head: f32, vehicle_body: f32) -> f32 {
    let diff = super::living::wrap(head - vehicle_body).clamp(-85.0, 85.0);
    let mut body = head - diff;
    if diff.abs() > 50.0 {
        body += diff * 0.2;
    }
    body
}











#[cfg(test)]
mod tests {
    use super::*;


    // Lower-layer loan test, not a native GoalContext integration test.
    #[test]
    #[ignore = "daily"]
    fn retained_controller_disables_goals_without_becoming_a_target() {
        let mut vehicle = of(71, Species::Spider);
        let mut rider = of(72, Species::Skeleton);
        rider.extra.mount = Some(vehicle.id);
        vehicle.extra.rider = Some(rider.id);
        let retained = [rider.seen(None, &crate::items::Items::empty())];
        let root = std::env::var_os("HORIZON_TEST_RESOURCES").expect("prepared resources required");
        assert!(crate::mobs::world::with_resource_test_world(
            std::path::Path::new(&root),
            &[],
            true,
            true,
            |_, _| {},
            |w, _| {
                let w = MobWorld {
                    passenger_objects: &retained,
                    ..w.clone()
                };
                drive(&mut vehicle, &w);
                assert_eq!(vehicle.ai.goals.disabled & CONTROLLED, CONTROLLED);
                assert!(w.seen(rider.id).is_none());
                assert!(w.target_info(super::super::Who::Mob(rider.id)).is_none());
                assert_eq!(w.others_near(vehicle.bounds()).count(), 0);
                vehicle.extra.rider = None;
                drive(&mut vehicle, &w);
                assert_eq!(vehicle.ai.goals.disabled & CONTROLLED, 0);
                vehicle.extra.rider = Some(rider.id);
                vehicle.no_ai = true;
                drive(&mut vehicle, &w);
                assert_eq!(vehicle.ai.goals.disabled & CONTROLLED, 0);
            }
        ));
    }


    #[test]
    fn attachments_turn_with_the_vehicle() {
        // A ravager's seat sits a sixteenth behind its centre.
        let behind = [0.0, 2.2625, -0.0625];
        let [x, y, z] = rotated(behind, 0.0);
        assert!(x.abs() < 1e-9 && (z + 0.0625).abs() < 1e-9 && y == 2.2625);
        // Facing west (yaw 90), behind is to the east.
        let [x, _, z] = rotated(behind, 90.0);
        assert!((x - 0.0625).abs() < 1e-9 && z.abs() < 1e-9);
    }
    /// 26.3 bytecode (`javap -c`): `AbstractHorse.positionRider` and
    /// `Chicken.positionRider` set every living rider's `yBodyRot` to the
    /// vehicle's; `AbstractSkeleton.rideTick` and `Drowned.rideTick` take it
    /// from a `PathfinderMob` they control; no other vehicle or rider does.
    #[test]
    fn only_horses_chickens_and_steering_skeletons_take_the_vehicles_body() {
        let takes = |vehicle, rider, controlling| {
            takes_vehicle_body(&of(1, vehicle), &of(2, rider), controlling)
        };
        for vehicle in [
            Species::Horse,
            Species::SkeletonHorse,
            Species::Camel,
            Species::Llama,
            Species::Chicken,
        ] {
            assert!(takes(vehicle, Species::Zombie, false), "{vehicle:?}");
        }
        assert!(takes(Species::Spider, Species::Skeleton, true));
        assert!(takes(Species::Spider, Species::Drowned, true));
        assert!(!takes(Species::Spider, Species::Skeleton, false));
        // Not a PathfinderMob.
        assert!(!takes(Species::Ghast, Species::Skeleton, true));
        // Their own BodyRotationControl turns these.
        assert!(!takes(Species::Pig, Species::Zombie, true));
        assert!(!takes(Species::Ravager, Species::Pillager, true));
        assert!(!takes(Species::Hoglin, Species::Piglin, true));
        assert!(!takes(Species::Strider, Species::ZombifiedPiglin, true));
    }
    /// `Mob.getControllingPassenger` (26.3 bytecode): the first passenger,
    /// and only while no player is aboard, the vehicle has AI and the rider
    /// can steer.
    #[test]
    fn a_skeleton_steers_its_spider_only_as_its_first_passenger() {
        let mut spider = of(1, Species::Spider);
        let skeleton = of(2, Species::Skeleton);
        spider.extra.rider = Some(2);
        assert!(controls_vehicle(&spider, &skeleton, false, false));
        assert!(!controls_vehicle(&spider, &skeleton, true, false));
        assert!(!controls_vehicle(&spider, &skeleton, false, true));
        spider.extra.rider = Some(3);
        assert!(!controls_vehicle(&spider, &skeleton, false, false));
        spider.extra.rider = Some(2);
        spider.no_ai = true;
        assert!(!controls_vehicle(&spider, &skeleton, false, false));
    }
    /// `LivingEntityRenderer.extractRenderState` draws a rider of a living
    /// vehicle from `solveBodyRot` (the vehicle's body, interpolated with
    /// `rotLerp`), and anything else from its own body.
    #[test]
    fn a_rider_of_a_living_vehicle_is_drawn_from_the_vehicles_body() {
        let mut m = of(2, Species::Pillager);
        m.body_yaw = 0.0;
        m.previous_body_yaw = 0.0;
        m.head_yaw = 100.0;
        m.previous_head_yaw = 100.0;
        m.extra.mount = Some(7);
        m.extra.vehicle_body = Some((7, 30.0, 50.0));
        let close = |a: f32, b: f32| (a - b).abs() < 1e-4;
        // Halfway, the vehicle's body is at 40: the head is 60 off, so the
        // body is 85-clamped to 40 and turned a fifth of 60 toward the head.
        assert!(close(super::super::anim::body_rot(&m, 0.5), 52.0));
        assert!(close(super::super::anim::state(&m, 0.5).y_rot, 48.0));
        // Off that vehicle (or onto a boat), its own body again.
        m.extra.mount = Some(8);
        assert!(close(super::super::anim::body_rot(&m, 0.5), 0.0));
        m.extra.mount = None;
        assert!(close(super::super::anim::body_rot(&m, 0.5), 0.0));
    }
    /// An NPC plugin's enlarged fake player: the name it carries sits on the
    /// top of its scaled box, and on a seat it hangs `Avatar`'s 0.6 lower, scaled too.
    #[test]
    fn a_scaled_player_carries_and_sits_in_its_scale() {
        let close = |a: f64, b: f64| (a - b).abs() < 1e-6;
        assert!(close(passenger_attachment(&npc(0.0))[1], 1.8));
        assert!(close(passenger_attachment(&npc(1.5))[1], 2.7));
        assert!(close(vehicle_attachment(&npc(0.0), 1.375), 0.6));
        assert!(close(vehicle_attachment(&npc(1.5), 1.375), 0.9));
        // On a seat at y 55 (a text display, whose attachment is its feet):
        // a 1.5 NPC's feet at 54.1, where the server puts it.
        let seat = seat(
            [0.0; 3],
            vehicle_attachment(&npc(1.5), 1.375),
            Vec3::new(0.0, 55.0, 0.0),
            0.0,
        );
        assert!(close(seat.y, 54.1));
    }
    /// The attachments are `EntityTypes` floats widened to double, and a
    /// zombie's seat (2.0125) is above its box (1.95).
    #[test]
    fn seats_are_the_entity_type_floats() {
        let close = |a: f64, b: f64| (a - b).abs() < 1e-12;
        assert!(close(up(0.86875)[1], 0.868_749_976_158_142_1));
        assert!(close(f64::from(0.7_f32), 0.699_999_988_079_071));
        assert!(close(
            up(2.0125)[1] - f64::from(0.7_f32),
            1.312_500_059_604_645
        ));
    }

    #[test]
    fn archers_keep_their_distance_and_brawlers_close_in() {
        assert!(ranged(Species::Skeleton) && ranged(Species::Pillager));
        assert!(!ranged(Species::Zombie) && !ranged(Species::Vindicator));
    }

}

#[cfg(test)]
mod happy_ghast_seat_tests {
    use super::happy_ghast_attachment;

    #[test]
    fn passengers_take_separate_seats() {
        assert_eq!(happy_ghast_attachment(0), [0.0, 4.0, 1.7]);
        let seats: Vec<_> = (0..4).map(happy_ghast_attachment).collect();
        for (i, a) in seats.iter().enumerate() {
            assert!(seats[i + 1..].iter().all(|b| a != b));
        }
        assert_eq!(happy_ghast_attachment(9), seats[3]);
    }

}
