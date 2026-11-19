//! A bat (`HORIZON_SPECIES=minecraft:bat`) against a real `Mob` bat under a stone ceiling: the flutter, the hanging and the take-off, with the
//! spot and the wish to hang imposed by the `Mob` (the two generators differ). One bat per level. Also: a hanging bat costs nothing, and a
//! player coming within 4 blocks wakes it by event.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::behaviours::bat_flutter::{self, Flutter, Hanging};
use horizon_ecs_proto::{cow::*, goals::*};

fn bat() -> bool {
    species().name == "minecraft:bat"
}
/// Woken before the `Mob`'s step: it is steered in that tick.
fn wake_now(w: &mut World, e: Entity) {
    bat_flutter::take_off(w, e);
    w.entity_mut(e).remove::<bat_flutter::Waking>().insert(bat_flutter::Fluttering);
}
fn ceiling(r: &MobRef, h: &mut Herd) {
    for x in -19..=19 {
        for z in -19..=19 {
            r.set_block(x, 8, z, "minecraft:stone");
            h.set_block(x, 8, z, "minecraft:stone");
        }
    }
}

#[test]
fn a_bat_flutters_and_hangs_as_a_mob_does_when_its_decisions_are_imposed() {
    if !have_data() {
        return;
    }
    let _ = flat_pair();
    if !bat() {
        return;
    }
    let spots = [(-12.5, 5.5, -8.5), (3.5, 6.5, 7.5), (-4.5, 6.0, -3.5), (8.5, 5.5, 5.5), (0.5, 6.5, -7.5), (-10.5, 5.0, 4.5), (6.5, 6.5, -4.5), (-2.5, 5.5, -9.5)];
    let mut worst = [0.0_f64; 3];
    let (mut hangs, mut takeoffs, mut woken_by_block) = (0, 0, 0);
    for (k, &(x0, y0, z0)) in spots.iter().enumerate() {
        let (mut r, mut h) = flat_pair();
        ceiling(&r, &mut h);
        r.step(None);
        h.tick();
        let (i, e) = (r.add(Vec3::new(x0, y0, z0)), ecs_cow(&mut h, x0, y0, z0, 700 + k as u64));
        h.world.entity_mut(e).remove::<GoalSet>();
        let mut hung_since = 0;
        for t in 1..=1500_usize {
            let was_resting = r.mobs[i].extra.flag;
            // Both are woken after 10 ticks of hanging (a player coming by).
            if was_resting {
                hung_since += 1;
                if hung_since >= 10 {
                    r.mobs[i].extra.flag = false;
                    wake_now(&mut h.world, e);
                    takeoffs += 1;
                    hung_since = 0;
                }
            } else {
                hung_since = 0;
            }
            if t == 1 {
                r.mobs[i].extra.flag = false;
                wake_now(&mut h.world, e);
            }
            r.step(None);
            let m = &r.mobs[i];
            h.world.get_mut::<Flutter>(e).unwrap().0 = m.extra.positions.first().copied();
            if m.extra.flag && h.world.get::<Hanging>(e).is_none() {
                h.world.entity_mut(e).insert(bat_flutter::Perching);
                hangs += 1;
            }
            // The `Mob` also wakes when the block above is not a conductor any more (the bat drifted off the ceiling's edge): the ecs bat does
            // not watch the block (no block event yet), the test tells it.
            if !m.extra.flag && h.world.get::<Hanging>(e).is_some() {
                bat_flutter::take_off(&mut h.world, e);
                woken_by_block += 1;
            }
            h.tick();
            if k == 0 && t == 1 && std::env::var_os("TRACE4").is_some() {
                let em = h.world.entity(e);
                eprintln!("t1 vel {:?} flutter {:?} yaw {:?} contact {:?} ", em.get::<Vel>().unwrap().0, em.get::<Flutter>().unwrap().0, em.get::<Yaw>().unwrap().0, em.get::<Contact>().map(|c| c.on_ground));
                eprintln!("t1 fly {} moving {} fluttering {} hanging {} waking {} disabled {} walk {} modes {}", em.contains::<horizon_ecs_proto::locomotion::FlyMode>(), em.contains::<Moving>(), em.contains::<bat_flutter::Fluttering>(), em.contains::<Hanging>(), em.contains::<bat_flutter::Waking>(), em.contains::<bevy_ecs::entity_disabling::Disabled>(), em.contains::<horizon_ecs_proto::locomotion::WalkMode>(), em.contains::<horizon_ecs_proto::locomotion::Modes>());
            }
            if k == 3 && (31..=35).contains(&t) && std::env::var_os("TRACE").is_some() {
                eprintln!("t={t} MOB v {:?} target {:?} | ECS v {:?} target {:?} pos y {:.3} pos {:?} / {:?}", r.mobs[i].velocity, r.mobs[i].extra.positions.first(), h.world.get::<Vel>(e).unwrap().0, h.world.get::<Flutter>(e).unwrap().0, h.world.get::<Pos>(e).unwrap().0.y, r.mobs[i].position, h.world.get::<Pos>(e).unwrap().0);
            }
            let (a, b) = (r.mobs[i].position, h.world.get::<Pos>(e).unwrap().0);
            let d = ((a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2)).sqrt();
            if d > std::env::var("GAP").ok().and_then(|g| g.parse().ok()).unwrap_or(0.01) && std::env::var_os("TRACE2").is_some() {
                eprintln!("bat {k} t={t} gap {d:.3} MOB ({:.3},{:.3},{:.3}) rest {} | ECS ({:.3},{:.3},{:.3}) hanging {}", a.x, a.y, a.z, r.mobs[i].extra.flag, b.x, b.y, b.z, h.world.get::<Hanging>(e).is_some());
            }
            for (n, lim) in [20, 100, 1500].into_iter().enumerate() {
                if t <= lim && d > worst[n] {
                    worst[n] = d;
                }
            }
            if std::env::var_os("TRACE3").is_some() && k == 7 && t > 270 {
                eprintln!("t={t} mob rest {} y {:.3} was {was_resting} ecs hanging {} hung_since {hung_since}", r.mobs[i].extra.flag, r.mobs[i].position.y, h.world.get::<Hanging>(e).is_some());
            }
            assert_eq!(r.mobs[i].extra.flag, h.world.get::<Hanging>(e).is_some(), "bat {k} t={t}: the hanging differs");
        }
    }
    eprintln!("bat: worst gap to +20 {:.4}, +100 {:.4}, +1500 {:.4}; {hangs} hangs, {takeoffs} take-offs, {woken_by_block} wakes by the block", worst[0], worst[1], worst[2]);
    assert!(hangs >= 4, "the bats hardly hung");
    assert!(worst[0] < 0.1 && worst[2] < 0.3, "{worst:?}");
}

/// A hanging bat is in no query: the world ticks with a hundred of them at no cost beyond the frame's; a player coming within 4 blocks takes
/// off the bat by event and not another.
#[test]
fn a_hanging_bat_is_woken_by_a_player_coming_near() {
    if !have_data() {
        return;
    }
    let _ = flat_pair();
    if !bat() {
        return;
    }
    let (r, mut h) = flat_pair();
    ceiling(&r, &mut h);
    let near = ecs_cow(&mut h, 3.5, 7.1, 3.5, 5);
    let far = ecs_cow(&mut h, -14.5, 7.1, -14.5, 6);
    for e in [near, far] {
        bat_flutter::perch(&mut h.world, e);
    }
    for _ in 0..5 {
        h.tick();
    }
    assert!(h.world.get::<Hanging>(near).is_some() && h.world.get::<Hanging>(far).is_some(), "bats hang at birth");
    assert!(h.world.get::<Moving>(near).is_none(), "a hanging bat is not moving");
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 4.5, y: 10.0, z: 3.5, main: 0, off: 0 });
    for _ in 0..3 {
        h.tick();
    }
    assert!(h.world.get::<Hanging>(near).is_none(), "the near bat took off");
    assert!(h.world.get::<Hanging>(far).is_some(), "the far one stays");
}
