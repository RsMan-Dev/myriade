//! `Steer::Look` (the nautilus family): the input follows the look's pitch, the dash adds its push along the look.
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::mount::Steer;
use horizon_ecs_proto::rules::mount::{look_dash, look_input};

#[test]
fn level_look_goes_forward() {
    let (x, y, z) = look_input(0.3, 1.0, 0.0);
    assert!((x - 0.3).abs() < 1e-6 && y.abs() < 1e-6 && (z - 1.0).abs() < 1e-6);
}

#[test]
/// The jar's `getRiddenInput`: up is `-sin(pitch)` and forward `cos(pitch)`, both times -0,5 backwards (so backwards, looking down, goes up).
fn look_up_climbs_and_backwards_is_halved() {
    let (_, y, z) = look_input(0.0, 1.0, -90.0);
    assert!((y - 1.0).abs() < 1e-5 && z.abs() < 1e-5);
    let (_, y, z) = look_input(0.0, -1.0, 45.0);
    assert!((y - 0.5 * 0.707_106_8).abs() < 1e-5 && (z + 0.5 * 0.707_106_8).abs() < 1e-5);
}

#[test]
fn no_forward_key_no_pitch_input() {
    let (_, y, z) = look_input(0.0, 0.0, 45.0);
    assert_eq!((y, z), (0.0, 0.0));
}

#[test]
fn dash_goes_along_the_look() {
    let v = look_dash(Vec3::new(0.0, 0.0, 0.0), 0.0, 0.0, 1.2, 1.0, 1.0, 1.0);
    assert!((v.z - 1.2).abs() < 1e-6 && v.x.abs() < 1e-6 && v.y.abs() < 1e-6);
}

#[test]
fn held_jump_adds_half_up_to_the_ghast_input() {
    use horizon_ecs_proto::rules::mount::fly_input;
    let (_, still, _) = fly_input(0.0, 0.0, 0.0, false, 0.05);
    let (_, held, _) = fly_input(0.0, 0.0, 0.0, true, 0.05);
    assert_eq!(still, 0.0);
    assert!((held - 0.5 * 3.9 * 0.05).abs() < 1e-6, "{held}");
}

#[test]
fn pending_scale_is_applied_to_the_bar_once() {
    use horizon_ecs_proto::rules::mount::pending_scale;
    assert!((pending_scale(0) - 0.4).abs() < 1e-6 && (pending_scale(45) - 0.6).abs() < 1e-6 && pending_scale(95) == 1.0);
}

#[test]
fn nautilus_steer_is_look() {
    assert!(matches!(Steer::NAUTILUS, Steer::Look { cooldown: 40, .. }));
}
