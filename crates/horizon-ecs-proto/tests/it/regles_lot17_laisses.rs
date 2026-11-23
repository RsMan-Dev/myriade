//! Lot 17a of the lead rules (the old `Mob` leash tests): the slack and stretched rope, the damping of a pull that moves with the
//! creature, the spin's friction, the knot's place on its post, the quad lead's points and pull, the boat's offsets, and the gates of
//! `canBeLeashed` (an aggravated nautilus). Vanilla 26.3 values (`Leashable`, `LeashFenceKnotEntity`, `AbstractBoat`, `Nautilus`).
use crate::common::*;
use myriade_engine_lib::mobs::sulfur_cube::{Archetype, ARCHETYPES};
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::behaviours::neutral_anger::Angry;
use horizon_ecs_proto::behaviours::swallow_hold::Held;
use horizon_ecs_proto::bridge::host::HostSetup;
use horizon_ecs_proto::boat_leash::{boat_leash_offset, boat_quad_leash_offsets};
use horizon_ecs_proto::leash::{self, Holder, Leash};
use horizon_ecs_proto::mount::ride;
use horizon_ecs_proto::targeting::Who;
use horizon_ecs_proto::rules::leash::{elastic_pull, knot_state, quad_pull, quad_points, spin_friction, spin_step, HolderState};

/// A standing holder on the line of x, its centre 0,9 above its feet, as the bench reads a player.
fn holder_at(x: f64) -> HolderState {
    HolderState {
        position: Vec3::new(x, 64.0, 0.0),
        center: Vec3::new(x, 64.9, 0.0),
        width: 0.6,
        height: 1.8,
        yaw: 0.0,
        velocity: Vec3::ZERO,
    }
}

fn setup() -> HostSetup {
    HostSetup { seed: 7, dimension: "overworld", simulation_radius: 4 }
}

#[test]
fn a_slack_rope_does_not_pull() {
    let pulled = elastic_pull(Vec3::new(0.0, 64.0, 0.0), 0.0, (0.9, 1.87), &holder_at(3.0), Vec3::ZERO, 6.0);
    assert!(pulled.is_none());
}

#[test]
fn a_stretched_rope_pulls_toward_the_holder() {
    let (force, _) = elastic_pull(Vec3::new(0.0, 64.0, 0.0), 0.0, (0.9, 1.87), &holder_at(10.0), Vec3::ZERO, 6.0).expect("past the slack");
    // The distance is about 10, so the pull is about 4 before the damping of a body that stands still: it points at the holder.
    assert!(force[0] > 0.0);
    assert!(force[0] < 10.0 - 6.0);
}

#[test]
fn a_pull_that_moves_with_the_body_is_damped() {
    let at = Vec3::new(0.0, 64.0, 0.0);
    let away = elastic_pull(at, 0.0, (0.9, 1.87), &holder_at(10.0), Vec3::new(-0.1, 0.0, 0.0), 6.0).expect("pull").0;
    let with = elastic_pull(at, 0.0, (0.9, 1.87), &holder_at(10.0), Vec3::new(0.1, 0.0, 0.0), 6.0).expect("pull").0;
    assert!((with[0] - away[0] * f64::from(0.3_f32)).abs() < 1e-9);
}

#[test]
fn a_leads_spin_wears_by_the_ground_friction_and_by_the_air() {
    // Blue ice is 0,989 under the creature; the air is 0,91 and the water 0,8.
    assert_eq!(spin_friction(Some(0.989_f32), false), 0.989_f32 * 0.91_f32);
    assert_eq!(spin_friction(None, false), 0.91_f32);
    assert_eq!(spin_friction(None, true), 0.8_f32);
    assert_eq!(spin_step(1.0, 0.91_f32), f64::from(0.91_f32));
    assert_eq!(spin_step(1.0e-10, 0.5_f32), 0.0);
}

#[test]
fn a_knot_sits_on_the_fence_post() {
    let k = knot_state([10, 64, -3]);
    assert_eq!(k.position.x, 10.5);
    assert_eq!(k.position.y, 64.375);
    assert_eq!(k.center.y, 64.625);
    assert_eq!((k.width, k.height), (0.375, 0.5));
}

#[test]
fn a_horse_quad_lead_hangs_its_four_corners_from_the_origin_table() {
    // `createQuadLeashOffsets` of a horse (1,5 by 1,6): front-left, back-left, back-right, front-right.
    let q = quad_points(1.5, 1.6, 0.04, 0.52, 0.23, 0.87);
    let (lr, y, fb, f) = (0.345, 1.392, 0.78, 0.06);
    let want = [[-lr, y, fb + f], [-lr, y, -fb + f], [lr, y, -fb + f], [lr, y, fb + f]];
    for (got, want) in q.iter().zip(want) {
        for k in 0..3 {
            assert!((got[k] - want[k]).abs() < 1e-6, "corner {got:?} against {want:?}");
        }
    }
}

#[test]
fn a_quad_lead_pulls_a_quarter_of_its_four_corners_on_each_end() {
    let at = Vec3::new(0.0, 64.0, 0.0);
    assert!(quad_pull(at, 0.0, (0.9, 1.87), &holder_at(3.0), Vec3::ZERO, 6.0).is_none(), "slack");
    let single = elastic_pull(at, 0.0, (0.9, 1.87), &holder_at(10.0), Vec3::ZERO, 6.0).expect("pull").0;
    let quad = quad_pull(at, 0.0, (0.9, 1.87), &holder_at(10.0), Vec3::ZERO, 6.0).expect("pull").0;
    assert!(quad[0] > 0.0);
    assert!(quad[0] <= single[0] * 1.5);
}

#[test]
fn a_boat_hangs_its_lead_at_the_front_of_its_box() {
    let one = boat_leash_offset();
    assert!((one[0]).abs() < 1e-12);
    assert!((one[1] - 0.88 * 0.5625).abs() < 1e-6);
    assert!((one[2] - 0.64 * 1.375).abs() < 1e-6);
    let q = boat_quad_leash_offsets();
    assert!(q[0][0] < 0.0 && q[0][2] > 0.0, "front-left");
    assert!(q[2][0] > 0.0 && q[2][2] < 0.0, "back-right");
    assert!((q[0][2] - 0.64 * 1.375).abs() < 1e-6);
    assert!((q[3][0] - 0.382 * 1.375).abs() < 1e-6);
}

#[test]
fn an_aggravated_nautilus_takes_no_lead_until_it_calms() {
    have_data();
    let (mw, mut h) = awake_host(&setup());
    let nautilus = h.spawn(mw, "minecraft:nautilus", [3.5, 64.0, 5.5], None).expect("a nautilus");
    h.world_mut().entity_mut(nautilus).insert(Angry);
    assert!(!leash::attach(h.world_mut(), nautilus, Holder::Knot([3, 64, 5])), "aggravated: refused");
    h.world_mut().entity_mut(nautilus).remove::<Angry>();
    assert!(leash::attach(h.world_mut(), nautilus, Holder::Knot([3, 64, 5])), "calm: accepted");
}

#[test]
fn a_horse_leashed_to_a_ghast_takes_the_quad_pull() {
    have_data();
    let (mw, mut h) = awake_host(&setup());
    let horse = h.spawn(mw, "minecraft:horse", [3.5, 64.0, 5.5], None).expect("a horse");
    let ghast = h.spawn(mw, "minecraft:ghast", [5.5, 64.0, 5.5], None).expect("a ghast");
    assert!(leash::attach(h.world_mut(), horse, Holder::Entity(ghast)), "the ghast holds the lead");
    assert!(h.world().get::<Leash>(horse).is_some_and(|l| l.quad), "both ends support a quad lead");
}

#[test]
fn only_horses_camels_and_sniffers_take_a_quad_lead() {
    have_data();
    let (mw, mut h) = awake_host(&setup());
    for (name, quad) in [("minecraft:horse", true), ("minecraft:camel", true), ("minecraft:sniffer", true), ("minecraft:llama", false), ("minecraft:cow", false)] {
        let e = h.spawn(mw, name, [0.5, 64.0, 0.5], None).expect(name);
        assert_eq!(h.world().get::<leash::QuadLead>(e).is_some(), quad, "{name}");
    }
}

#[test]
fn a_camel_husk_is_leashed_until_a_mob_rides_it() {
    have_data();
    let (mw, mut h) = awake_host(&setup());
    let husk = h.spawn(mw, "minecraft:camel_husk", [3.5, 64.0, 5.5], None).expect("a camel husk");
    assert!(ride(h.world_mut(), husk, Who::Player(1)), "a player rides it");
    assert!(leash::attach(h.world_mut(), husk, Holder::Knot([3, 64, 5])), "a player rider does not stop the lead");
    let other = h.spawn(mw, "minecraft:camel_husk", [6.5, 64.0, 5.5], None).expect("a camel husk");
    let zombie = h.spawn(mw, "minecraft:zombie", [7.5, 64.0, 5.5], None).expect("a zombie");
    assert!(ride(h.world_mut(), other, Who::Entity(zombie)), "a mob rides it");
    assert!(!leash::attach(h.world_mut(), other, Holder::Knot([6, 64, 5])), "a mob rides it: refused");
}

#[test]
fn a_wild_zombie_horse_with_a_mob_rider_takes_no_lead() {
    have_data();
    let (mw, mut h) = awake_host(&setup());
    let horse = h.spawn(mw, "minecraft:zombie_horse", [3.5, 64.0, 5.5], None).expect("a zombie horse");
    let zombie = h.spawn(mw, "minecraft:zombie", [4.5, 64.0, 5.5], None).expect("a zombie");
    assert!(leash::attach(h.world_mut(), horse, Holder::Knot([3, 64, 5])), "no rider: accepted");
    leash::detach(h.world_mut(), horse);
    assert!(ride(h.world_mut(), horse, Who::Entity(zombie)));
    assert!(!leash::attach(h.world_mut(), horse, Holder::Knot([3, 64, 5])), "a mob rides it: refused");
}

#[test]
fn a_sulfur_cube_takes_a_lead_only_while_it_holds_a_block() {
    have_data();
    let (mw, mut h) = awake_host(&setup());
    let cube = h.spawn(mw, "minecraft:sulfur_cube", [3.5, 64.0, 5.5], None).expect("a sulfur cube");
    assert!(!leash::attach(h.world_mut(), cube, Holder::Knot([3, 64, 5])), "empty: refused");
    let archetype: &'static Archetype = &ARCHETYPES[0];
    h.world_mut().entity_mut(cube).insert(Held { item: "minecraft:cobblestone".into(), archetype });
    assert!(leash::attach(h.world_mut(), cube, Holder::Knot([3, 64, 5])), "holding a block: accepted");
}

#[test]
fn a_cow_leashed_to_a_ghast_keeps_the_single_pull() {
    have_data();
    let (mw, mut h) = awake_host(&setup());
    let cow = h.spawn(mw, "minecraft:cow", [3.5, 64.0, 5.5], None).expect("a cow");
    let ghast = h.spawn(mw, "minecraft:ghast", [5.5, 64.0, 5.5], None).expect("a ghast");
    assert!(leash::attach(h.world_mut(), cow, Holder::Entity(ghast)));
    assert!(h.world().get::<Leash>(cow).is_some_and(|l| !l.quad), "a cow takes no quad lead");
}
