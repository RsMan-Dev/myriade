//! The minecart's track physics on sampled facts, against the vanilla constants read in the jar (`OldMinecartBehavior`: the speeds 0.4 and
//! 0.2, the slide 0.0078125) and values worked by hand. Horizon's own tick lives in the game crate, not reachable from here: no
//! tick-by-tick oracle yet.
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::minecart::*;

fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-6, "{a} vs {b}");
}

/// A flat east-west rail at block (0, 64, 0).
fn flat_ew(powered: Option<bool>) -> Rail {
    Rail { exits: [[-1, 0, 0], [1, 0, 0]], powered }
}

#[test]
fn the_rail_line_lies_at_the_rail_top_under_the_cart() {
    let p = rail_point(&flat_ew(None), [0, 64, 0], Vec3::new(0.3, 64.0, 0.7));
    near(p.x, 0.3);
    near(p.y, 64.0625);
    near(p.z, 0.5);
}

#[test]
fn a_cart_on_a_flat_rail_moves_along_it_at_its_speed() {
    let mut pos = Vec3::new(0.3, 64.0, 0.5);
    let mut vel = Vec3::new(0.1, 0.0, 0.3);
    let d = along_track(Track::default(), &mut pos, &mut vel, [0, 64, 0], &flat_ew(None), [0.0, 0.0], false);
    // The velocity turns onto the line (x only), keeping its horizontal length sqrt(0.1^2 + 0.3^2).
    let speed = (0.1_f64 * 0.1 + 0.3 * 0.3).sqrt();
    near(vel.x, speed);
    near(vel.z, 0.0);
    near(d.x, speed);
    near(pos.z, 0.5);
}

#[test]
fn in_water_the_speed_is_held_to_0_2() {
    let mut pos = Vec3::new(0.3, 64.0, 0.5);
    let mut vel = Vec3::new(0.3, 0.0, 0.0);
    let t = Track { in_water: true, on_ground: false };
    let d = along_track(t, &mut pos, &mut vel, [0, 64, 0], &flat_ew(None), [0.0, 0.0], false);
    near(d.x, 0.2);
}

#[test]
fn a_rider_takes_three_quarters_of_the_speed() {
    let mut pos = Vec3::new(0.3, 64.0, 0.5);
    let mut vel = Vec3::new(0.3, 0.0, 0.0);
    let d = along_track(Track::default(), &mut pos, &mut vel, [0, 64, 0], &flat_ew(None), [0.0, 0.0], true);
    near(d.x, 0.225);
}

#[test]
fn an_unpowered_powered_rail_halves_the_speed_and_stops_a_slow_cart() {
    let mut pos = Vec3::new(0.3, 64.0, 0.5);
    let mut vel = Vec3::new(0.1, 0.0, 0.0);
    along_track(Track::default(), &mut pos, &mut vel, [0, 64, 0], &flat_ew(Some(false)), [0.0, 0.0], false);
    near(vel.x, 0.05);
    let mut slow = Vec3::new(0.02, 0.0, 0.0);
    along_track(Track::default(), &mut pos, &mut slow, [0, 64, 0], &flat_ew(Some(false)), [0.0, 0.0], false);
    near(slow.x, 0.0);
}

#[test]
fn a_rising_rail_slides_the_cart_back_by_the_jar_slide() {
    // Ascending east: the low exit is the west one (y -1), the high one the east (y 0).
    let rail = Rail { exits: [[-1, -1, 0], [1, 0, 0]], powered: None };
    let mut pos = Vec3::new(0.5, 65.0, 0.5);
    let mut vel = Vec3::ZERO;
    let d = along_track(Track::default(), &mut pos, &mut vel, [0, 64, 0], &rail, [0.0, 0.0], false);
    // The slide takes 0.0078125 per block of rise (the exits span 2 blocks): -2 * slide, then the projection keeps that speed going back.
    near(vel.x, -2.0 * SLIDE);
    near(d.x, -2.0 * SLIDE);
    near(pos.y, 65.0);
}

#[test]
fn the_plain_slowdown_is_0_96_and_riding_keeps_0_997() {
    let v = slowdown(Vec3::new(0.1, 0.5, 0.2), false, false);
    near(v.x, 0.096);
    near(v.y, 0.0);
    near(v.z, 0.2 * 0.96);
    let r = slowdown(Vec3::new(0.1, 0.0, 0.0), true, false);
    near(r.x, 0.1 * 0.997);
    let w = slowdown(Vec3::new(0.1, 0.0, 0.0), false, true);
    near(w.x, 0.1 * 0.96 * 0.95);
}

#[test]
fn a_chest_with_all_slots_empty_keeps_0_98_plus_the_comparator_term() {
    let v = container_slowdown(Vec3::new(0.1, 0.0, 0.0), 0, false, false);
    near(v.x, 0.1 * (0.98 + 15.0 * 0.001));
    let loot = container_slowdown(Vec3::new(0.1, 0.0, 0.0), 0, true, false);
    near(loot.x, 0.1 * 0.98);
}

#[test]
fn a_furnace_follows_its_push_at_0_8_and_halves_its_speed_on_land() {
    let (v, push) = furnace_slowdown(Vec3::new(0.1, 0.0, 0.0), Vec3::new(0.04, 0.0, 0.0), false);
    near(v.x, 0.1 * 0.8 + 0.04);
    near(push.x, 0.04);
    let (idle, _) = furnace_slowdown(Vec3::new(0.1, 0.0, 0.0), Vec3::ZERO, false);
    near(idle.x, 0.1 * 0.98);
    near(furnace_max_speed(0.4, false), 0.2);
    near(furnace_max_speed(0.2, true), 0.15);
}

#[test]
fn a_cart_at_rest_on_a_flat_rail_with_no_rider_and_no_push_may_sleep() {
    let rail = flat_ew(None);
    assert!(can_park(Some(&rail), Vec3::ZERO, false, Vec3::ZERO));
    assert!(!can_park(Some(&rail), Vec3::new(0.01, 0.0, 0.0), false, Vec3::ZERO));
    assert!(!can_park(Some(&rail), Vec3::ZERO, true, Vec3::ZERO), "a rider keeps it awake");
    assert!(!can_park(Some(&flat_ew(Some(true))), Vec3::ZERO, false, Vec3::ZERO), "a powered rail keeps it awake");
    assert!(!can_park(None, Vec3::ZERO, false, Vec3::ZERO), "off the rails it falls");
}

#[test]
fn off_track_speed_is_capped_and_halved_on_ground() {
    let v = off_track_velocity(Vec3::new(0.5, 0.2, -0.5), false, MAX_LAND);
    near(v.x, MAX_LAND);
    near(v.z, -MAX_LAND);
    near(v.y, 0.2);
    let g = off_track_velocity(Vec3::new(0.2, 0.0, 0.0), true, MAX_LAND);
    near(g.x, 0.1);
    near(off_track_drag(Vec3::new(1.0, 0.0, 0.0), false).x, 0.95);
    near(off_track_drag(Vec3::new(1.0, 0.0, 0.0), true).x, 1.0);
}

#[test]
fn fall_accumulates_outside_water_and_blasts_tnt_from_three_blocks() {
    let mut fall = 0.0;
    assert_eq!(fall_step(&mut fall, -1.0, false, false, false), None);
    near(fall, 1.0);
    assert_eq!(fall_step(&mut fall, -1.0, true, false, false), None);
    near(fall, 1.0);
    let blast = fall_step(&mut fall, -2.5, false, true, true).expect("3.5 blocks on a TNT cart");
    near(blast, 0.35 * 0.35);
    near(fall, 0.0);
    let mut short = 2.0;
    assert_eq!(fall_step(&mut short, 0.0, false, true, true), None, "under three blocks no blast");
}

#[test]
fn push_from_a_nearby_entity_is_at_most_a_twentieth() {
    let p = push_from(Vec3::new(2.0, 0.0, 0.0), Vec3::ZERO).expect("two blocks away");
    near(p[0], 0.025);
    near(p[1], 0.0);
    assert!(push_from(Vec3::new(0.001, 0.0, 0.0), Vec3::ZERO).is_none(), "on the centre no push");
}

#[test]
fn activators_by_genre() {
    assert_eq!(tnt_primed_by(true, -1), Some(FUSE));
    assert_eq!(tnt_primed_by(true, 5), None, "already primed");
    assert_eq!(tnt_primed_by(false, -1), None);
    assert!(hopper_enabled_on_activator(false));
    assert!(!hopper_enabled_on_activator(true));
    assert!(command_due(true, 10, 6));
    assert!(!command_due(true, 9, 6), "four ticks between activations");
    assert!(!command_due(false, 10, 0));
}

#[test]
fn tnt_fuse_counts_down_then_explodes() {
    assert_eq!(fuse_step(3), (2, false, true));
    assert_eq!(fuse_step(0), (0, true, false));
    assert_eq!(fuse_step(-1), (-1, false, false));
    assert!(ignites_by_collision(true, 0.01));
    assert!(!ignites_by_collision(false, 1.0));
    near(f64::from(blast_power(0.0, 0.5)), 4.0);
}

#[test]
fn container_signal_and_hopper_box() {
    assert_eq!(container_signal(&[1.0, 0.0], 2), 8);
    assert_eq!(container_signal(&[], 0), 0);
    let (min, max) = hopper_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 2.0, 1.0));
    near(min[0], -0.25);
    near(max[0], 1.25);
    near(max[1], 2.0);
}

#[test]
fn seven_genres_each_with_its_kind() {
    assert_eq!(GENRES.len(), 7);
    assert_eq!(GENRES[4].1, Kind::Tnt);
}
