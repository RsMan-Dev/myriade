//! The drive of a mount as a parameter (`mount::Steer`) against the `Mob` in the same scene: the pig and its stick (`HORIZON_SPECIES=minecraft:pig`),
//! the camel and its dash (`minecraft:camel`), with the rider's inputs imposed; the second seat and the creature that steers its mount
//! (`REGISTER_ALL=1`, species of the table).
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_client_lib::mobs::equine::Rider;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::goals::*;
use horizon_ecs_proto::mount::*;
use horizon_ecs_proto::targeting::{Target, Who};

fn skip(name: &str) -> bool {
    !have_data() || std::env::var("HORIZON_SPECIES").as_deref() != Ok(name)
}
fn rider(forward: f32, yaw: f32, jump: f32, sprint: bool, steer: bool) -> Rider {
    Rider { strafe: 0.0, forward, yaw, pitch: 0.0, jump, steer, sprint, jumping: false, sneak: false, client: false }
}
fn gap(a: Vec3, b: Vec3) -> f64 {
    ((a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2)).sqrt()
}

/// Both sides, one mount, the rider's input written each tick by `f(t) = (forward, yaw, jump, sprint, steer)`; the worst gap in position, and
/// the speed of each side at the ticks of `show`.
fn run(ticks: u32, preroll: u32, boost: Option<(u32, u32)>, f: impl Fn(u32) -> (f32, f32, f32, bool, bool)) -> f64 {
    let mut r = MobRef::new(60, false);
    let mut h = Herd::with_slab(60, true, false);
    let i = r.add_id(Vec3::new(0.5, 1.0, 0.5), 31_337);
    let e = spawn_species(&mut h.world, species(), 0.5, 0.5, 5);
    h.world.resource_mut::<Entered>().0.push(e);
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5, y: 1.0, z: 0.5, main: 0, off: 0 });
    let view = player_view(Vec3::new(0.5, 1.0, 0.5), None);
    for _ in 0..preroll {
        r.step(Some(&view));
        h.tick();
    }
    // The ECS mount takes the place where the `Mob` is, so the two start together.
    let at = r.mobs[i].position;
    h.world.get_mut::<Pos>(e).unwrap().0 = at;
    r.mobs[i].extra.set_saddled(true);
    // The saddle goes in its slot; `Mount.saddled` follows.
    horizon_ecs_proto::equipment::set_slot(&mut h.world, e, horizon_ecs_proto::goals::Slot::Saddle, saddle());
    assert!(h.world.get::<Mount>(e).unwrap().saddled);
    assert!(ride(&mut h.world, e, Who::Player(1)));
    let mut worst = 0.0_f64;
    for t in 1..=ticks {
        if let Some((from, total)) = boost
            && t == from
        {
            r.mobs[i].extra.counter = 0;
            r.mobs[i].extra.counter2 = total as i32;
            h.world.entity_mut(e).insert(Boosted { elapsed: 0, total });
        }
        let (fw, yaw, jump, sprint, steer) = f(t);
        r.mobs[i].rider = Some(rider(fw, yaw, jump, sprint, steer));
        h.world.get_mut::<Mount>(e).unwrap().input = RiderInput { forward: fw, strafe: 0.0, yaw, pitch: 0.0, jump, sprint, steer };
        r.step(Some(&view));
        h.tick();
        let (m, p) = (r.mobs[i].position, h.world.get::<Pos>(e).unwrap().0);
        let d = gap(m, p);
        worst = worst.max(d);
        if t % 40 == 0 || (t <= 24 && std::env::var_os("TRACE").is_some()) {
            eprintln!("t{t}: v mob {:.4} ecs {:.4} | mob ({:.2},{:.2},{:.2}) ecs ({:.2},{:.2},{:.2}) gap {d:.3}", r.mobs[i].velocity.z, h.world.get::<Vel>(e).unwrap().0.z, m.x, m.y, m.z, p.x, p.y, p.z);
        }
    }
    worst
}

/// The pig that carries a rider holding the carrot on a stick: always forward along the look at 0,225 of its speed; the stick's boost
/// (here 160 ticks, imposed) multiplies it by `1 + 1,15 sin(pi t / total)`; a turn of 90 degrees at the middle.
#[test]
fn the_pig_obeys_the_stick_like_the_original() {
    if skip("minecraft:pig") {
        return;
    }
    let worst = run(260, 5, Some((20, 160)), |t| (0.0, if t < 130 { 0.0 } else { 90.0 }, 0.0, false, true));
    eprintln!("pig with the stick, 260 ticks, boost 20..180: worst gap {worst:.3}");
    assert!(worst < 0.3, "the pig does not go where the original goes: {worst}");
}

/// Without the stick in hand the saddled pig is not steered: the rider's look does not turn it and nothing pushes it forward.
#[test]
fn a_pig_without_the_stick_is_not_steered() {
    if skip("minecraft:pig") {
        return;
    }
    let mut h = Herd::with_slab(60, true, false);
    let e = spawn_species(&mut h.world, species(), 0.5, 0.5, 5);
    h.world.resource_mut::<Entered>().0.push(e);
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5, y: 1.0, z: 0.5, main: 0, off: 0 });
    h.world.get_mut::<Mount>(e).unwrap().saddled = true;
    assert!(ride(&mut h.world, e, Who::Player(1)));
    for _ in 0..40 {
        h.world.get_mut::<Mount>(e).unwrap().input = RiderInput { forward: 1.0, yaw: 137.0, steer: false, ..Default::default() };
        h.tick();
        assert!((h.world.get::<Yaw>(e).unwrap().0 - 137.0).abs() > 1.0, "the pig took the look of a rider without the stick");
    }
    // The stick makes it go, and a second use during the boost is refused.
    h.world.get_mut::<Mount>(e).unwrap().input = RiderInput { forward: 0.0, yaw: 137.0, steer: true, ..Default::default() };
    assert!(boost(&mut h.world, e));
    assert!(!boost(&mut h.world, e), "a boost runs already");
    h.tick();
    assert!((h.world.get::<Yaw>(e).unwrap().0 - 137.0).abs() < 1e-3);
    let total = h.world.get::<Boosted>(e).unwrap().total;
    assert!((140..981).contains(&total), "the boost lasts 140 to 980 ticks: {total}");
}

/// The camel: the jump is a dash (push, lift, 55 ticks of cooldown in which the press is thrown away), the sprint adds 0,1 to the speed
/// outside the cooldown; the dash at 10, a press at 40 (cooling down), at 70 (ready), sprint from 90, a turn at 100.
#[test]
fn the_camel_dashes_like_the_original() {
    if skip("minecraft:camel") {
        return;
    }
    let worst = run(170, 60, None, |t| (1.0, if t < 100 { 0.0 } else { 90.0 }, if matches!(t, 10 | 40 | 70) { 1.0 } else { 0.0 }, t >= 90, false));
    eprintln!("camel, 170 ticks: worst gap {worst:.3}");
    assert!(worst < 0.5, "the camel does not go where the original goes: {worst}");
}

/// A camel has two seats: the second player takes the second one and never steers; the first getting off hands over the steering.
#[test]
fn a_camel_carries_two_and_the_second_does_not_steer() {
    if skip("minecraft:camel") {
        return;
    }
    let mut h = Herd::with_slab(60, true, false);
    let e = spawn_species(&mut h.world, species(), 0.5, 0.5, 5);
    h.world.resource_mut::<Entered>().0.push(e);
    for id in 1..=3 {
        h.world.resource_mut::<Online>().set(Player { id, x: 0.5, y: 1.0, z: 0.5, main: 0, off: 0 });
    }
    assert!(ride(&mut h.world, e, Who::Player(1)));
    assert!(ride(&mut h.world, e, Who::Player(2)), "the second seat");
    assert!(!ride(&mut h.world, e, Who::Player(3)), "a camel carries two");
    for _ in 0..5 {
        h.tick();
    }
    let (a, b) = (h.world.resource::<Online>().get(1).unwrap(), h.world.resource::<Online>().get(2).unwrap());
    assert!(((a.x - b.x).powi(2) + (a.z - b.z).powi(2)).sqrt() > 1.0, "the seats are 1,2 blocks apart: {a:?} {b:?}");
    assert_eq!(dismount(&mut h.world, e), Some(Who::Player(1)));
    assert_eq!(rider_of(&h.world, e), Some(Who::Player(2)), "the passenger steers now");
    assert!(is_occupied(&h.world, e, OCC_MOUNTED));
    assert_eq!(dismount(&mut h.world, e), Some(Who::Player(2)));
    assert!(!is_occupied(&h.world, e, OCC_MOUNTED));
}

/// A creature rider steers its mount: it follows the seat (no fall, no walk of its own), its walks are the mount's, the mount walks to the
/// rider's target, and the mount's own movement goals are off while it is steered. A shooter keeps 8 blocks from its target.
#[test]
fn a_skeleton_steers_its_spider_to_its_target() {
    if std::env::var_os("REGISTER_ALL").is_none() || !have_data() || std::env::var("HORIZON_SPECIES").as_deref() != Ok("minecraft:spider") {
        return;
    }
    let mut h = Herd::with_slab(60, true, false);
    let table = species_table().0;
    let sk = *table.iter().find(|s| s.name == "minecraft:skeleton").expect("skeleton");
    let spider = spawn_species(&mut h.world, species(), 0.5, 0.5, 5);
    let skel = spawn_species(&mut h.world, &sk, 0.5, 0.5, 6);
    h.world.resource_mut::<Entered>().0.extend([spider, skel]);
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5, y: 1.0, z: 14.5, main: 0, off: 0 });
    for _ in 0..5 {
        h.tick();
    }
    assert!(ride(&mut h.world, spider, Who::Entity(skel)));
    assert!(is_occupied(&h.world, spider, OCC_MOUNTED));
    assert_eq!(nav_owner(&h.world, skel), spider, "the walks of the rider are the mount's");
    assert_eq!(nav_owner(&h.world, spider), spider);
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5, y: 1.0, z: 14.5, main: 0, off: 0 });
    horizon_ecs_proto::targeting::set_target(&mut h.world, skel, Target::new(Who::Player(1), 20, u8::MAX));
    let start = h.world.get::<Pos>(spider).unwrap().0;
    let mut worst_seat = 0.0_f64;
    for t in 0..300 {
        h.tick();
        let (sp, kp) = (h.world.get::<Pos>(spider).unwrap().0, h.world.get::<Pos>(skel).unwrap().0);
        worst_seat = worst_seat.max(((sp.x - kp.x).powi(2) + (sp.z - kp.z).powi(2)).sqrt());
        assert!(kp.y > sp.y, "the rider is above its mount at tick {t}");
    }
    let end = h.world.get::<Pos>(spider).unwrap().0;
    let left = ((end.x - 0.5).powi(2) + (end.z - 14.5).powi(2)).sqrt();
    eprintln!("spider from {start:?} to {end:?}: {left:.1} blocks from the player after 300 ticks; rider never farther than {worst_seat:.3} from the seat");
    assert!(worst_seat < 0.01, "the rider stays on the seat: {worst_seat}");
    assert!((7.0..=12.0).contains(&left), "a shooter closes in to about 8 blocks and keeps its distance: {left}");
    // Getting off: the rider walks again, the mount's goals are back.
    assert_eq!(dismount(&mut h.world, spider), Some(Who::Entity(skel)));
    assert_eq!(nav_owner(&h.world, skel), skel);
    h.tick();
}
