//! The bee (`HORIZON_SPECIES=minecraft:bee`) against a real `Mob` bee: a poppy near it (one bee, one flower, one level: the bees would
//! meet): it flies to the flower, hovers over it for 400 ticks or more and takes the nectar. The tick of the nectar and the place the bee hovers
//! at, 16 draws on each side (the laws: the draws differ); then, with a hive, the bee with nectar goes home and the ecs bee is out of the world
//! while it is inside (the `Mob` side has no hive block entity in the harness: only the ecs side is checked there).
use crate::common;
use bevy_ecs::prelude::Entity;
use common::*;
use horizon_client_lib::mobs::bee as mbee;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::behaviours::bee::{Bee, Hives, InHive};
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::goals::arm_all;

#[test]
fn a_bee_pollinates_a_flower_as_a_mob_does() {
    if !have_data() {
        return;
    }
    let _ = flat_pair();
    if species().name != "minecraft:bee" {
        return;
    }
    let n = 16;
    let (mut mt, mut et, mut md, mut ed) = (Vec::new(), Vec::new(), 0.0, 0.0);
    for k in 0..n {
        let (mut r, mut h) = flat_pair();
        let f = (3 + k % 3, 1, -2 + k % 4);
        r.set_block(f.0, f.1, f.2, "minecraft:poppy");
        h.set_block(f.0, f.1, f.2, "minecraft:poppy");
        h.tick();
        let s = Vec3::new(0.5, 2.5, 0.5);
        let (i, e) = (r.add(s), ecs_cow(&mut h, s.x, s.y, s.z, 3100 + k as u64));
        // The ecs animal learns where it is at its first ticks.
        arm_all(&mut h.world);
        let (mut mn, mut en) = (None, None);
        let (mut mp, mut ep) = (None, None);
        for t in 1..=900_i64 {
            r.mobs[i].ai.no_action_time = 0;
            if std::env::var_os("TRACE9").is_some() && (k == 0 || k == 2) && (44..=50).contains(&t) {
                let mw = r.w;
                let q = r.mobs[i].create_path_to(mw, [f.0, f.1, f.2], 1);
                eprintln!("k{k} t={t} mob pos ({:.2},{:.2},{:.2}) path {:?} reached {:?} cd? flower {:?}", r.mobs[i].position.x, r.mobs[i].position.y, r.mobs[i].position.z, q.is_some(), q.as_ref().map(|p| p.reached), mbee::get(&r.mobs[i]).map(|b| (b.flower_cooldown, b.flower)));
            }
            r.step(None);
            h.tick();
            if std::env::var_os("TRACE8").is_some() && k == 2 && t % 10 == 0 && t <= 160 {
                let fc = Vec3::new(f64::from(f.0) + 0.5, f64::from(f.1) + 0.6, f64::from(f.2) + 0.5);
                eprintln!("t={t} MOB dist {:.2} pollinating {} | ECS dist {:.2} pollinating {} following {}", r.mobs[i].position.distance(fc), mbee::pollinating(&r.mobs[i]), h.world.get::<Pos>(e).unwrap().0.distance(fc), h.world.get::<Bee>(e).map_or(false, |b| b.pollinating), h.world.entity(e).contains::<Following>());
            }
            if mp.is_none() && mbee::pollinating(&r.mobs[i]) {
                mp = Some(t);
            }
            if ep.is_none() && h.world.get::<Bee>(e).is_some_and(|b| b.pollinating) {
                ep = Some(t);
            }
            if mn.is_none() && mbee::has_nectar(&r.mobs[i]) {
                mn = Some(t);
            }
            if en.is_none() && h.world.get::<Bee>(e).is_some_and(|b| b.nectar) {
                en = Some(t);
            }
            if mn.is_some() && en.is_some() {
                break;
            }
        }
        if std::env::var_os("TRACE").is_some() {
            eprintln!("bee {k}: mob pollinating at {mp:?} nectar {mn:?} | ecs pollinating at {ep:?} nectar {en:?}");
        }
        let c = Vec3::new(f64::from(f.0) + 0.5, 1.6, f64::from(f.2) + 0.5);
        md += r.mobs[i].position.distance(c);
        ed += h.world.get::<Pos>(e).unwrap().0.distance(c);
        if let Some(t) = mn {
            mt.push(t as f64);
        }
        if let Some(t) = en {
            et.push(t as f64);
        }
    }
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len().max(1) as f64;
    eprintln!("bee nectar: mob {}/{n} at {:.0}, dist {:.2} | ecs {}/{n} at {:.0}, dist {:.2}", mt.len(), mean(&mt), md / n as f64, et.len(), mean(&et), ed / n as f64);
    // Measured, not equal: the `Mob` bee approaches the flower slowly by a direct wanted point (0.35) once the first path ends and counts the time
    // of the pollination only within a block of it, and half of its first asks find no reachable flower (the 600-tick cache): see PORTABLES.md.
    assert!(et.len() == n as usize && mt.len() >= 6, "the bees get nectar");
    assert!((mean(&mt) - mean(&et)).abs() < 0.25 * mean(&mt).max(mean(&et)), "the time of the nectar");
}

#[test]
fn a_bee_with_nectar_goes_into_its_hive_and_comes_out() {
    if !have_data() {
        return;
    }
    let _ = flat_pair();
    if species().name != "minecraft:bee" {
        return;
    }
    let (_r, mut h) = flat_pair();
    h.world.insert_resource(horizon_ecs_proto::behaviours::bee::StayInHive(Some(false)));
    h.set_block(4, 1, 0, "minecraft:bee_nest");
    h.tick();
    let e = ecs_cow(&mut h, 0.5, 2.5, 0.5, 3300);
    arm_all(&mut h.world);
    {
        let now = h.world.resource::<horizon_ecs_proto::cow::Clock>().0;
        let mut b = h.world.get_mut::<Bee>(e).unwrap();
        b.nectar = true;
        b.hive = Some([4, 1, 0]);
        b.no_nectar_since = now;
    }
    // The nectar is the event that wakes the hive goals (a bee that takes it by pollinating asks them itself).
    horizon_ecs_proto::behaviours::bee::ask_hive(&mut h.world, e);
    let (mut inside, mut out_again) = (None, None);
    for t in 1..=3500_i64 {
        h.tick();
        if inside.is_none() && h.world.get::<InHive>(e).is_some() {
            inside = Some(t);
        }
        if inside.is_some() && out_again.is_none() && h.world.get::<InHive>(e).is_none() {
            out_again = Some(t);
        }
    }
    eprintln!("bee hive: in at {inside:?}, out at {out_again:?}; hives {}", h.world.resource::<Hives>().bees.len());
    assert!(inside.is_some(), "the bee went in");
    assert!(out_again.is_some_and(|o| o - inside.unwrap() >= 2400), "it stays 2400 ticks with nectar");
}

/// What the `Mob` answers when asked for the path to a flower (the first search of the pollination), for flowers at z -2 .. 1.
#[test]
fn the_mobs_first_search_of_a_flower() {
    if !have_data() || std::env::var_os("PROBE").is_none() {
        return;
    }
    let _ = flat_pair();
    if species().name != "minecraft:bee" {
        return;
    }
    for z in -2..=1 {
        let (mut r, mut h) = flat_pair();
        r.set_block(3, 1, z, "minecraft:poppy");
        h.set_block(3, 1, z, "minecraft:poppy");
        h.tick();
        let (i, e) = (r.add(Vec3::new(0.5, 2.5, 0.5)), ecs_cow(&mut h, 0.5, 2.5, 0.5, 3200));
        for _ in 0..3 {
            r.mobs[i].ai.no_action_time = 0;
            r.step(None);
            h.tick();
        }
        let mw = r.w;
        let path = r.mobs[i].create_path_to(mw, [3, 1, z], 1);
        let ecs = horizon_ecs_proto::locomotion::fly::reaches(&h.world, e, [3, 1, z], 1);
        eprintln!("flower z={z}: mob path {:?} reached {:?} nodes {:?} | ecs reaches {ecs} | mob pos {:?} on_ground {}", path.is_some(), path.as_ref().map(|p| p.reached), path.as_ref().map(|p| p.nodes.len()), r.mobs[i].position, r.mobs[i].on_ground);
    }
}

/// A hive that is removed lets everyone inside out at once (`BeehiveBlockEntity`'s emergency release), by the event of the point of interest.
#[test]
fn a_removed_hive_lets_its_bees_out() {
    if !have_data() {
        return;
    }
    let _ = flat_pair();
    if species().name != "minecraft:bee" {
        return;
    }
    let (_r, mut h) = flat_pair();
    h.world.insert_resource(horizon_ecs_proto::behaviours::bee::StayInHive(Some(true)));
    h.set_block(4, 1, 0, "minecraft:bee_nest");
    h.tick();
    let e = ecs_cow(&mut h, 4.5, 1.5, 0.5, 3400);
    horizon_ecs_proto::behaviours::bee::enter_hive(&mut h.world, e, [4, 1, 0]);
    for _ in 0..10 {
        h.tick();
    }
    assert!(h.world.get::<InHive>(e).is_some(), "it stays in at night");
    h.set_block(4, 1, 0, "minecraft:air");
    for _ in 0..5 {
        h.tick();
    }
    assert!(h.world.get::<InHive>(e).is_none(), "the hive is gone: it is out");
}

/// `Bee.doHurtTarget`: an angry bee that reaches its target stings it (poison), has stung, forgets its anger and drops its target; then it
/// dies by the draw of `customServerAiStep`. The Mob side of this (a creature to hit in the harness) is not compared: only the rules are checked.
#[test]
fn an_angry_bee_stings_once_and_is_spent() {
    use horizon_ecs_proto::behaviours::{bee::Stung, neutral_anger::is_angry};
    use horizon_ecs_proto::effects::{has, Kind};
    use horizon_ecs_proto::targeting::{set_target, Target, Who};
    if !have_data() {
        return;
    }
    let _ = flat_pair();
    if species().name != "minecraft:bee" {
        return;
    }
    let (_r, mut h) = flat_pair();
    let e = ecs_cow(&mut h, 0.5, 1.5, 0.5, 3500);
    let foe = ecs_cow(&mut h, 1.5, 1.5, 0.5, 3501);
    arm_all(&mut h.world);
    for _ in 0..10 {
        h.tick();
    }
    assert!(!h.world.get::<Bee>(e).unwrap().stung && !has(&h.world, foe, Kind::Poison), "witness: nothing stung before the anger");
    let mut t = Target::new(Who::Entity(foe), u16::MAX, u8::MAX);
    t.must_see = false;
    t.follow_scale = f64::INFINITY;
    set_target(&mut h.world, e, t);
    h.tick();
    assert!(is_angry(&h.world, e), "the target makes it angry (witness: the sting needs the anger)");
    for _ in 0..200 {
        h.tick();
        if h.world.get::<Bee>(e).is_none_or(|b| b.stung) {
            break;
        }
    }
    for _ in 0..6 {
        h.tick();
    }
    let Some(b) = h.world.get::<Bee>(e) else { panic!("the bee is gone before the check") };
    assert!(b.stung && !b.nectar, "it has stung and its nectar is gone");
    assert!(h.world.get::<Stung>(e).is_some(), "the slow death is under way");
    assert!(has(&h.world, foe, Kind::Poison), "the victim is poisoned");
    assert!(!is_angry(&h.world, e) && h.world.get::<Target>(e).is_none(), "it forgot its anger and its target");
}

/// An angry bee and its victim (a second bee), the bee holding the victim as its target; the world ticked once so the anger is taken.
fn angry_pair(seed: u64) -> (Herd, Entity, Entity) {
    use horizon_ecs_proto::targeting::{set_target, Target, Who};
    let (_r, mut h) = flat_pair();
    let e = ecs_cow(&mut h, 0.5, 1.5, 0.5, seed);
    let foe = ecs_cow(&mut h, 1.5, 1.5, 0.5, seed + 1);
    arm_all(&mut h.world);
    for _ in 0..10 {
        h.tick();
    }
    let mut t = Target::new(Who::Entity(foe), u16::MAX, u8::MAX);
    t.must_see = false;
    t.follow_scale = f64::INFINITY;
    set_target(&mut h.world, e, t);
    h.tick();
    (h, e, foe)
}
/// The poison of the sting by the level's difficulty (`Bee.doHurtTarget`: none on easy, 10 s on normal, 18 s on hard).
#[test]
fn the_poison_of_a_sting_follows_the_difficulty() {
    use horizon_ecs_proto::effects::{Difficulty, Effects, Kind};
    if !have_data() {
        return;
    }
    let _ = flat_pair();
    if species().name != "minecraft:bee" {
        return;
    }
    for (id, seconds) in [(1_u8, 0_u64), (2, 10), (3, 18)] {
        let (mut h, e, foe) = angry_pair(3600 + u64::from(id));
        h.world.insert_resource(Difficulty(id));
        let mut seen = None;
        for _ in 0..300 {
            h.tick();
            let now = now_of(&h.world, foe);
            if let Some(f) = h.world.get::<Effects>(foe).and_then(|f| f.list.iter().find(|f| f.kind == Kind::Poison).copied()) {
                seen = Some(f.ends.saturating_sub(now));
                break;
            }
            if h.world.get::<Bee>(e).is_some_and(|b| b.stung) && seconds == 0 {
                for _ in 0..10 {
                    h.tick();
                }
                break;
            }
        }
        assert!(h.world.get::<Bee>(e).unwrap().stung, "difficulty {id}: the bee stung");
        match seen {
            Some(left) => assert!(seconds > 0 && left.abs_diff(seconds * 20) <= 4, "difficulty {id}: {left} ticks of poison left, wanted {}", seconds * 20),
            None => assert_eq!(seconds, 0, "difficulty {id}: no poison"),
        }
    }
}
/// A refused hit (the victim is in its hurt cooldown against a bigger blow) gives the stinger and the anger back: no poison, the bee tries again.
#[test]
fn a_refused_sting_is_given_back() {
    use horizon_ecs_proto::behaviours::neutral_anger::is_angry;
    use horizon_ecs_proto::effects::{has, Kind};
    if !have_data() {
        return;
    }
    let _ = flat_pair();
    if species().name != "minecraft:bee" {
        return;
    }
    let (mut h, e, foe) = angry_pair(3700);
    let (mut tried, mut given_back) = (false, false);
    for _ in 0..80 {
        let now = now_of(&h.world, foe);
        h.world.entity_mut(foe).insert(HurtAt { at: now, last: 1000.0 });
        h.tick();
        assert!(!has(&h.world, foe, Kind::Poison), "a refused sting poisoned");
        let stung = h.world.get::<Bee>(e).unwrap().stung;
        tried |= stung;
        given_back |= tried && !stung && is_angry(&h.world, e);
    }
    assert!(tried, "witness: the bee struck (and its sting was taken)");
    assert!(given_back, "the refused sting was not given back");
    for _ in 0..200 {
        h.tick();
        if h.world.get::<Bee>(e).is_none_or(|b| b.stung) {
            break;
        }
    }
    for _ in 0..5 {
        h.tick();
    }
    assert!(h.world.get::<Bee>(e).unwrap().stung && has(&h.world, foe, Kind::Poison), "witness: without the cooldown the sting lands");
}
/// The time a baby bee spends in the hive counts for its growth, and the love of an adult for its length (`setBeeReleaseData`).
#[test]
fn the_time_in_a_hive_ages_and_calms_a_bee() {
    use horizon_ecs_proto::behaviours::{breed::Love, follow_parent::make_baby};
    if !have_data() {
        return;
    }
    let _ = flat_pair();
    if species().name != "minecraft:bee" {
        return;
    }
    let (_r, mut h) = flat_pair();
    h.world.insert_resource(horizon_ecs_proto::behaviours::bee::StayInHive(Some(false)));
    h.set_block(4, 1, 0, "minecraft:bee_nest");
    h.tick();
    let (baby, adult) = (ecs_cow(&mut h, 4.5, 1.5, 0.5, 3800), ecs_cow(&mut h, 4.5, 1.5, 1.5, 3801));
    arm_all(&mut h.world);
    make_baby(&mut h.world, baby, 3000);
    let now = now_of(&h.world, adult);
    h.world.entity_mut(adult).insert(Love(now + 2000));
    let left = |h: &Herd, e: Entity| h.world.get::<GrowAt>(e).map(|g| g.0.saturating_sub(now_of(&h.world, e)));
    let love_left = |h: &Herd, e: Entity| h.world.get::<Love>(e).map(|l| l.0.saturating_sub(now_of(&h.world, e)));
    let (g0, l0) = (left(&h, baby).unwrap(), love_left(&h, adult).unwrap());
    let t0 = h.world.resource::<Clock>().0;
    for e in [baby, adult] {
        horizon_ecs_proto::behaviours::bee::enter_hive(&mut h.world, e, [4, 1, 0]);
    }
    for _ in 0..800 {
        h.tick();
        if h.world.get::<InHive>(baby).is_none() && h.world.get::<InHive>(adult).is_none() {
            break;
        }
    }
    let stay = h.world.resource::<Clock>().0 - t0;
    assert!(h.world.get::<InHive>(baby).is_none() && stay >= 600, "witness: both came out after the stay ({stay} ticks)");
    let (g1, l1) = (left(&h, baby).unwrap(), love_left(&h, adult).unwrap());
    assert!((g0 - g1).abs_diff(stay) <= 3, "the baby grew by {} ticks in {stay}", g0 - g1);
    assert!((l0 - l1).abs_diff(stay) <= 3, "the love shortened by {} ticks in {stay}", l0 - l1);
}
