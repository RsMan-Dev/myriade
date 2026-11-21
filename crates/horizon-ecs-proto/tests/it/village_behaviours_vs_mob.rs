//! The behaviours of the village side (`defend_village_target`, `offer_flower`, the raiders' goals, the zombie's village walk) against the
//! `Mob` in the same scene (`HORIZON_SPECIES` is the species the test is written for, `HORIZON_TEST_RESOURCES` set).
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_client_lib::gossip::{GossipType, Gossips};
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::behaviours::defend_village_target::Asks;
use horizon_ecs_proto::behaviours::gossip as eg;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::goals::*;
use horizon_ecs_proto::hostile::PlayerState;
use horizon_ecs_proto::targeting::Target;

const PLAYER: u32 = 7;
fn golem_skip() -> bool {
    !have_data() || wrong_species(&["minecraft:iron_golem"])
}
fn villager_ext(bad: bool) -> Option<Box<horizon_client_lib::mobs::villager::Ext>> {
    let mut gossips = Gossips::default();
    if bad {
        gossips.add([PLAYER as i32, 0, 0, 0], GossipType::MajorNegative, 100);
    }
    Some(Box::new(horizon_client_lib::mobs::villager::Ext { gossips, ..Default::default() }))
}
/// How a scene gives the player and the word: `walk` from x (per tick), `word_at` the tick the villager hears, `creative` the player.
struct Scene {
    from: f64,
    step: f64,
    stop: f64,
    word_at: usize,
    creative: bool,
}
fn player_x(s: &Scene, t: usize) -> f64 {
    (s.from + s.step * t as f64).clamp(s.from.min(s.stop), s.from.max(s.stop))
}
/// The first tick the golem holds the player as target, and the asks made by the ecs.
fn ecs_run(s: &Scene, ticks: usize) -> (Option<usize>, u64) {
    let mut h = Herd::with_slab_awake(40, true, false);
    h.tick();
    let sp = *h.world.resource::<SpeciesTable>().0.iter().find(|x| x.name == "minecraft:villager").unwrap();
    let g = spawn_species(&mut h.world, species(), 0.5, 0.5, 5);
    let v = spawn_species(&mut h.world, &sp, 3.5, 0.5, 6);
    h.world.get_mut::<ActiveUntil>(v).unwrap().0 = 0;
    h.world.resource_mut::<Entered>().0.extend([g, v]);
    if s.creative {
        let mut ps = h.world.resource_mut::<PlayerState>();
        ps.creative.resize(PLAYER as usize + 1, false);
        ps.creative[PLAYER as usize] = true;
    }
    let y = h.world.get::<Pos>(g).unwrap().0.y;
    h.world.init_resource::<Asks>();
    let mut first = None;
    for t in 0..ticks {
        if t == s.word_at {
            eg::add(&mut h.world, v, PLAYER, eg::Kind::MajorNegative, 100);
        }
        // The game reports a player when he moved: a player who stands still is no event.
        if t == 0 || player_x(s, t) != player_x(s, t - 1) {
            h.world.resource_mut::<Online>().set(Player { id: PLAYER, x: player_x(s, t), y, z: 0.5, ..Default::default() });
        }
        h.tick();
        if first.is_none() && h.world.get::<Target>(g).is_some() {
            first = Some(t);
        }
    }
    (first, h.world.resource::<Asks>().0)
}
fn mob_run(s: &Scene, ticks: usize) -> Option<usize> {
    let mut r = MobRef::new(40, false);
    let g = r.add_id(Vec3::new(0.5, 1.0, 0.5), 5);
    let v = r.add_kind(Vec3::new(3.5, 1.0, 0.5), mob_kind("minecraft:villager"));
    r.mobs[v].no_ai = true;
    let mut first = None;
    for t in 0..ticks {
        if t == s.word_at {
            r.mobs[v].extra.villager = villager_ext(true);
        }
        let mut view = player_view(Vec3::new(player_x(s, t), 1.0, 0.5), None);
        view.uuid = [PLAYER as i32, 0, 0, 0];
        view.creative = s.creative;
        r.step(Some(&view));
        if first.is_none() && r.mobs[g].ai.targets.is_running("DefendVillage") {
            first = Some(t);
        }
    }
    first
}

/// A player walks into the box of a golem whose villager already holds him in bad reputation: the same tick.
#[test]
fn defend_village_target_vs_mob_a_player_walking_in_is_taken_on_the_same_tick() {
    if golem_skip() {
        return;
    }
    let s = Scene { from: 30.0, step: -0.5, stop: 6.0, word_at: 0, creative: false };
    let (e, m) = (ecs_run(&s, 120).0, mob_run(&s, 120));
    eprintln!("walks in: ecs {e:?} mob {m:?}");
    assert!(m.is_some(), "the Mob never defends: the scene is wrong");
    assert_eq!(e, m);
}

/// The villager hears of him while he stands in the box: the same tick.
#[test]
fn defend_village_target_vs_mob_a_gossip_heard_with_the_player_standing_there() {
    if golem_skip() {
        return;
    }
    let s = Scene { from: 6.0, step: 0.0, stop: 6.0, word_at: 40, creative: false };
    let (e, m) = (ecs_run(&s, 100).0, mob_run(&s, 100));
    eprintln!("gossip: ecs {e:?} mob {m:?}");
    assert!(m.is_some());
    assert_eq!(e, m);
}

/// A creative player stands in the box for 2000 ticks, the villager holding him in bad reputation: the Mob asks each tick and refuses each tick;
/// the ecs asks on its events (the golem's creation, the word, the player's first appearance), a count of wake-ups that does not grow with the time.
#[test]
fn defend_village_target_vs_mob_asks_on_events_not_each_tick() {
    if golem_skip() {
        return;
    }
    let s = Scene { from: 6.0, step: 0.0, stop: 6.0, word_at: 5, creative: true };
    let (e, asks) = ecs_run(&s, 2000);
    assert_eq!(e, None);
    assert_eq!(mob_run(&s, 200), None);
    eprintln!("creative player standing: {asks} asks in 2000 ticks");
    assert!(asks <= 5, "{asks} asks in 2000 ticks");
}

/// A fence ring (x -1..=4, z -1..=2) too small for the golem to find a way anywhere: no stroll holds the ask off.
const PEN: [(i32, i32); 2] = [(-1, 4), (-1, 2)];
fn pen(mut set: impl FnMut(i32, i32)) {
    for x in PEN[0].0..=PEN[0].1 {
        for z in PEN[1].0..=PEN[1].1 {
            if x == PEN[0].0 || x == PEN[0].1 || z == PEN[1].0 || z == PEN[1].1 {
                set(x, z);
            }
        }
    }
}
/// `offer_flower`: the first start is a draw of one in 8000 asked every other tick (the `Mob` draws each ask, the ecs a geometric deadline: not the
/// same stream, so the law over seeds), and the poppy is held out for the same number of ticks.
fn flower_ecs(seed: u64, limit: usize) -> Option<(usize, usize)> {
    let mut h = Herd::with_slab_awake(40, true, false);
    h.tick();
    let sp = *h.world.resource::<SpeciesTable>().0.iter().find(|x| x.name == "minecraft:villager").unwrap();
    h.world.insert_resource(horizon_ecs_proto::hostile::Daylight(true));
    pen(|x, z| h.set_block(x, 1, z, "minecraft:oak_fence"));
    h.tick();
    let g = spawn_species(&mut h.world, species(), 0.5, 0.5, seed);
    let v = spawn_species(&mut h.world, &sp, 2.5, 0.5, seed + 1);
    h.world.get_mut::<ActiveUntil>(v).unwrap().0 = 0;
    h.world.resource_mut::<Entered>().0.extend([g, v]);
    let mut start = None;
    for t in 0..limit {
        h.tick();
        let on = h.world.get::<horizon_ecs_proto::behaviours::offer_flower::Offering>(g).is_some();
        match (start, on) {
            (None, true) => start = Some(t),
            (Some(s), false) => return Some((s, t - s)),
            _ => {}
        }
    }
    None
}
fn flower_mob(id: u32, limit: usize) -> Option<(usize, usize)> {
    let mut r = MobRef::new(40, false);
    pen(|x, z| r.set_block(x, 1, z, "minecraft:oak_fence"));
    let g = r.add_id(Vec3::new(0.5, 1.0, 0.5), id);
    let v = r.add_kind(Vec3::new(2.5, 1.0, 0.5), mob_kind("minecraft:villager"));
    r.mobs[v].no_ai = true;
    let mut start = None;
    for t in 0..limit {
        r.step(None);
        let on = r.mobs[g].ai.goals.is_running("OfferFlower");
        match (start, on) {
            (None, true) => start = Some(t),
            (Some(s), false) => return Some((s, t - s)),
            _ => {}
        }
    }
    None
}
#[test]
fn offer_flower_vs_mob_starts_by_the_same_law_and_lasts_as_long() {
    if golem_skip() {
        return;
    }
    const N: u64 = 60;
    const LIMIT: usize = 120_000;
    let e: Vec<_> = (0..N).map(|k| flower_ecs(100 + 2 * k, LIMIT)).collect();
    let m: Vec<_> = (0..N).map(|k| flower_mob(900 + k as u32, LIMIT)).collect();
    let mean = |v: &[Option<(usize, usize)>]| v.iter().map(|x| x.map_or(LIMIT, |p| p.0) as f64).sum::<f64>() / v.len() as f64;
    let (me, mm) = (mean(&e), mean(&m));
    let len = |v: &[Option<(usize, usize)>]| v.iter().flatten().map(|p| p.1 as f64).sum::<f64>() / v.len() as f64;
    let top = |v: &[Option<(usize, usize)>]| v.iter().flatten().map(|p| p.1).max();
    eprintln!("offer_flower: first start mean ecs {me:.0} mob {mm:.0}; length mean ecs {:.0} mob {:.0}; longest ecs {:?} mob {:?}", len(&e), len(&m), top(&e), top(&m));
    assert!(e.iter().all(Option::is_some) && m.iter().all(Option::is_some));
    // The stroll (every 240 ticks) takes the move flag from the poppy at times, in both: the longest hold is the full 400 ticks.
    assert_eq!((top(&e), top(&m)), (Some(400), Some(400)));
    assert!((0.7..1.43).contains(&(len(&e) / len(&m))), "length of the hold");
    assert!((0.7..1.43).contains(&(me / mm)), "ecs {me:.0} mob {mm:.0}");
}

fn raider_skip(name: &str) -> bool {
    !have_data() || wrong_species(&[name])
}
fn raider_state(m: &mut horizon_client_lib::mobs::Mob, f: impl FnOnce(&mut horizon_client_lib::mobs::raider::State)) {
    f(horizon_client_lib::mobs::raider::state_mut(m));
}
/// A raid lost at the village: the raider that belongs to it celebrates (starts on the same tick). The goal's draws come every other game tick: the Mob's cheer
/// (1 in 50, observable as a sound) and the jump (1 in 25) are counted over the same span; the Mob's jump draw follows its cheer draw, so the cheer rate checks the cadence.
fn celebrate_ecs(ticks: usize) -> (Option<usize>, u64) {
    use horizon_ecs_proto::{behaviours::celebration::Jumps, raids};
    let mut h = Herd::with_slab_awake(40, true, false);
    h.world.init_resource::<Jumps>();
    h.tick();
    let e = spawn_species(&mut h.world, species(), 0.5, 0.5, 31);
    h.world.resource_mut::<Entered>().0.push(e);
    let id = 1;
    raids::set_view(&mut h.world, id, Some(raids::RaidView { center: [0, 1, 0], active: true, over: true, loss: true, wave: 0 }));
    raids::enlist(&mut h.world, e, raids::Raider { raid: Some(id), wave: 1, leader: false }, false);
    raids::wake_raiders(&mut h.world, id);
    let mut start = None;
    for t in 0..ticks {
        h.tick();
        if start.is_none() && h.world.get::<raids::Celebrating>(e).is_some() {
            start = Some(t);
        }
    }
    (start, h.world.resource::<Jumps>().0)
}
fn celebrate_mob(ticks: usize) -> (Option<usize>, usize) {
    let mut r = MobRef::new(40, false);
    let g = r.add_id(Vec3::new(0.5, 1.0, 0.5), 31);
    for _ in 0..40 {
        r.step(None);
    }
    raider_state(&mut r.mobs[g], |s| {
        s.raid_id = Some(1);
        s.raid_over = true;
        s.raid_loss = true;
    });
    let mut start = None;
    for t in 0..ticks {
        r.step(None);
        if start.is_none() && horizon_client_lib::mobs::raider::state(&r.mobs[g]).is_some_and(|s| s.celebrating) {
            start = Some(t);
        }
    }
    (start, r.events.iter().filter(|e| format!("{e:?}").contains("celebrate")).count())
}
#[test]
fn celebration_vs_mob_starts_on_the_same_tick_and_draws_at_the_same_cadence() {
    if raider_skip("minecraft:pillager") {
        return;
    }
    let (e, m) = (celebrate_ecs(6000), celebrate_mob(6000));
    eprintln!("celebration: ecs {e:?} mob {m:?}");
    assert_eq!(e.0, m.0, "the tick it starts");
    assert!(m.1 > 30, "the Mob cheers: the scene is wrong");
    // Mob: cheers in 1/50 of the goal ticks (3000 of them); ECS: jumps in 1/25, so twice as many draws hit as cheers.
    let ratio = e.1 as f64 / (2 * m.1) as f64;
    assert!((0.7..1.43).contains(&ratio), "ecs {} jumps, mob {} cheers", e.1, m.1);
}

/// A banner lies near a raider of an active raid: the tick its walk to the banner starts, counted from the banner's arrival.
fn banner_ecs(at: usize, ticks: usize) -> Option<usize> {
    use horizon_ecs_proto::{behaviours::obtain_banner::Fetching, raids};
    let mut h = crate::raids_vs_mob::village_herd(40);
    let e = spawn_species(&mut h.world, species(), 0.5, 0.5, 31);
    h.world.resource_mut::<Entered>().0.push(e);
    let id = 1;
    raids::set_view(&mut h.world, id, Some(raids::RaidView { center: [0, 1, 0], active: true, over: false, loss: false, wave: 0 }));
    raids::enlist(&mut h.world, e, raids::Raider { raid: Some(id), wave: 1, leader: false }, false);
    for t in 0..ticks {
        if t == at {
            horizon_ecs_proto::item::drop_item(&mut h.world, horizon_client_lib::mobs::raider::ominous_banner(), Vec3::new(8.5, 1.0, 0.5), Vec3::ZERO, 0);
        }
        h.tick();
        if t >= at && h.world.get::<Fetching>(e).is_some() {
            return Some(t - at);
        }
    }
    None
}
fn banner_mob(at: usize, ticks: usize) -> Option<usize> {
    let mut r = MobRef::new(40, false);
    let g = r.add_id(Vec3::new(0.5, 1.0, 0.5), 31);
    for _ in 0..40 {
        r.step(None);
    }
    raider_state(&mut r.mobs[g], |s| s.active_raid = true);
    for t in 0..ticks {
        if t == at {
            raider_state(&mut r.mobs[g], |s| {
                s.banner_at = Some([8.5, 1.0, 0.5]);
                s.banner_item = 9;
            });
        }
        r.step(None);
        if t >= at && r.mobs[g].ai.goals.running_names().contains(&"ObtainBanner") {
            return Some(t - at);
        }
    }
    None
}
#[test]
fn obtain_banner_vs_mob_walks_when_the_banner_lies_near() {
    if raider_skip("minecraft:pillager") {
        return;
    }
    let got: Vec<_> = [3usize, 7, 12, 19].iter().map(|&a| (banner_ecs(a, 30 + a), banner_mob(a, 30 + a))).collect();
    eprintln!("obtain_banner: (ecs, mob) {got:?}");
    for (e, m) in &got {
        assert!(m.is_some(), "the Mob walks to the banner: the scene is wrong");
        assert_eq!(e, m, "the tick the walk starts");
    }
}

const BED: [i32; 3] = [30, 1, 0];
/// The tick the raider's walk to a home starts, and the tick it comes within a block and a half of it (the hops are drawn, so the arrival is a mean over `n` raiders).
fn homeward_ecs(n: usize, ticks: usize) -> (Option<usize>, Vec<Option<usize>>) {
    use horizon_ecs_proto::raids;
    let mut h = crate::raids_vs_mob::village_herd(40);
    h.set_block_state(BED[0], BED[1], BED[2], "minecraft:red_bed", &[("part", "head"), ("facing", "east")]);
    h.set_block_state(BED[0] - 1, BED[1], BED[2], "minecraft:red_bed", &[("part", "foot"), ("facing", "east")]);
    h.tick();
    let es: Vec<Entity> = (0..n).map(|i| spawn_species(&mut h.world, species(), -8.0 - i as f64 * 2.0, 0.5, 31 + i as u64)).collect();
    for &e in &es {
        h.world.resource_mut::<Entered>().0.push(e);
    }
    let id = 1;
    raids::set_view(&mut h.world, id, Some(raids::RaidView { center: [0, 1, 0], active: true, over: false, loss: false, wave: 0 }));
    for &e in &es {
        raids::enlist(&mut h.world, e, raids::Raider { raid: Some(id), wave: 1, leader: false }, false);
    }
    raids::wake_raiders(&mut h.world, id);
    let (mut start, mut arrive) = (None, vec![None; n]);
    for t in 0..ticks {
        h.tick();
        for (i, &e) in es.iter().enumerate() {
            if start.is_none() && h.world.get::<horizon_ecs_proto::behaviours::raid_move_through_village::Visiting>(e).is_some() {
                start = Some(t);
            }
            let p = h.world.get::<Pos>(e).unwrap().0;
            if arrive[i].is_none() && ((p.x - 30.5).powi(2) + (p.z - 0.5).powi(2)).sqrt() < 2.0 {
                arrive[i] = Some(t);
            }
        }
    }
    (start, arrive)
}
fn homeward_mob(n: usize, ticks: usize) -> (Option<usize>, Vec<Option<usize>>) {
    let mut r = MobRef::new(40, false);
    r.set_block_props(BED[0], BED[1], BED[2], "minecraft:red_bed", &[("part", "head"), ("facing", "east")]);
    r.set_block_props(BED[0] - 1, BED[1], BED[2], "minecraft:red_bed", &[("part", "foot"), ("facing", "east")]);
    for (p, n) in crate::raids_vs_mob::POINTS {
        r.set_block(p[0], p[1], p[2], n);
    }
    r.apply_block_events();
    let gs: Vec<usize> = (0..n).map(|i| r.add_id(Vec3::new(-8.0 - i as f64 * 2.0, 1.0, 0.5), 31 + i as u32)).collect();
    for _ in 0..40 {
        r.step(None);
    }
    for &g in &gs {
        raider_state(&mut r.mobs[g], |s| {
            s.active_raid = true;
            s.in_village = true;
            s.homes = Some(std::sync::Arc::new(vec![BED]));
        });
    }
    let (mut start, mut arrive) = (None, vec![None; n]);
    for t in 0..ticks {
        r.step(None);
        for (i, &g) in gs.iter().enumerate() {
            if start.is_none() && r.mobs[g].ai.goals.running_names().contains(&"MoveThroughVillage") {
                start = Some(t);
            }
            let p = r.mobs[g].position;
            if arrive[i].is_none() && ((p.x - 30.5).powi(2) + (p.z - 0.5).powi(2)).sqrt() < 2.0 {
                arrive[i] = Some(t);
            }
        }
    }
    (start, arrive)
}
#[test]
fn raid_move_through_village_vs_mob_starts_on_the_same_tick_and_arrives_as_fast() {
    if raider_skip("minecraft:pillager") {
        return;
    }
    let (e, m) = (homeward_ecs(8, 1500), homeward_mob(8, 1500));
    let mean = |v: &[Option<usize>]| v.iter().flatten().sum::<usize>() as f64 / v.iter().flatten().count().max(1) as f64;
    eprintln!("raid_move_through_village: start ecs {:?} mob {:?}; arrived ecs {} mean {:.0}, mob {} mean {:.0}", e.0, m.0, e.1.iter().flatten().count(), mean(&e.1), m.1.iter().flatten().count(), mean(&m.1));
    assert!(m.1.iter().flatten().count() >= 4, "the Mob walks to the bed: the scene is wrong");
    assert_eq!(e.0, m.0, "the tick it starts");
    assert!(e.1.iter().flatten().count() >= 4, "the ECS raiders reach the bed");
    assert!((0.7..1.43).contains(&(mean(&e.1) / mean(&m.1))), "arrival ecs {:.0}, mob {:.0}", mean(&e.1), mean(&m.1));
}

const PATROL_TARGET: [i32; 3] = [38, 1, 0];
/// A patrol of a leader and two followers, outside any raid, walks toward its target in hops of ten blocks: the tick the walk starts and the mean x
/// reached by `ticks` (the hops' draws are fixed by the target, not drawn).
fn patrol_ecs(ticks: usize) -> (Option<usize>, f64, Vec<(usize, [i32; 3])>) {
    use horizon_ecs_proto::raids::{Patrol, Raider};
    let mut h = Herd::with_slab_awake(40, false, false);
    h.tick();
    let es: Vec<Entity> = (0..3).map(|i| spawn_species(&mut h.world, species(), -20.0 - i as f64 * 7.0, 0.5, 31 + i as u64)).collect();
    for &e in &es {
        h.world.resource_mut::<Entered>().0.push(e);
    }
    for _ in 0..40 {
        h.tick();
    }
    for (i, &e) in es.iter().enumerate() {
        h.world.entity_mut(e).insert((Patrol { leader: i == 0, patrolling: true, target: Some(PATROL_TARGET), cooldown_until: 0 }, Raider { raid: None, wave: 0, leader: false }));
        horizon_ecs_proto::behaviours::long_distance_patrol::wake(&mut h.world, e);
    }
    let (mut start, mut hops, mut last) = (None, Vec::new(), None);
    for t in 0..ticks {
        h.tick();
        let tg = h.world.get::<Patrol>(es[1]).and_then(|p| p.target);
        if tg != last && tg.is_some() {
            hops.push((t, tg.unwrap()));
        }
        last = tg;
        if start.is_none() && es.iter().any(|&e| h.world.get::<horizon_ecs_proto::behaviours::long_distance_patrol::Hopping>(e).is_some()) {
            start = Some(t);
        }
    }
    (start, es.iter().map(|&e| h.world.get::<Pos>(e).unwrap().0.x).sum::<f64>() / 3.0, hops)
}
fn patrol_mob(ticks: usize) -> (Option<usize>, f64, Vec<(usize, [i32; 3])>) {
    let mut r = MobRef::new(40, false);
    let gs: Vec<usize> = (0..3).map(|i| r.add_id(Vec3::new(-20.0 - i as f64 * 7.0, 1.0, 0.5), 31 + i as u32)).collect();
    for _ in 0..40 {
        r.step(None);
    }
    for (i, &g) in gs.iter().enumerate() {
        raider_state(&mut r.mobs[g], |s| {
            s.patrolling = true;
            s.patrol_leader = i == 0;
            s.patrol_target = Some(PATROL_TARGET);
        });
    }
    let (mut start, mut seen, mut hops) = (None, 0, Vec::new());
    for t in 0..ticks {
        r.step(None);
        // The game's own answer to the leader's shout: its companions take the hop as their target.
        let shouts: Vec<_> = r.events[seen..].iter().filter_map(|e| if let horizon_client_lib::mobs::MobEvent::PatrolTarget { mob, target } = e { Some((*mob, *target)) } else { None }).collect();
        seen = r.events.len();
        for &(id, to) in &shouts {
            if id == r.mobs[gs[1]].id {
                hops.push((t, to));
            }
        }
        for (id, to) in shouts {
            if let Some(&g) = gs.iter().find(|&&g| r.mobs[g].id == id) {
                horizon_client_lib::mobs::raider::set_patrol_target(&mut r.mobs[g], to);
            }
        }
        if start.is_none() && gs.iter().any(|&g| r.mobs[g].ai.goals.running_names().contains(&"LongDistancePatrol")) {
            start = Some(t);
        }
    }
    (start, gs.iter().map(|&g| r.mobs[g].position.x).sum::<f64>() / 3.0, hops)
}
#[test]
fn long_distance_patrol_vs_mob_starts_on_the_same_tick_and_shouts_the_same_hops() {
    if raider_skip("minecraft:pillager") {
        return;
    }
    let (mut e, m) = (patrol_ecs(150), patrol_mob(150));
    e.2.retain(|h| h.1 != PATROL_TARGET);
    eprintln!("long_distance_patrol: start {:?} {:?}, hops ecs {:?} mob {:?}, mean x {:.1} {:.1}", e.0, m.0, e.2, m.2, e.1, m.1);
    assert!(m.1 > -25.0, "the Mob walks: the scene is wrong");
    assert_eq!(e.0, m.0, "the tick it starts");
    assert_eq!(e.2.first().map(|h| h.1), m.2.first().map(|h| h.1), "the first hop the leader gives its companions");
    assert!((e.1 - m.1).abs() < 1.5, "mean x after 150 ticks: ecs {:.1}, mob {:.1}", e.1, m.1);
}

/// A zombie at night near a village where a villager holds a point: the tick its walk through the village starts.
fn zombie_ecs(ticks: usize) -> Option<usize> {
    use horizon_ecs_proto::behaviours::move_through_village::Crossing;
    let mut h = crate::raids_vs_mob::village_herd(40);
    h.set_daylight(false);
    assert!(h.world.resource_mut::<horizon_ecs_proto::poi::PoiRegistry>().acquire([-4, 1, 0]), "a villager holds the lectern");
    let z = spawn_species(&mut h.world, species(), -12.0, 0.5, 5);
    h.world.resource_mut::<Entered>().0.push(z);
    (0..ticks).find(|_| {
        h.tick();
        h.world.get::<Crossing>(z).is_some()
    })
}
fn zombie_mob(ticks: usize) -> Option<usize> {
    use horizon_client_lib::mobs::MobWorld;
    let mut r = MobRef::new(40, false);
    for (p, n) in crate::raids_vs_mob::POINTS {
        r.set_block(p[0], p[1], p[2], n);
    }
    r.apply_block_events();
    r.w = Box::leak(Box::new(MobWorld { daylight: false, ..r.w.clone() }));
    r.add_kind(Vec3::new(3.5, 1.0, 2.5), mob_kind("minecraft:villager"));
    let z = r.add_id(Vec3::new(-12.0, 1.0, 0.5), 5);
    (0..ticks).find(|_| {
        r.step(None);
        r.mobs[z].ai.goals.running_names().contains(&"MoveThroughVillage")
    })
}
#[test]
fn move_through_village_vs_mob_starts_at_night_near_a_held_point() {
    if raider_skip("minecraft:zombie") {
        return;
    }
    let (e, m) = (zombie_ecs(1200), zombie_mob(1200));
    eprintln!("move_through_village: start tick ecs {e:?} mob {m:?}");
    assert!(m.is_some(), "the Mob walks through the village: the scene is wrong");
    // The Mob asks `canUse` at each goal tick; the module asks every 60 ticks (a drawn first wait): it starts within one period, not on the same tick.
    let (e, m) = (e.expect("the ECS zombie walks through the village"), m.unwrap());
    assert!(e >= m && e <= m + 61, "ecs start {e} within one period of the Mob's {m}");
}
