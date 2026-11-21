//! The ravager against a real `Mob` ravager in the same scene (`HORIZON_TEST_RESOURCES` set): the bite (first tick, gaps), then a
//! stun forced through the same entry on both sides (`blocked` / `on_blocked`, the draw repeated until it stuns), the roar that
//! follows (its sound, its hit of 6 ten ticks later, the stun's particles), and no bite while busy. The entity events (4, 39, 69)
//! exist on the `Mob` side only: the ECS has no entity event yet.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_client_lib::mobs::MobEvent;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{
    cow::*,
    goals::*,
    hostile::*,
    host_out::HostEvent,
};

fn player_pos() -> Vec3 {
    Vec3::new(0.5, 1.0, 0.5)
}
/// The vanilla roar's hit (`AttackPlayer(6.0)`); the bite is another amount.
const ROAR_DAMAGE: f32 = 6.0;

#[derive(Default, Debug)]
struct Log {
    bites: Vec<i64>,
    roar_hits: Vec<i64>,
    sounds: Vec<(i64, String)>,
    stars: u32,
}

fn mob_run(at: &[(f64, f64)], ticks: i64, stun: bool) -> Log {
    let mut rf = MobRef::new(60, false);
    for &(x, z) in at {
        rf.add_kind(Vec3::new(x, 1.0, z), mob_kind("minecraft:ravager"));
    }
    let mut log = Log::default();
    if stun {
        let mut stunned = false;
        for _ in 0..200 {
            if horizon_client_lib::mobs::ravager::blocked(&mut rf.mobs[0], &mut rf.events) {
                stunned = true;
                break;
            }
        }
        assert!(stunned, "the draw stuns within 200 tries");
        take_mob(&rf.events, 0, 0, &mut log);
    }
    for t in 1..=ticks {
        let from = rf.events.len();
        let p = player_view(player_pos(), None);
        rf.step(Some(&p));
        take_mob(&rf.events, from, t, &mut log);
    }
    log
}
fn take_mob(events: &[MobEvent], from: usize, t: i64, log: &mut Log) {
    for e in &events[from..] {
        match e {
            MobEvent::AttackPlayer(a, _) if *a == ROAR_DAMAGE => log.roar_hits.push(t),
            MobEvent::AttackPlayer(..) => log.bites.push(t),
            MobEvent::Sound(n, ..) => log.sounds.push((t, n.to_string())),
            // the Mob sends one event per star with count 0 (a coloured particle); the ECS one burst carrying their count.
            MobEvent::Particles { kind, .. } if kind.to_string() == "entity_effect" => log.stars += 1,
            _ => {}
        }
    }
}

fn ecs_run(at: &[(f64, f64)], ticks: i64, stun: bool) -> Log {
    let mut h = Herd::with_slab_awake(60, true, false);
    h.world.resource_mut::<PlayerState>().immortal = true;
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let idx = table.iter().position(|s| s.name == "minecraft:ravager").expect("the ravager is in the table");
    let ids: Vec<Entity> = at
        .iter()
        .enumerate()
        .map(|(i, &(x, z))| {
            let e = spawn_species(&mut h.world, &table[idx], x, z, 1000 + i as u64);
            h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
            h.world.resource_mut::<Entered>().0.push(e);
            e
        })
        .collect();
    let mut log = Log::default();
    if stun {
        let mut stunned = false;
        for _ in 0..200 {
            if horizon_ecs_proto::behaviours::shield_stun::on_blocked(&mut h.world, ids[0]) {
                stunned = true;
                break;
            }
        }
        assert!(stunned, "the draw stuns within 200 tries");
        take_ecs(&mut h, 0, &mut log);
    }
    for t in 1..=ticks {
        let p = player_pos();
        h.world.resource_mut::<Online>().set(Player { id: 1, x: p.x, y: p.y, z: p.z, main: 0, off: 0, ..Default::default() });
        h.tick();
        take_ecs(&mut h, t, &mut log);
    }
    log
}
fn take_ecs(h: &mut Herd, t: i64, log: &mut Log) {
    for hit in h.world.resource::<PlayerState>().hits.iter().filter(|x| x.tick == t as u64) {
        if hit.amount == ROAR_DAMAGE {
            log.roar_hits.push(t);
        } else {
            log.bites.push(t);
        }
    }
    let out: Vec<HostEvent> = h.world.resource_mut::<Events>().host.drain().collect();
    for ev in out {
        match ev {
            HostEvent::Sound { name, .. } => log.sounds.push((t, name.to_string())),
            HostEvent::Particles { kind, count, .. } if kind == "entity_effect" => log.stars += count,
            _ => {}
        }
    }
}
fn sound_at(log: &Log, tail: &str) -> Option<i64> {
    log.sounds.iter().find(|(_, n)| n.ends_with(tail)).map(|(t, _)| *t)
}
fn gaps(v: &[i64]) -> f64 {
    let g: Vec<f64> = v.windows(2).map(|w| (w[1] - w[0]) as f64).collect();
    g.iter().sum::<f64>() / g.len().max(1) as f64
}

/// A ravager 3 blocks from a still player (the bite's reach): the first bite and the cadence of the bites, both sides.
#[test]
fn a_ravager_bites_as_a_mob_does() {
    if !have_data() {
        return;
    }
    let at = [(3.5, 0.5)];
    let (m, e) = (mob_run(&at, 120, false), ecs_run(&at, 120, false));
    eprintln!("bites: mob {:?} ecs {:?}", m.bites, e.bites);
    let (mf, ef) = (m.bites.first().copied().expect("the mob bites"), e.bites.first().copied().expect("the ecs bites"));
    assert!((mf - ef).abs() <= 3, "first bite: mob {mf}, ecs {ef}");
    assert!((gaps(&m.bites) - gaps(&e.bites)).abs() <= 2.0, "cadence: mob {:?}, ecs {:?}", m.bites, e.bites);
}

/// A stun at tick 0 (the shield), then the roar: while busy no bite; the roar's sound when the stun ends (40 ticks), its hit 10
/// ticks after that, and the stun's particles. The tolerance is one tick: the ECS timers are on the wheel, the `Mob` counts.
#[test]
fn a_shield_stun_then_the_roar_as_a_mob_does() {
    if !have_data() {
        return;
    }
    let at = [(2.5, 0.5)];
    let (m, e) = (mob_run(&at, 80, true), ecs_run(&at, 80, true));
    eprintln!("sounds: mob {:?} ecs {:?}", m.sounds, e.sounds);
    eprintln!("roar hits: mob {:?} ecs {:?}; stars: mob {} ecs {}", m.roar_hits, e.roar_hits, m.stars, e.stars);
    for (side, log) in [("mob", &m), ("ecs", &e)] {
        assert!(log.sounds.iter().any(|(t, n)| *t == 0 && n.ends_with("stunned")), "{side}: the stun sounds at tick 0");
        let roar = sound_at(log, "ravager.roar").unwrap_or_else(|| panic!("{side}: a roar sounds"));
        assert!((38..=42).contains(&roar), "{side}: the roar comes when the stun ends, at {roar}");
        assert!(log.bites.iter().all(|&b| b >= roar), "{side}: no bite while stunned or roaring: {:?}", log.bites);
        assert!(log.stars > 0, "{side}: the stun's particles");
    }
    let (mr, er) = (sound_at(&m, "ravager.roar").unwrap(), sound_at(&e, "ravager.roar").unwrap());
    assert!((mr - er).abs() <= 1, "roar sound: mob {mr}, ecs {er}");
    let (mh, eh) = (m.roar_hits.first().copied().expect("the mob's roar hits"), e.roar_hits.first().copied().expect("the ecs roar hits"));
    assert_eq!(mh - mr, 10, "the mob's roar hits 10 ticks after its sound");
    assert!((mh - eh).abs() <= 1, "roar hit: mob {mh}, ecs {eh}");
}
