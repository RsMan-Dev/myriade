//! The status effects capability against the `Mob`'s own effects (`tick_effects`) in the same scene (`HORIZON_TEST_RESOURCES` set): the
//! health of a cow tick by tick under poison, wither and regeneration (the periods by level), the speed factors of speed and slowness, and the
//! height of a levitating cow.
use crate::common;
use common::*;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{cow::*, effects::{self, Kind}};

/// The scenes are written for a farm animal of 10 hearts or more that is not undead (poison and regeneration do not touch the undead: not modelled).
fn skip() -> bool {
    !have_data() || wrong_species(&["minecraft:cow", "minecraft:pig", "minecraft:sheep"])
}
fn timelines(mob_id: &'static str, kind: Kind, amp: i32, ticks: i32, wound: f32, run: usize) -> (Vec<f32>, Vec<f32>) {
    let (mut r, mut h) = flat_pair();
    let i = r.add(Vec3::new(0.5, 1.0, 0.5));
    let e = ecs_cow(&mut h, 0.5, 1.0, 0.5, 5);
    for _ in 0..3 {
        r.step(None);
        h.tick();
    }
    if wound > 0.0 {
        r.hit(i, wound, [0.5, 1.0, 5.5]);
        h.hit(e, wound, (0.5, 5.5));
        // The window of invulnerability and the knockback are over before the effect starts.
        for _ in 0..20 {
            r.step(None);
            h.tick();
        }
    }
    r.mobs[i].add_effect(mob_id, ticks, amp);
    assert!(effects::add(&mut h.world, e, kind, amp, ticks));
    let (mut mv, mut ev) = (Vec::new(), Vec::new());
    for _ in 0..run {
        r.step(None);
        h.tick();
        mv.push(mob_health(&r.mobs[i]));
        ev.push(health(&h, e));
    }
    (mv, ev)
}
fn same(name: &str, (m, e): (Vec<f32>, Vec<f32>)) {
    let first = m.iter().zip(&e).position(|(a, b)| a != b);
    eprintln!("{name}: health {} -> {} (mob), {} -> {} (ecs), first difference at {first:?}", m[0], m.last().unwrap(), e[0], e.last().unwrap());
    assert!(first.is_none(), "{name}: mob {m:?}\necs {e:?}");
}

#[test]
fn poison_wither_and_regeneration_act_on_the_ticks_of_the_mob() {
    if skip() {
        return;
    }
    same("poison 0 for 200", timelines("minecraft:poison", Kind::Poison, 0, 200, 0.0, 230));
    same("poison 1 for 90", timelines("minecraft:poison", Kind::Poison, 1, 90, 0.0, 120));
    same("poison 2 for 37", timelines("minecraft:poison", Kind::Poison, 2, 37, 0.0, 60));
    same("wither 0 for 130", timelines("minecraft:wither", Kind::Wither, 0, 130, 0.0, 150));
    same("regeneration 0 for 220", timelines("minecraft:regeneration", Kind::Regeneration, 0, 220, 6.0, 240));
    same("regeneration 1 for 100", timelines("minecraft:regeneration", Kind::Regeneration, 1, 100, 6.0, 120));
}

#[test]
fn speed_and_slowness_scale_the_speed_as_the_mob_does() {
    if skip() {
        return;
    }
    for (id, kind, amp) in [("minecraft:speed", Kind::Speed, 0), ("minecraft:speed", Kind::Speed, 1), ("minecraft:slowness", Kind::Slowness, 0), ("minecraft:slowness", Kind::Slowness, 2)] {
        let (mut r, mut h) = flat_pair();
        let i = r.add(Vec3::new(0.5, 1.0, 0.5));
        let e = ecs_cow(&mut h, 0.5, 1.0, 0.5, 5);
        let (m0, e0) = (r.mobs[i].movement_speed(), h.world.get::<Body>(e).unwrap().speed);
        r.mobs[i].add_effect(id, 100, amp);
        effects::add(&mut h.world, e, kind, amp, 100);
        let (m1, e1) = (r.mobs[i].movement_speed(), h.world.get::<Body>(e).unwrap().speed);
        eprintln!("{id} {amp}: factor mob {} ecs {}", m1 / m0, e1 / e0);
        assert!(((m1 / m0) - (e1 / e0)).abs() < 1e-9);
        for _ in 0..101 {
            r.step(None);
            h.tick();
        }
        assert_eq!(h.world.get::<Body>(e).unwrap().speed, e0, "the speed is put back");
        assert!(effects::has(&h.world, e, kind) == false);
    }
}

#[test]
fn a_levitating_cow_rises_as_the_mob_s_does() {
    if skip() {
        return;
    }
    let (mut r, mut h) = flat_pair();
    let i = r.add(Vec3::new(0.5, 1.0, 0.5));
    let e = ecs_cow(&mut h, 0.5, 1.0, 0.5, 5);
    for _ in 0..3 {
        r.step(None);
        h.tick();
    }
    r.mobs[i].add_effect("minecraft:levitation", 40, 1);
    effects::add(&mut h.world, e, Kind::Levitation, 1, 40);
    let mut worst: f64 = 0.0;
    for t in 0..70 {
        r.step(None);
        h.tick();
        let (m, p) = (r.mobs[i].position.y, h.world.get::<Pos>(e).unwrap().0.y);
        worst = worst.max((m - p).abs());
        if t % 10 == 0 {
            eprintln!("tick {t}: y mob {m:.4} ecs {p:.4}");
        }
    }
    assert!(worst < 1e-6, "{worst}");
}

/// Two effects at once and the end of one: the speeds compose (speed times slowness) and come back one after the other, as the `Mob`'s.
#[test]
fn speed_and_slowness_together_compose_and_come_back_as_the_mob_s() {
    if skip() {
        return;
    }
    let (mut r, mut h) = flat_pair();
    let i = r.add(Vec3::new(0.5, 1.0, 0.5));
    let e = ecs_cow(&mut h, 0.5, 1.0, 0.5, 5);
    let base = h.world.get::<Body>(e).unwrap().speed;
    r.mobs[i].add_effect("minecraft:speed", 30, 1);
    r.mobs[i].add_effect("minecraft:slowness", 60, 0);
    effects::add(&mut h.world, e, Kind::Speed, 1, 30);
    effects::add(&mut h.world, e, Kind::Slowness, 0, 60);
    let m0 = r.mobs[i].movement_speed();
    for t in 0..70 {
        let (m, s) = (r.mobs[i].movement_speed(), h.world.get::<Body>(e).unwrap().speed / base);
        let want = m / m0 * (1.4 * 0.85);
        if t % 10 == 0 || t == 29 || t == 30 || t == 31 || t == 59 || t == 60 || t == 61 {
            eprintln!("tick {t}: mob factor {:.4} ecs {s:.4}", m / m0 * (1.4 * 0.85));
        }
        assert!((want - s).abs() < 1e-9, "tick {t}: mob {want} ecs {s}");
        r.step(None);
        h.tick();
    }
}
