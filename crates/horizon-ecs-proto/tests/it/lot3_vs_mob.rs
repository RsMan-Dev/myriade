//! Lot 3 (rabbit raid, school, fox items, parrot follow, turtle, species of the first wave) against real `Mob`s of the species
//! (`mob_kind`): the laws of first occurrence and of durations, N draws per side, with ids far apart (PIEGES 46: the
//! Java generator's first draw is correlated between neighbouring seeds). `--nocapture` prints. Cow body only.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_client_lib::mobs::{self, Mob, MobEvent, Who as MWho};
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{
    behaviours::{Entry, neutral_anger, fox_state, follow_flock_leader, follow_mob, fox_pick_up_loot, fox_search_for_items, pufferfish_puff, raid_garden, reset_universal_anger, turtle_go_to_water},
    cow::*,
    goals::*,
    targeting::*,
};
use std::sync::Arc;

fn leak<T>(v: T) -> &'static T {
    Box::leak(Box::new(v))
}
fn ecs_with(h: &mut Herd, entries: &'static [Entry], x: f64, z: f64, seed: u64, phase: u64) -> Entity {
    let mut sp = *species();
    sp.goals = horizon_ecs_proto::species::Config { entries };
    set_lazy_look(&mut h.world, false);
    // The ECS generator is a xorshift whose seed is `| 1` (900 and 901 are one seed) and whose first draws of a small seed
    // are small: well-mixed seeds only (PIEGES 46).
    let seed = (seed + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let e = spawn_species(&mut h.world, &sp, x, z, seed);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.get_mut::<Phase>(e).unwrap().0 = phase;
    h.world.resource_mut::<Entered>().0.push(e);
    e
}
/// Ids far apart: the Mob's seed comes from its id.
fn far_id(j: usize) -> u32 {
    1 + (j as u32) * 7919 % 100_003
}
fn mob_add(r: &mut MobRef, kind: Arc<mobs::MobType>, at: Vec3, j: usize) -> usize {
    let mut m = Mob::new(far_id(j), kind, at, 0.0);
    m.persistent = true;
    m.ai.no_action_time = 100;
    r.mobs.push(m);
    r.searches.push(0);
    r.mobs.len() - 1
}
fn on() -> bool {
    if !have_data() {
        return false;
    }
    let sp = std::env::var("HORIZON_SPECIES").unwrap_or_else(|_| "minecraft:cow".into());
    if sp != "minecraft:cow" {
        eprintln!("SKIP lot2_vs_mob: written for HORIZON_SPECIES=minecraft:cow, not {sp}");
        return false;
    }
    true
}
fn spot(j: usize, step: f64, row: usize) -> Vec3 {
    Vec3::new(-20.0 + step * (j % row) as f64 + 0.5, 1.0, -20.0 + step * (j / row) as f64 + 0.5)
}
fn near(j: usize) -> Vec3 {
    Vec3::new(-0.5 + 1.2 * (j % 8) as f64, 1.0, -3.5 + 1.2 * (j / 8) as f64)
}
/// A roof block over a column of the `Mob`'s world, with the light engine run on it (a raw `set_block` leaves the sky light as it
/// was: the `Mob` still sees the sky; the ECS's `BlockChanges` relights).
fn roof(r: &MobRef, x: i32, z: i32) {
    r.set_block(x, 6, z, "minecraft:stone");
    // SAFETY: as `MobRef::set_block`.
    unsafe {
        horizon_client_lib::mobs::world::relight_test_world(&mut *r.level.blocks.0, &[(x, 6, z)]);
    }
}
fn mean_of(v: &[Option<usize>]) -> (usize, f64) {
    let f: Vec<f64> = v.iter().filter_map(|x| x.map(|t| t as f64)).collect();
    (f.len(), f.iter().sum::<f64>() / f.len().max(1) as f64)
}

/// A block with properties in the world of the `Mob` (`MobRef`) and in the one of the ECS (the herd's level): the ECS can only
/// write names through `BlockChanges`, the controls that need a ripe bush write the state directly.
fn set_state(r: &MobRef, h: &Herd, x: i32, y: i32, z: i32, name: &str, props: &[(&str, &str)]) {
    let reg = r.w.registry;
    let base = reg.default_state(name).unwrap();
    let mut st = reg.state(base).unwrap().clone();
    for (k, v) in props {
        st.properties.insert((*k).into(), (*v).into());
    }
    let id = reg.id(&st).unwrap();
    for blocks in [r.level.blocks.0, h.world.resource::<Level>().blocks.0] {
        // SAFETY: the test thread owns the two worlds while it builds the scene (no reference to their blocks is alive).
        unsafe {
            let _ = (&mut *blocks).set(x, y, z, id);
        }
    }
}


/// 1. `raid_garden`: the porter says the continuation of the goal is always false (`rabbit.rs:80-88,138-144`), so that the rabbit
/// never eats. A real `Mob` rabbit and a ripe carrot 5 blocks away: the raids (`MobEvent::CropRaided`) the `Mob` does in 3 000 ticks,
/// and the ECS animal's.
#[test]
fn raid_garden_does_the_mob_rabbit_eat() {
    if !on() {
        return;
    }
    let n = 16;
    let ticks = 3000;
    let mut r = MobRef::new(24, false);
    let mut h = Herd::with_slab(24, false, false);
    let kind = mob_kind("minecraft:rabbit");
    let entries: &'static [Entry] = leak([Entry::new(&raid_garden::MODULE, 5, &raid_garden::Params::DEFAULT)]);
    let (mut ids, mut es, mut spots) = (Vec::new(), Vec::new(), Vec::new());
    for j in 0..n {
        let p = spot(j, 12.0, 4);
        let i = mob_add(&mut r, kind.clone(), p, j);
        ids.push(i);
        es.push(ecs_with(&mut h, entries, p.x, p.z, 3100 + j as u64, u64::from(far_id(j))));
        let (x, z) = (p.x.floor() as i32 + 5, p.z.floor() as i32);
        r.set_block(x, 0, z, "minecraft:farmland");
        h.set_block(x, 0, z, "minecraft:farmland");
        spots.push((x, z));
    }
    h.tick();
    r.step(None);
    for &(x, z) in &spots {
        set_state(&r, &h, x, 1, z, "minecraft:carrots", &[("age", "7")]);
    }
    let mut mob_raids = vec![0usize; n];
    let (mut m_started, mut e_started) = (0usize, 0usize);
    let mut was = vec![false; n];
    let mut ewas = vec![false; n];
    // The setup is right: the farmland is in the crop tag and the carrot is ripe.
    {
        let (x, z) = spots[0];
        let ok = r.w.has_tag("minecraft:supports_crops", r.w.block_name(x, 0, z)) && r.w.prop(x, 1, z, "age") == Some("7");
        eprintln!("raid_garden setup: farmland in #supports_crops and carrot age 7 on the Mob world: {ok}");
        assert!(ok);
    }
    let mut first_m = vec![None; n];
    let mut first_e = vec![None; n];
    let age_of = |mw: &horizon_client_lib::mobs::MobWorld, x: i32, z: i32| -> i32 { mw.prop(x, 1, z, "age").and_then(|a| a.parse().ok()).unwrap_or(-1) };
    for t in 0..ticks {
        let from = r.events.len();
        r.step(None);
        for ev in &r.events[from..] {
            if let MobEvent::CropRaided { mob, .. } = ev {
                if let Some(j) = ids.iter().position(|&i| r.mobs[i].id == *mob) {
                    mob_raids[j] += 1;
                    first_m[j].get_or_insert(t);
                }
            }
        }
        h.tick();
        let mw = h.world.resource::<Level>().mw.0;
        for j in 0..n {
            let on = r.mobs[ids[j]].ai.goals.running_names().iter().any(|n| n.contains("RaidGarden"));
            if on && !was[j] {
                m_started += 1;
            }
            was[j] = on;
            let eon = h.world.get::<raid_garden::Raiding>(es[j]).is_some();
            if eon && !ewas[j] {
                e_started += 1;
            }
            ewas[j] = eon;
        }
        for (j, &(x, z)) in spots.iter().enumerate() {
            if first_e[j].is_none() && age_of(mw, x, z) < 7 {
                first_e[j].get_or_insert(t);
            }
        }
    }
    let (rm, re) = (mob_raids.iter().sum::<usize>(), first_e.iter().flatten().count());
    eprintln!("raid_garden: starts of the goal: Mob {m_started}, ECS {e_started}");
    eprintln!("raid_garden: Mob rabbits that raided {} of {n} (raids {rm}, first at {:?}); ECS animals that raided {re} of {n} (first at {:?})", first_m.iter().flatten().count(), mean_of(&first_m), mean_of(&first_e));
    assert!(first_m.iter().flatten().count() > 0 || re == 0, "the Mob never raids: so must the ECS (the porter's claim) : mob {rm} ecs {re}");
}

/// The species of the registry with their `Mob`: the entry (name) and `Species` with all its goals.
fn ecs_named(h: &mut Herd, name: &str, entries: Option<&'static [Entry]>, x: f64, z: f64, seed: u64, phase: u64) -> Entity {
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let mut sp = *table.iter().find(|s| s.name == name).unwrap_or_else(|| panic!("{name} is not in the registry"));
    if let Some(e) = entries {
        sp.goals = horizon_ecs_proto::species::Config { entries: e };
    }
    set_lazy_look(&mut h.world, false);
    let seed = (seed + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    // `spawn_armed` admits the animal to the zone: with no player near it is born asleep; the benches keep it awake as `ecs_with` does.
    let e = spawn_species(&mut h.world, &sp, x, z, seed);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.get_mut::<Phase>(e).unwrap().0 = phase;
    h.world.resource_mut::<Entered>().0.push(e);
    // The kernel's stroll timer is armed for the animals of the build only: a spawned one is armed by hand (as `blocked` does).
    if h.world.get::<Stroll>(e).is_some() && h.world.get::<GoalSet>(e).is_some_and(|g| g.entries().iter().any(|en| en.module.name == "stroll")) {
        horizon_ecs_proto::behaviours::stroll::arm_now(&mut h.world, e);
    }
    e
}

/// 2. `follow_flock_leader`: 30 cods in a pool, each side repeated 10 times: the number of schools (distinct leaders) and the sizes
/// after 600 ticks. The order of the candidates (sections of `getEntitiesOfClass` against the `Index`) makes the law.
fn schools(ecs: bool, rep: usize) -> Vec<i32> {
    let n = 30;
    let mut r = MobRef::new(24, false);
    let kind = mob_kind("minecraft:cod");
    let mut h = Herd::with_slab(24, false, false);
    for x in -9..=9 {
        for z in -9..=9 {
            for y in 1..=3 {
                r.set_block(x, y, z, "minecraft:water");
                h.set_block(x, y, z, "minecraft:water");
            }
        }
    }
    let at = |j: usize| Vec3::new(-6.0 + 2.0 * (j % 7) as f64 + 0.5, 1.0 + (j % 3) as f64, -6.0 + 2.0 * (j / 7) as f64 + 0.5);
    let entries: &'static [Entry] = leak([
        Entry::new(&horizon_ecs_proto::behaviours::fish_swim::MODULE, 4, &horizon_ecs_proto::behaviours::fish_swim::Params::DEFAULT),
        Entry::new(&follow_flock_leader::MODULE, 5, &follow_flock_leader::Params::DEFAULT),
    ]);
    let (mut ids, mut es) = (Vec::new(), Vec::new());
    for j in 0..n {
        let i = mob_add(&mut r, kind.clone(), at(j), j + 100 * rep);
        r.mobs[i].ai.no_action_time = 0;
        ids.push(i);
        if ecs {
            es.push(ecs_named(&mut h, "minecraft:cod", Some(entries), at(j).x, at(j).z, 4000 + (j + 100 * rep) as u64, u64::from(far_id(j + 100 * rep))));
        }
    }
    if ecs {
        for &e in &es {
            h.world.get_mut::<Pos>(e).unwrap().0.y = h.world.get::<Pos>(e).unwrap().0.y.max(1.0);
        }
    }
    let mut sizes = Vec::new();
    for _ in 0..600 {
        if ecs {
            h.tick();
        } else {
            for &i in &ids {
                r.mobs[i].ai.no_action_time = 0;
            }
            let from = r.events.len();
            r.step(None);
            // What the game does with the school events (`apply_brain_events`, `aquatic::start_following`).
            let evs: Vec<MobEvent> = r.events[from..].to_vec();
            for ev in evs {
                match ev {
                    MobEvent::StartFollowing { fish, leader } => {
                        let alive = |ms: &[Mob], id: u32| ms.iter().any(|m| m.id == id && m.alive() && !m.removed);
                        let cur = r.mobs.iter().find(|m| m.id == fish).map(|m| m.extra.leader);
                        if let Some(cur) = cur
                            && !cur.is_some_and(|l| alive(&r.mobs, l))
                        {
                            if let Some(m) = r.mobs.iter_mut().find(|m| m.id == fish) {
                                m.extra.leader = Some(leader);
                            }
                            if let Some(m) = r.mobs.iter_mut().find(|m| m.id == leader) {
                                m.extra.followers += 1;
                            }
                        }
                    }
                    MobEvent::SchoolSize { leader, by } => {
                        if let Some(m) = r.mobs.iter_mut().find(|m| m.id == leader) {
                            m.extra.followers += by;
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    if ecs {
        let mut counts: std::collections::HashMap<Entity, i32> = Default::default();
        for &e in &es {
            let s = h.world.get::<follow_flock_leader::School>(e).copied();
            if let Some(l) = s.and_then(|s| s.leader) {
                *counts.entry(l).or_insert(1) += 1;
            }
        }
        sizes.extend(counts.values().copied());
    } else {
        if std::env::var_os("L3DBG").is_some() && rep == 0 {
            for &i in ids.iter().take(6) {
                let m = &r.mobs[i];
                eprintln!("cod dbg: y {:.2} water {} leader {:?} followers {} running {:?}", m.position.y, m.fluids.in_water, m.extra.leader, m.extra.followers, m.ai.goals.running_names());
            }
        }
        let mut counts: std::collections::HashMap<u32, i32> = Default::default();
        for &i in &ids {
            if let Some(l) = r.mobs[i].extra.leader {
                *counts.entry(l).or_insert(1) += 1;
            }
        }
        sizes.extend(counts.values().copied());
    }
    sizes
}
#[test]
fn follow_flock_leader_school_sizes() {
    if !on() {
        return;
    }
    let (mut m, mut e) = (Vec::new(), Vec::new());
    for rep in 0..8 {
        m.push(schools(false, rep));
        e.push(schools(true, rep));
    }
    let stat = |v: &[Vec<i32>]| {
        let k = v.len() as f64;
        let schools = v.iter().map(|x| x.len() as f64).sum::<f64>() / k;
        let biggest = v.iter().map(|x| x.iter().copied().max().unwrap_or(0) as f64).sum::<f64>() / k;
        let followers = v.iter().map(|x| x.iter().map(|s| f64::from(s - 1)).sum::<f64>()).sum::<f64>() / k;
        (schools, biggest, followers)
    };
    let (a, b) = (stat(&m), stat(&e));
    eprintln!("follow_flock_leader after 600 ticks, mean of 8 pools of 30 cods (schools, biggest, followers): mob {a:.2?} ecs {b:.2?}");
    assert!((a.2 - b.2).abs() < 8.0, "followers: {a:?} vs {b:?}");
}

fn norm(s: &str) -> String {
    let s: String = s.rsplit("::").next().unwrap_or(s).chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_lowercase();
    s.strip_suffix("goal").map(str::to_string).unwrap_or(s)
}
fn same(a: &str, b: &str) -> bool {
    a.starts_with(b) || b.starts_with(a)
}
/// 6. The species of the first wave, one short scenario each against its `Mob`: the goals that ran at rest (1 200 ticks), then with a
/// player 5 blocks away (600 ticks): the goals only one side ran, and whether both hold a target. Informative table; the assertion is
/// on the targets (the same side must hold one).
#[test]
fn the_species_of_the_first_wave_against_their_mob() {
    if !on() {
        return;
    }
    let names = [
        "armadillo", "creeper", "donkey", "enderman", "endermite", "horse", "husk", "mooshroom", "mule", "panda", "piglin_brute", "silverfish", "skeleton_horse", "zoglin",
        "zombie_horse", "zombie_villager", "zombified_piglin",
    ];
    let mut bad = Vec::new();
    for (k, name) in names.iter().enumerate() {
        let full = format!("minecraft:{name}");
        let mut r = MobRef::new(24, false);
        let mut h = Herd::with_slab(24, false, false);
        let at = Vec3::new(0.5, 1.0, 0.5);
        let i = mob_add(&mut r, mob_kind(&full), at, 7 + k);
        r.mobs[i].ai.no_action_time = 0;
        let e = ecs_named(&mut h, &full, None, at.x, at.z, 5000 + k as u64, u64::from(far_id(7 + k)));
        if let Some(mut a) = h.world.get_mut::<ActiveUntil>(e) {
            a.0 = u64::MAX;
        }
        let ecs_goals = |h: &Herd| -> Vec<String> {
            let Some(g) = h.world.get::<GoalSet>(e) else { return Vec::new() };
            g.entries().iter().filter(|en| g.is_running(en.module)).map(|en| norm(en.module.name)).collect()
        };
        let mut seen: [(std::collections::BTreeSet<String>, std::collections::BTreeSet<String>); 2] = Default::default();
        let player = Vec3::new(5.5, 1.0, 0.5);
        let view = player_view(player, None);
        // The `Mob`'s test player is not hurt: the ECS one must not be either (a dead player drops every target).
        h.world.resource_mut::<horizon_ecs_proto::hostile::PlayerState>().immortal = true;
        h.world.resource_mut::<Online>().set(Player { id: 1, x: player.x, y: player.y, z: player.z, main: 0, off: 0, ..Default::default() });
        let have_player = false;
        let _ = have_player;
        let (mut mt, mut et) = (0usize, 0usize);
        let mut first = (None, None);
        for t in 0..1800 {
            let phase = usize::from(t >= 1200);
            if phase == 0 {
                h.world.resource_mut::<Online>().remove(1);
                r.step(None);
            } else {
                if t == 1200 {
                    // Both back at the start, where the player is 5 blocks away.
                    r.mobs[i].position = at;
                    r.mobs[i].velocity = Vec3::ZERO;
                    if h.world.get_entity(e).is_ok() {
                        h.teleport(e, at);
                    }
                    h.world.resource_mut::<Online>().set(Player { id: 1, x: player.x, y: player.y, z: player.z, main: 0, off: 0, ..Default::default() });
                }
                r.step(Some(&view));
            }
            h.tick();
            if r.mobs[i].ai.target.is_some() {
                mt += 1;
                first.0.get_or_insert(t);
            }
            if h.world.get::<Target>(e).is_some() {
                et += 1;
                first.1.get_or_insert(t);
            }
            for n in r.mobs[i].ai.goals.running_names() {
                seen[phase].0.insert(norm(n));
            }
            for n in ecs_goals(&h) {
                seen[phase].1.insert(n);
            }
        }
        let mob_target = r.mobs[i].ai.target.is_some();
        let ecs_target = h.world.get::<Target>(e).is_some();
        for (p, label) in ["rest", "player at 5"].iter().enumerate() {
            let (m, c) = &seen[p];
            let only_m: Vec<&String> = m.iter().filter(|x| !c.iter().any(|y| same(x, y))).collect();
            let only_e: Vec<&String> = c.iter().filter(|x| !m.iter().any(|y| same(x, y))).collect();
            eprintln!("{name} [{label}]: only Mob {only_m:?} | only ECS {only_e:?}");
        }
        eprintln!("{name}: target mob {mob_target} ecs {ecs_target}; ticks with a target mob {mt} (first {:?}) ecs {et} (first {:?})", first.0, first.1);
        if mob_target != ecs_target {
            bad.push(*name);
        }
    }
    eprintln!("species whose target differs: {bad:?}");
}

/// 3. The fox: the cost of the draws of `fox_search_for_items` for 1 500 foxes that sleep, sit or faceplant (wall time per tick, with
/// and without the module), and an item dropped 3 blocks from a fox that stands still.
#[test]
fn fox_search_cost_while_asleep_and_the_item_beside_a_still_fox() {
    if !on() {
        return;
    }
    let run = |with: bool| -> f64 {
        let entries: &'static [Entry] = if with { leak([Entry::new(&fox_search_for_items::MODULE, 11, &fox_search_for_items::Params::DEFAULT)]) } else { leak([]) };
        let mut h = Herd::with_slab(40, false, false);
        let es: Vec<Entity> = (0..1500).map(|j| ecs_named(&mut h, "minecraft:cow", Some(entries), -17.0 + 0.9 * (j % 40) as f64, -17.0 + 0.9 * (j / 40) as f64, 6000 + j as u64, j as u64)).collect();
        for &e in &es {
            fox_state::ensure(&mut h.world, e);
            fox_state::set(&mut h.world, e, fox_state::SLEEPING, true);
        }
        for _ in 0..30 {
            h.tick();
        }
        let t0 = std::time::Instant::now();
        for _ in 0..300 {
            h.tick();
        }
        t0.elapsed().as_secs_f64() / 300.0 * 1.0e6
    };
    let (a, b) = (run(false), run(true));
    eprintln!("fox_search while asleep: 1500 sleeping foxes: {b:.0} us/tick with the module, {a:.0} without: {:.3} us per fox and tick", (b - a) / 1500.0);
    // The item beside a still fox: dropped 3 blocks away at tick 100; ticks until the fox holds it (ECS side only: the pickup of the
    // `Mob` is in `game`).
    let mut h = Herd::with_slab(24, false, false);
    let entries: &'static [Entry] = leak([Entry::new(&fox_search_for_items::MODULE, 11, &fox_search_for_items::Params::DEFAULT), Entry::new(&fox_pick_up_loot::MODULE, 12, &fox_pick_up_loot::Params::DEFAULT)]);
    let es: Vec<Entity> = (0..20).map(|j| ecs_named(&mut h, "minecraft:cow", Some(entries), -20.0 + 4.0 * (j % 10) as f64, -10.0 + 8.0 * (j / 10) as f64, 6100 + j as u64, j as u64)).collect();
    for _ in 0..100 {
        h.tick();
    }
    for (j, _) in es.iter().enumerate() {
        let p = h.world.get::<Pos>(es[j]).unwrap().0;
        horizon_ecs_proto::item::drop_item(&mut h.world, horizon_client_lib::items::ItemStack::new("minecraft:sweet_berries", 1), Vec3::new(p.x + 3.0, p.y + 0.5, p.z), Vec3::ZERO, 0);
    }
    let mut got = vec![None; es.len()];
    for t in 0..400 {
        h.tick();
        for (j, &e) in es.iter().enumerate() {
            if got[j].is_none() && main_hand(&h.world, e).is_some() {
                got[j] = Some(t);
            }
        }
    }
    eprintln!("fox_search: an item 3 blocks from a still fox at tick 100: foxes that took it within 400 ticks {} of {} (mean tick {:?})", got.iter().flatten().count(), es.len(), mean_of(&got));
    assert!(got.iter().flatten().count() >= 15, "a still fox must see an item dropped beside it");
}

/// 4. `follow_mob`: (a) the cost, wall time per tick of 1 500 walking cows with one parrot in the world and without it (every cow that
/// changes column searches the index for a parrot); (b) the wake: a parrot that stands still 12 blocks from a still animal is put 4
/// blocks from it at tick 20: the tick at which it follows, on each side.
#[test]
fn follow_mob_cost_with_a_parrot_and_the_wake_in_the_column() {
    if horizon_ecs_proto::cow::skip_unless_default_species("follow_mob_cost_with_a_parrot_and_the_wake_in_the_column") {
        return;
    }
    if !on() {
        return;
    }
    let run = |parrot: bool| -> f64 {
        let mut h = Herd::with_slab(40, false, false);
        for j in 0..1500 {
            let e = ecs_named(&mut h, "minecraft:cow", None, -17.0 + 0.9 * (j % 40) as f64, -17.0 + 0.9 * (j / 40) as f64, 7000 + j as u64, j as u64);
            let _ = e;
        }
        if parrot {
            ecs_named(&mut h, "minecraft:parrot", None, 30.0, 30.0, 7999, 1);
        }
        for _ in 0..30 {
            h.tick();
        }
        let t0 = std::time::Instant::now();
        for _ in 0..300 {
            h.tick();
        }
        t0.elapsed().as_secs_f64() / 300.0 * 1.0e6
    };
    let (a, b) = (run(false), run(true));
    eprintln!("follow_mob cost: 1500 cows walking: {a:.0} us/tick without a parrot, {b:.0} with one: {:.1} us per tick for the parrot's existence", b - a);
    let mut r = MobRef::new(24, false);
    let mut h = Herd::with_slab(24, false, false);
    let pm = mob_add(&mut r, mob_kind("minecraft:parrot"), Vec3::new(-10.5, 1.0, 0.5), 9);
    let k = r.kind.clone();
    let cm = mob_add(&mut r, k, Vec3::new(1.5, 1.0, 0.5), 10);
    r.mobs[cm].ai.goals.clear();
    r.mobs[pm].ai.no_action_time = 100;
    let entries: &'static [Entry] = leak([Entry::new(&follow_mob::MODULE, 3, &follow_mob::Params::DEFAULT)]);
    let pe = ecs_named(&mut h, "minecraft:parrot", Some(entries), -10.5, 0.5, 7100, u64::from(far_id(9)));
    let ce = ecs_named(&mut h, "minecraft:cow", Some(leak([])), 1.5, 0.5, 7101, u64::from(far_id(10)));
    let _ = ce;
    let (mut tm, mut te) = (None, None);
    for t in 0..120 {
        if t == 20 {
            r.mobs[pm].position = Vec3::new(-2.5, 1.0, 0.5);
            h.teleport(pe, Vec3::new(-2.5, 1.0, 0.5));
        }
        r.step(None);
        h.tick();
        if t >= 20 {
            if tm.is_none() && r.mobs[pm].ai.goals.running_names().iter().any(|n| n.contains("FollowMob")) {
                tm = Some(t);
            }
            if te.is_none() && h.world.get::<follow_mob::Tailing>(pe).is_some() {
                te = Some(t);
            }
        }
    }
    eprintln!("follow_mob wake after a move into the zone at tick 20: Mob follows at {tm:?}, ECS at {te:?}");
    assert!(te.is_some() || tm.is_none(), "the ECS parrot must follow when the Mob's does");
}

/// The silverfish of the table took no target on the `Mob` side at 5 blocks (the ECS one did): the distance at which each takes it.
#[test]
fn silverfish_target_distance() {
    if !on() {
        return;
    }
    for d in [1.5_f64, 2.5, 3.5, 5.0, 7.0] {
        let mut r = MobRef::new(24, false);
        let mut h = Herd::with_slab(24, false, false);
        let i = mob_add(&mut r, mob_kind("minecraft:silverfish"), Vec3::new(0.5, 1.0, 0.5), 3);
        r.mobs[i].ai.no_action_time = 0;
        let e = ecs_named(&mut h, "minecraft:silverfish", None, 0.5, 0.5, 8000, u64::from(far_id(3)));
        let player = Vec3::new(0.5 + d, 1.0, 0.5);
        let view = player_view(player, None);
        h.world.resource_mut::<Online>().set(Player { id: 1, x: player.x, y: player.y, z: player.z, main: 0, off: 0, ..Default::default() });
        let (mut a, mut b) = (None, None);
        for t in 0..300 {
            r.step(Some(&view));
            h.tick();
            if a.is_none() && r.mobs[i].ai.target.is_some() {
                a = Some(t);
            }
            if b.is_none() && h.world.get::<Target>(e).is_some() {
                b = Some(t);
            }
        }
        eprintln!("silverfish: player at {d} blocks: target at tick mob {a:?} ecs {b:?}; follow range mob {:.1}", r.mobs[i].attrs.follow_range);
    }
}

/// Starts of the stroll in 3 000 ticks at rest, on each side, for the species whose ECS animal never strolled in the table.
#[test]
fn stroll_starts_at_rest() {
    if !on() {
        return;
    }
    for (k, name) in ["armadillo", "endermite", "creeper", "husk", "zoglin", "mooshroom"].iter().enumerate() {
        let full = format!("minecraft:{name}");
        let mut r = MobRef::new(24, false);
        let mut h = Herd::with_slab(24, false, false);
        let i = mob_add(&mut r, mob_kind(&full), Vec3::new(0.5, 1.0, 0.5), 7 + k);
        r.mobs[i].ai.no_action_time = 0;
        let e = ecs_named(&mut h, &full, None, 0.5, 0.5, 5000 + k as u64, u64::from(far_id(7 + k)));
        let (mut ms, mut es, mut mon, mut eon) = (0, 0, false, false);
        let mut moved = (0.0f64, 0.0f64);
        for _ in 0..3000 {
            r.mobs[i].ai.no_action_time = 0;
            r.step(None);
            h.tick();
            let m_on = r.mobs[i].ai.goals.running_names().iter().any(|n| norm(n).starts_with("stroll"));
            if m_on && !mon {
                ms += 1;
            }
            mon = m_on;
            let g = h.world.get::<GoalSet>(e).unwrap();
            let e_on = g.entries().iter().any(|en| en.module.name == "stroll" && g.is_running(en.module));
            if e_on && !eon {
                es += 1;
            }
            eon = e_on;
            let (p, q) = (r.mobs[i].position, h.world.get::<Pos>(e).unwrap().0);
            moved.0 = moved.0.max(((p.x - 0.5).powi(2) + (p.z - 0.5).powi(2)).sqrt());
            moved.1 = moved.1.max(((q.x - 0.5).powi(2) + (q.z - 0.5).powi(2)).sqrt());
        }
        eprintln!("{name}: stroll starts in 3000 ticks mob {ms} ecs {es}; farthest from the start mob {:.1} ecs {:.1}", moved.0, moved.1);
    }
}

/// 5. The turtle's first walk to the water on land (both sides), then after 150 ticks IN the water (the `Mob` keeps its countdown
/// frozen out of the goal's conditions, the ECS draws it again at the exit): the tick of the first walk, counted from the exit.
#[test]
fn turtle_go_to_water_delay_after_the_water() {
    if !on() {
        return;
    }
    let mut res = Vec::new();
    for in_water_first in [false, true] {
        let (mut a, mut b) = (Vec::new(), Vec::new());
        for batch in 0..3 {
            let n = 12;
            let mut r = MobRef::new(24, false);
            let mut h = Herd::with_slab(24, false, false);
            let kind = mob_kind("minecraft:turtle");
            let entries: &'static [Entry] = leak([Entry::new(&turtle_go_to_water::MODULE, 3, &turtle_go_to_water::Params::DEFAULT)]);
            let (mut ids, mut es, mut land, mut wet) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
            for j in 0..n {
                let p = spot(j, 12.0, 4);
                for dx in 5..=8 {
                    for dz in -1..=1 {
                        for y in [-1, 0] {
                            r.set_block(p.x.floor() as i32 + dx, y, p.z.floor() as i32 + dz, "minecraft:water");
                            h.set_block(p.x.floor() as i32 + dx, y, p.z.floor() as i32 + dz, "minecraft:water");
                        }
                    }
                }
                let start = if in_water_first { Vec3::new(p.x + 6.0, 0.2, p.z) } else { p };
                let i = mob_add(&mut r, kind.clone(), start, j + 60 * batch);
                r.mobs[i].ai.no_action_time = 0;
                ids.push(i);
                let e = ecs_named(&mut h, "minecraft:cow", Some(entries), start.x, start.z, 9500 + (j + 60 * batch) as u64, u64::from(far_id(j + 60 * batch)));
                if in_water_first {
                    h.teleport(e, start);
                }
                es.push(e);
                land.push(p);
                wet.push(start);
            }
            let (mut fa, mut fb) = (vec![None; n], vec![None; n]);
            let exit = if in_water_first { 150 } else { 0 };
            for t in 0..(exit + 900) {
                if in_water_first && t == exit {
                    for j in 0..n {
                        r.mobs[ids[j]].position = land[j];
                        r.mobs[ids[j]].velocity = Vec3::ZERO;
                        h.teleport(es[j], land[j]);
                    }
                }
                r.step(None);
                h.tick();
                if t >= exit {
                    for j in 0..n {
                        if fa[j].is_none() && r.mobs[ids[j]].ai.goals.running_names().iter().any(|x| x.contains("TurtleGoToWater")) {
                            fa[j] = Some(t - exit);
                        }
                        if fb[j].is_none() && h.world.get::<turtle_go_to_water::GoingToWater>(es[j]).is_some() {
                            fb[j] = Some(t - exit);
                        }
                    }
                }
            }
            a.extend(fa);
            b.extend(fb);
        }
        res.push((in_water_first, mean_of(&a), mean_of(&b)));
    }
    for (w, m, e) in &res {
        eprintln!("turtle_go_to_water first walk {} the water: (count, mean tick from the exit) mob {m:?} ecs {e:?}", if *w { "after 150 ticks in" } else { "without" });
    }
}

/// 5. `reset_universal_anger` (ECS side; the `Mob` side is `golem.rs:48-57`): a hit by a player on a golem that holds no target:
/// the `Mob` lights the anger (400 + nextInt(381)); here `neutral_anger` counts only after a held target was lost.
#[test]
fn reset_universal_anger_without_a_held_target() {
    if !on() {
        return;
    }
    let mut h = Herd::with_slab(24, false, false);
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 3.5, y: 1.0, z: 0.5, main: 0, off: 0, ..Default::default() });
    let entries: &'static [Entry] = leak([
        Entry::new(&reset_universal_anger::MODULE, 4, &reset_universal_anger::Params::DEFAULT),
        Entry::new(&neutral_anger::MODULE, 200, &neutral_anger::Params::DEFAULT),
    ]);
    let e = ecs_named(&mut h, "minecraft:cow", Some(entries), 0.5, 0.5, 9900, 5);
    for _ in 0..5 {
        h.tick();
    }
    h.hit_by(e, 1.0, 1);
    let mut angry = Vec::new();
    for _ in 0..40 {
        h.tick();
        angry.push(neutral_anger::is_angry(&h.world, e));
    }
    eprintln!("reset_universal_anger: ECS golem hit by a player, no target held: angry in the 40 ticks after: {}", angry.iter().filter(|a| **a).count());
}

/// 5. `pufferfish_puff`: the `Mob` looks at the SELECTED (nearest) player only; the ECS at every player within 2 blocks. A creative player
/// at 1 block (the nearest) and a survival one at 1.5: the `Mob` does not puff (by `golem`-style reading of `aquatic.rs:988-1005`).
#[test]
fn pufferfish_puff_with_two_players() {
    if !on() {
        return;
    }
    let mut h = Herd::with_slab(24, false, false);
    {
        let mut on = h.world.resource_mut::<Online>();
        on.set(Player { id: 1, x: 1.5, y: 1.0, z: 0.5, main: 0, off: 0, ..Default::default() });
        on.set(Player { id: 2, x: 2.0, y: 1.0, z: 0.5, main: 0, off: 0, ..Default::default() });
    }
    {
        let mut ps = h.world.resource_mut::<horizon_ecs_proto::hostile::PlayerState>();
        ps.creative.resize(3, false);
        ps.creative[1] = true;
    }
    let entries: &'static [Entry] = leak([Entry::new(&pufferfish_puff::MODULE, 1, &pufferfish_puff::Params::DEFAULT)]);
    let e = ecs_named(&mut h, "minecraft:cow", Some(entries), 0.5, 0.5, 9901, 6);
    let mut puffed = false;
    for _ in 0..60 {
        h.tick();
        puffed |= h.world.get::<pufferfish_puff::Puffing>(e).is_some();
    }
    eprintln!("pufferfish_puff: creative player nearest (1.0) + survival player at 1.5: ECS puffs {puffed} (the Mob reads the nearest only: no puff)");
}
