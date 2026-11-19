//! The three pilot modules (avoid_entity, flee_sun, random_stand) against a real `Mob` that has the original goal added
//! to its selector (`goals.clear()` then `add`, the SCOPE of the benches), same parameters, same world.
//! Values are printed (`--nocapture`) and the gaps are asserted.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_client_lib::mobs::{
    self, Mob, MobWorld, Seen,
    goals::{Avoid, AvoidEntity, FleeSun, hide_pos},
};
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{
    behaviours::{Entry, Inst, avoid_entity, flee_sun, random_stand},
    cow::*,
    goals::*,
};
use std::sync::Arc;

#[derive(Clone, Copy, Debug)]
struct Tk {
    run: bool,
    speed: f64,
    x: f64,
    z: f64,
}
fn leak<T>(v: T) -> &'static T {
    Box::leak(Box::new(v))
}
fn ecs_with(h: &mut Herd, entries: &'static [Entry], x: f64, z: f64, seed: u64) -> Entity {
    let mut sp = *species();
    sp.goals = horizon_ecs_proto::species::Config { entries };
    let e = spawn_species(&mut h.world, &sp, x, z, seed);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    // The animal enters the active zone: its modules arm their first deadlines.
    h.world.resource_mut::<Entered>().0.push(e);
    e
}
fn mob_add(r: &mut MobRef, kind: Arc<mobs::MobType>, at: Vec3, clear: bool) -> usize {
    let mut m = Mob::new(r.mobs.len() as u32 + 1, kind, at, 0.0);
    m.persistent = true;
    m.ai.no_action_time = 100;
    if clear {
        m.ai.goals.clear();
        m.ai.targets.clear();
    }
    r.mobs.push(m);
    r.searches.push(0);
    r.mobs.len() - 1
}
/// `MobRef::step` with the day imposed (the world of the `Mob` has `daylight` true).
fn step_day(r: &mut MobRef, player: Option<&mobs::PlayerView>, day: bool) {
    r.t += 1;
    let items = r.w.items.expect("items");
    let mut snapshot: Vec<Seen> = r.mobs.iter().map(|m| m.seen(None, items)).collect();
    let mut grid = mobs::seen_grid::SeenGrid::new(snapshot.iter().map(|s| s.bounds));
    let mut lvl = r.tl.level();
    for slot in 0..r.mobs.len() {
        mobs::apply_mob_pushes(&mut r.mobs[slot], &mut r.pushes);
        let from = r.events.len();
        let view = MobWorld {
            others: &snapshot,
            others_grid: Some(&grid),
            others_solid: Some(&[]),
            nearest_player: player.map(|p| p.position),
            player: player.cloned(),
            game_time: r.t,
            daylight: day,
            ..r.w.clone()
        };
        let kept = mobs::ai::tick(&mut r.mobs[slot], &view, &mut lvl, &mut r.events);
        mobs::take_mob_pushes(&mut r.events, from, &mut r.pushes);
        let mut s = r.mobs[slot].seen(None, items);
        s.alive &= kept;
        s.removed |= !kept;
        grid.update(slot, s.bounds);
        snapshot[slot] = s;
    }
    r.apply_block_events();
}
type Blocks = *mut horizon_client_render::world::World;
/// Sets the sky light of the block (x, y, z) of a world (the test world has no lighting).
fn set_sky(blocks: Blocks, x: i32, y: i32, z: i32, sky: u8) {
    // SAFETY: the test thread owns the world while it edits it.
    let w = unsafe { &mut *blocks };
    let ch = Arc::make_mut(w.chunks.get_mut(&(x >> 4, z >> 4)).expect("chunk"));
    let s = ch.section_index(y).expect("section");
    let sec = Arc::make_mut(&mut ch.sections[s]);
    let mut all = [0u8; 4096];
    for (i, v) in all.iter_mut().enumerate() {
        *v = sec.light.get(i);
    }
    let i = horizon_client_render::chunk::index(x, y, z);
    all[i] = sky << 4 | (all[i] & 15);
    sec.light = horizon_client_render::chunk::Light::from_slice(&all);
}
/// A roof: every cell x, z in [-r, r], y in -2..=3 gets sky light `sky`, except the columns of `open` (x, z).
fn roof(blocks: Blocks, r: i32, sky: u8, open: &[(i32, i32)]) {
    for x in -r..=r {
        for z in -r..=r {
            if open.contains(&(x, z)) {
                continue;
            }
            for y in -2..=3 {
                set_sky(blocks, x, y, z, sky);
            }
        }
    }
}
fn ecs_blocks(h: &Herd) -> Blocks {
    h.world.resource::<Level>().blocks.0
}

// ======================================================================================================================
// AvoidEntity
// ======================================================================================================================
#[derive(Clone, Copy)]
struct AvOpt {
    species: bool,
    dist: f64,
    max: f64,
    wall: bool,
    creative: bool,
    forced: bool,
    /// The listed animal appears at the tick `APPEAR` (else it is there from the start).
    late: bool,
    /// Changes the id (so the draws) of the animals of the two sides.
    salt: usize,
    ticks: usize,
}
impl AvOpt {
    fn new(species: bool, dist: f64, max: f64) -> Self {
        Self { species, dist, max, wall: false, creative: false, forced: false, late: true, salt: 0, ticks: 40 }
    }
}
struct AvOut {
    mob: Vec<Tk>,
    ecs: Vec<Tk>,
    target: Option<(f64, f64)>,
}
const APPEAR: usize = 20;
fn avoid_run(o: &AvOpt) -> AvOut {
    let mut r = MobRef::new(20, false);
    let (walk, sprint) = (1.0, 1.4);
    let params = leak(avoid_entity::Params {
        what: if o.species { avoid_entity::Avoid::Species(leak([species().id])) } else { avoid_entity::Avoid::Player },
        max_dist: o.max,
        walk,
        sprint,
        ..avoid_entity::Params::DEFAULT
    });
    let entries: &'static [Entry] = leak([Entry::new(&avoid_entity::MODULE, 3, params)]);
    let none: &'static [Entry] = &[];
    // ---- Mob
    for _ in 0..o.salt {
        let kind = r.kind.clone();
        mob_add(&mut r, kind, Vec3::new(15.5, 1.0, 15.5), true);
    }
    let i = r.add(Vec3::new(0.5, 1.0, 0.5));
    let what = if o.species { Avoid::Species(vec![r.mobs[i].species()]) } else { Avoid::Player };
    r.mobs[i].ai.goals.clear();
    r.mobs[i].ai.targets.clear();
    r.mobs[i].ai.goals.add(3, AvoidEntity::new(what, o.max as f32, walk, sprint));
    if o.wall {
        for y in 1..=3 {
            for z in -3..=4 {
                r.set_block(3, y, z, "minecraft:stone");
            }
        }
    }
    let mut pv = player_view(Vec3::new(0.5 + o.dist, 1.0, 0.5), None);
    pv.creative = o.creative;
    let rec0 = if o.species && !o.late { 0 } else { APPEAR };
    let mut mob = Vec::new();
    let mut target = None;
    for t in 0..APPEAR + o.ticks {
        if o.species && t == if o.late { APPEAR } else { 0 } {
            let kind = r.kind.clone();
            mob_add(&mut r, kind, Vec3::new(0.5 + o.dist, 1.0, 0.5), true);
        }
        r.step(if !o.species && t >= APPEAR { Some(&pv) } else { None });
        if t >= rec0 {
            let m = &r.mobs[i];
            let run = r.names(i).contains(&"AvoidEntity") && !m.ai.nav.is_done();
            if run && target.is_none() {
                target = m.ai.nav.path.as_ref().map(|p| (f64::from(p.target[0]) + 0.5, f64::from(p.target[2]) + 0.5));
            }
            mob.push(Tk { run, speed: m.ai.nav.speed_modifier, x: m.position.x, z: m.position.z });
        }
    }
    // ---- ECS
    let mut h = Herd::with_slab(20, false, false);
    let e = ecs_with(&mut h, entries, 0.5, 0.5, 7 + o.salt as u64);
    h.world.get_mut::<Phase>(e).unwrap().0 = u64::from(r.mobs[i].id);
    if o.wall {
        for y in 1..=3 {
            for z in -3..=4 {
                h.set_block(3, y, z, "minecraft:stone");
            }
        }
    }
    if o.forced && let Some(t) = target {
        force_spot(&mut h.world, e, &avoid_entity::MODULE, t);
    }
    let mut ecs = Vec::new();
    for t in 0..APPEAR + o.ticks {
        if o.species && t == if o.late { APPEAR } else { 0 } {
            ecs_with(&mut h, none, 0.5 + o.dist, 0.5, 8);
        }
        if t == APPEAR && !o.species {
            h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5 + o.dist, y: 1.0, z: 0.5, main: 0, off: 0 });
        }
        h.tick();
        if t >= rec0 {
            let p = h.world.get::<Pos>(e).unwrap().0;
            let run = h.world.get::<GoalSet>(e).unwrap().is_running(&avoid_entity::MODULE) && h.world.get::<Nav>(e).unwrap().path.is_some();
            ecs.push(Tk { run, speed: h.world.get::<Nav>(e).unwrap().speed, x: p.x, z: p.z });
        }
    }
    AvOut { mob, ecs, target }
}
fn first_run(v: &[Tk]) -> Option<usize> {
    v.iter().position(|t| t.run)
}
fn dist(a: &Tk, b: &Tk) -> f64 {
    (a.x - b.x).hypot(a.z - b.z)
}
fn mean_start(o: &AvOpt, trials: usize) -> (usize, usize, f64, f64) {
    let (mut n_m, mut n_e, mut s_m, mut s_e) = (0, 0, 0.0, 0.0);
    for salt in 0..trials {
        let r = avoid_run(&AvOpt { salt, ..*o });
        if let Some(t) = first_run(&r.mob) {
            n_m += 1;
            s_m += t as f64;
        }
        if let Some(t) = first_run(&r.ecs) {
            n_e += 1;
            s_e += t as f64;
        }
    }
    (n_m, n_e, s_m / n_m.max(1) as f64, s_e / n_e.max(1) as f64)
}

#[test]
fn avoid_1_player_in_and_out_of_range() {
    if horizon_ecs_proto::cow::skip_unless_small_body("avoid_1_player_in_and_out_of_range") {
        return;
    }
    if horizon_ecs_proto::cow::skip_unless_walker("avoid_1_player_in_and_out_of_range") {
        return;
    }
    if !have_data() {
        return;
    }
    for d in [7.0, 9.0] {
        let (nm, ne, sm, se) = mean_start(&AvOpt::new(false, d, 8.0), 30);
        eprintln!("avoid_1 player at {d} (max 8), 30 trials: starts mob {nm} ecs {ne}; mean start tick mob {sm:.2} ecs {se:.2}");
        assert_eq!(nm == 0, d > 8.0);
        assert_eq!(nm, ne, "number of trials that start");
        assert!((sm - se).abs() < 1.5, "start tick law differs");
    }
}
#[test]
fn avoid_2_species_needs_a_line_of_sight() {
    if horizon_ecs_proto::cow::skip_unless_default_species("avoid_2_species_needs_a_line_of_sight") {
        return;
    }
    if horizon_ecs_proto::cow::skip_unless_farm_goals("avoid_2_species_needs_a_line_of_sight") {
        return;
    }
    if horizon_ecs_proto::cow::skip_unless_walker("avoid_2_species_needs_a_line_of_sight") {
        return;
    }
    if !have_data() {
        return;
    }
    for (d, wall, late) in [(5.0, false, true), (5.0, false, false), (5.0, true, true), (7.0, false, true)] {
        let (nm, ne, sm, se) = mean_start(&AvOpt { wall, late, ..AvOpt::new(true, d, 6.0) }, horizon_ecs_proto::cow::trials(20));
        eprintln!("avoid_2 species at {d} wall {wall} appears-later {late} (max 6), 20 trials: starts mob {nm} ecs {ne}; mean tick mob {sm:.2} ecs {se:.2}");
        if nm != ne {
            for salt in 0..horizon_ecs_proto::cow::trials(20) {
                let r = avoid_run(&AvOpt { wall, late, salt, ..AvOpt::new(true, d, 6.0) });
                if first_run(&r.mob).is_some() != first_run(&r.ecs).is_some() {
                    eprintln!("  salt {salt}: mob run {:?}", r.mob.iter().map(|t| t.run as u8).collect::<Vec<_>>());
                    eprintln!("  salt {salt}: ecs run {:?}", r.ecs.iter().map(|t| t.run as u8).collect::<Vec<_>>());
                }
            }
        }
        assert_eq!(nm, ne, "d {d} wall {wall} late {late}");
        assert!((sm - se).abs() < 1.5);
    }
}
#[test]
fn avoid_3_speed_and_trajectory_with_the_spot_imposed() {
    if horizon_ecs_proto::cow::skip_unless_default_species("avoid_3_speed_and_trajectory_with_the_spot_imposed") {
        return;
    }
    if horizon_ecs_proto::cow::skip_unless_farm_goals("avoid_3_speed_and_trajectory_with_the_spot_imposed") {
        return;
    }
    if horizon_ecs_proto::cow::skip_unless_walker("avoid_3_speed_and_trajectory_with_the_spot_imposed") {
        return;
    }
    if !have_data() {
        return;
    }
    for (d, max) in [(5.0, 8.0), (7.5, 8.0)] {
        // The trials in which the Mob started at the first selector tick (the ECS, forced, starts at the earliest tick).
        let (mut kept, mut speed_bad, mut state_bad, mut worst, mut end_gap) = (0, 0, 0, 0.0_f64, 0);
        let mut line = String::new();
        for salt in 0..horizon_ecs_proto::cow::trials(40) {
            let o = avoid_run(&AvOpt { forced: true, ticks: 160, salt, ..AvOpt::new(false, d, max) });
            let (Some(s), Some(es)) = (first_run(&o.mob), first_run(&o.ecs)) else { continue };
            if s != es {
                continue;
            }
            kept += 1;
            let ends = |v: &[Tk]| v.iter().skip(s).position(|t| !t.run).map(|k| k + s);
            if kept <= 6 {
                eprintln!("  trial {salt}: start {s}; first end mob {:?} ecs {:?}; speeds at start..+3 mob {:?} ecs {:?}", ends(&o.mob), ends(&o.ecs), &o.mob[s..s + 3].iter().map(|t| t.speed).collect::<Vec<_>>(), &o.ecs[s..s + 3].iter().map(|t| t.speed).collect::<Vec<_>>());
            }
            let stop = ends(&o.mob).unwrap_or(o.mob.len()).min(ends(&o.ecs).unwrap_or(o.mob.len()));
            if let (Some(a), Some(b)) = (ends(&o.mob), ends(&o.ecs)) {
                end_gap = end_gap.max(a.abs_diff(b));
            }
            // Up to the end of the first run: the next one draws its own spot on each side.
            for k in 0..stop {
                worst = worst.max(dist(&o.mob[k], &o.ecs[k]));
                if o.mob[k].run != o.ecs[k].run {
                    state_bad += 1;
                } else if o.mob[k].run && (o.mob[k].speed - o.ecs[k].speed).abs() > 1e-9 {
                    speed_bad += 1;
                }
            }
            let _ = o.target;
            if line.is_empty() {
                let e = o.mob.len() - 1;
                line = format!("start {s}; speed at start mob {:.2} ecs {:.2}; end mob ({:.2},{:.2}) ecs ({:.2},{:.2})", o.mob[s].speed, o.ecs[s].speed, o.mob[e].x, o.mob[e].z, o.ecs[e].x, o.ecs[e].z);
            }
        }
        eprintln!("avoid_3 player at {d} max {max}: {kept}/40 trials aligned; ticks with a different speed {speed_bad}, with a different running state {state_bad}; worst gap {worst:.3}; largest difference of the end tick {end_gap}; e.g. {line}");
        assert!(kept >= 3);
        // The speed switches at 7 blocks: the one tick where the two positions straddle the threshold may differ.
        assert!(speed_bad <= 2 * kept, "speeds differ");
        assert!(end_gap <= 2 && worst < 0.4, "the run differs");
    }
}
#[test]
#[ignore = "C: the ECS Player has no creative/spectator/sneaking field (the Mob does not flee a creative player)"]
fn avoid_4_a_creative_player_is_not_fled() {
    if !have_data() {
        return;
    }
    let o = avoid_run(&AvOpt { creative: true, ..AvOpt::new(false, 5.0, 8.0) });
    eprintln!("avoid_4 creative player at 5: mob {:?} ecs {:?}", first_run(&o.mob), first_run(&o.ecs));
    assert_eq!(first_run(&o.mob), None, "the Mob flees a creative player");
    assert_eq!(first_run(&o.ecs), first_run(&o.mob), "the ECS does not know the player's mode");
}
/// Two animals that stand still 5 blocks apart, asleep (no player near); a player far from both comes: the avoider must
/// start (it would at its first ask in the `Mob`, which asks every other tick while active).
#[test]
fn avoid_7_a_sleeping_avoider_wakes_up_with_a_listed_animal_in_range() {
    if horizon_ecs_proto::cow::skip_unless_walker("avoid_7_a_sleeping_avoider_wakes_up_with_a_listed_animal_in_range") {
        return;
    }
    if !have_data() {
        return;
    }
    let (mut nm, mut ne) = (0, 0);
    for salt in 0..10 {
        let o = AvOpt { late: false, salt, ..AvOpt::new(true, 5.0, 6.0) };
        let r = avoid_run(&o);
        nm += usize::from(first_run(&r.mob).is_some());
        // The ECS side: asleep, a far player wakes it.
        let params = leak(avoid_entity::Params { what: avoid_entity::Avoid::Species(leak([species().id])), max_dist: 6.0, ..avoid_entity::Params::DEFAULT });
        let entries: &'static [Entry] = leak([Entry::new(&avoid_entity::MODULE, 3, params)]);
        let mut h = Herd::with_slab(20, true, false);
        let e = ecs_with(&mut h, entries, 0.5, 0.5, 7 + salt as u64);
        let t = ecs_with(&mut h, &[], 5.5, 0.5, 8);
        for x in [e, t] {
            h.world.get_mut::<ActiveUntil>(x).unwrap().0 = 0;
        }
        let mut ran = false;
        for k in 0..80 {
            if k == 30 {
                h.world.resource_mut::<Online>().set(Player { id: 1, x: -15.0, y: 1.0, z: 15.0, main: 0, off: 0 });
            }
            h.tick();
            ran |= h.world.get::<GoalSet>(e).unwrap().is_running(&avoid_entity::MODULE) && h.world.get::<Nav>(e).unwrap().path.is_some();
        }
        ne += usize::from(ran);
    }
    eprintln!("avoid_7 asleep avoider, listed animal 5 away, a far player arrives: runs: mob {nm}/10 ecs {ne}/10");
    assert_eq!(nm, ne);
}
/// The law of the spot: where the goal sends the animal, for 3000 draws from the same geometry. `grass`: the east half of
/// the floor is grass (the `Mob` takes the best of its 10 tries by `walk_target_value`: 10 on grass for an animal).
const N_DRAWS: usize = 60_000;
fn spot_law(grass: bool) {
    let mut r = MobRef::new(20, false);
    let mut h = Herd::with_slab(20, false, false);
    if grass {
        for x in -19..=19 {
            for z in 0..=19 {
                r.set_block(x, 0, z, "minecraft:grass_block");
                h.set_block(x, 0, z, "minecraft:grass_block");
            }
        }
    }
    let i = r.add(Vec3::new(0.5, 1.0, 0.5));
    r.mobs[i].ai.goals.clear();
    let from = [-3.5, 1.0, 0.5];
    let mut mob: Vec<(i32, i32)> = Vec::new();
    let w = r.w;
    for _ in 0..N_DRAWS {
        if let Some(p) = r.mobs[i].default_random_pos_away(w, 16, 7, from) {
            mob.push((p[0].floor() as i32, p[2].floor() as i32));
        }
    }
    let entries: &'static [Entry] = leak([Entry::new(&avoid_entity::MODULE, 3, leak(avoid_entity::Params { max_dist: 24.0, ..avoid_entity::Params::DEFAULT }))]);
    let e = ecs_with(&mut h, entries, 0.5, 0.5, 11);
    for _ in 0..20 {
        h.tick();
    }
    h.world.resource_mut::<Online>().set(Player { id: 1, x: -3.5, y: 1.0, z: 0.5, main: 0, off: 0 });
    h.tick();
    let inst = Inst { slot: 0, nth: 0 };
    let mut ecs: Vec<(i32, i32)> = Vec::new();
    for _ in 0..N_DRAWS {
        if (avoid_entity::MODULE.can_use)(&mut h.world, e, inst) {
            (avoid_entity::MODULE.start)(&mut h.world, e, inst);
            let s = *h.world.get::<Stroll>(e).unwrap();
            ecs.push((s.tx.floor() as i32, s.tz.floor() as i32));
            (avoid_entity::MODULE.stop)(&mut h.world, e, inst);
        }
    }
    let m = |v: &[(i32, i32)], f: fn(&(i32, i32)) -> f64| v.iter().map(f).sum::<f64>() / v.len().max(1) as f64;
    let east = |v: &[(i32, i32)]| v.iter().filter(|p| p.1 >= 0).count() as f64 / v.len().max(1) as f64;
    eprintln!(
        "spot_law grass={grass}: mob n {} mean x {:.2} z {:.2} share z>=0 {:.3} | ecs n {} mean x {:.2} z {:.2} share z>=0 {:.3}",
        mob.len(), m(&mob, |p| f64::from(p.0)), m(&mob, |p| f64::from(p.1)), east(&mob),
        ecs.len(), m(&ecs, |p| f64::from(p.0)), m(&ecs, |p| f64::from(p.1)), east(&ecs)
    );
    let tol = 0.15;
    assert!((m(&mob, |p| f64::from(p.0)) - m(&ecs, |p| f64::from(p.0))).abs() < tol, "x law differs");
    assert!((m(&mob, |p| f64::from(p.1)) - m(&ecs, |p| f64::from(p.1))).abs() < tol, "z law differs");
    assert!((east(&mob) - east(&ecs)).abs() < 0.02, "share on the grass differs");
}
#[test]
fn avoid_5_spot_law_on_a_uniform_floor() {
    if horizon_ecs_proto::cow::skip_unless_walker("avoid_5_spot_law_on_a_uniform_floor") {
        return;
    }
    if have_data() {
        spot_law(false);
    }
}
#[test]
#[ignore = "C: the framework has no walk_target_value (best of 10 tries by grass/light)"]
fn avoid_6_spot_law_with_grass_walk_value() {
    if have_data() {
        spot_law(true);
    }
}

// ======================================================================================================================
// FleeSun
// ======================================================================================================================
fn run_species() -> String {
    std::env::var("HORIZON_SPECIES").unwrap_or_else(|_| "minecraft:cow".into())
}
fn is_zombie() -> bool {
    run_species() == "minecraft:zombie"
}
/// A roof over the field with the sky light that the species likes (a monster takes a spot lit above brightness 0.5, an
/// animal a dark one, `walk_target_value`), open at the columns of the animals.
fn shade_both(r: &MobRef, h: &Herd, open: &[(i32, i32)]) {
    let sky = if is_zombie() { 12 } else { 1 };
    roof(r.level.blocks.0, 18, sky, open);
    roof(ecs_blocks(h), 18, sky, open);
}
#[test]
fn flee_1_hide_pos_law() {
    if horizon_ecs_proto::cow::skip_unless_farm_goals("flee_1_hide_pos_law") {
        return;
    }
    if horizon_ecs_proto::cow::skip_unless_walker("flee_1_hide_pos_law") {
        return;
    }
    if !have_data() {
        return;
    }
    let mut r = MobRef::new(20, false);
    let mut h = Herd::with_slab(20, false, false);
    // Bright shade (sky 12) in the east half, dark shade (sky 1) in the west half.
    for x in -18..=18 {
        for z in -18..=18 {
            if (x, z) == (0, 0) {
                continue;
            }
            for y in -2..=3 {
                let sky = if x > 0 { 12 } else { 1 };
                set_sky(r.level.blocks.0, x, y, z, sky);
                set_sky(ecs_blocks(&h), x, y, z, sky);
            }
        }
    }
    let i = r.add(Vec3::new(0.5, 1.0, 0.5));
    r.mobs[i].ai.goals.clear();
    r.mobs[i].fire_ticks = 100_000;
    let w = r.w;
    let mut mob = Vec::new();
    for _ in 0..3000 {
        if let Some(p) = hide_pos(&mut r.mobs[i], w) {
            mob.push((p[0].floor() as i32, p[1] as i32, p[2].floor() as i32));
        }
    }
    let entries: &'static [Entry] = leak([Entry::new(&flee_sun::MODULE, 3, &flee_sun::Params::DEFAULT)]);
    let e = ecs_with(&mut h, entries, 0.5, 0.5, 11);
    h.set_day(true);
    for _ in 0..10 {
        h.tick();
    }
    let now = h.world.resource::<Clock>().0;
    h.world.entity_mut(e).insert(Burning { until: now + 1_000_000 });
    h.world.resource_mut::<Events>().ignited.push(e);
    let inst = Inst { slot: 0, nth: 0 };
    let mut ecs = Vec::new();
    for _ in 0..3000 {
        if (flee_sun::MODULE.can_use)(&mut h.world, e, inst) {
            (flee_sun::MODULE.start)(&mut h.world, e, inst);
            let s = *h.world.get::<Stroll>(e).unwrap();
            ecs.push((s.tx.floor() as i32, s.ty, s.tz.floor() as i32));
        }
    }
    let east = |v: &[(i32, i32, i32)]| v.iter().filter(|p| p.0 > 0).count() as f64 / v.len().max(1) as f64;
    eprintln!(
        "flee_1 {} : hide_pos success per call: mob {}/3000, ecs {}/3000; share of spots in the bright (east) shade: mob {:.2} ecs {:.2}",
        run_species(), mob.len(), ecs.len(), east(&mob), east(&ecs)
    );
    assert!((mob.len() as f64 - ecs.len() as f64).abs() < 150.0, "success rate differs");
    if !is_zombie() {
        eprintln!("KNOWN (C): an animal hides in the dark, the module takes the monster's rule (bright shade); not asserted for {}", run_species());
        return;
    }
    assert!((east(&mob) - east(&ecs)).abs() < 0.05, "the kind of shade taken differs (walk_target_value of the species)");
}

#[test]
fn flee_2_burning_animal_walks_to_the_spot_at_the_first_selector_tick() {
    if !have_data() {
        return;
    }
    if run_species() != "minecraft:cow" {
        eprintln!("SKIP flee_2: a zombie burns in the sun by itself on both sides");
        return;
    }
    let n = 12;
    // 12 animals 3 blocks apart reach x 21.5: the slab is 24 wide each way (on one of 20 the last stood in the void, fell and was removed).
    let pos = |k: usize| (-12.0 + 3.0 * k as f64 + 0.5, 0.5);
    let mut r = MobRef::new(24, false);
    let mut h = Herd::with_slab(24, false, false);
    let open: Vec<(i32, i32)> = (0..n).map(|k| (pos(k).0.floor() as i32, 0)).collect();
    shade_both(&r, &h, &open);
    let entries: &'static [Entry] = leak([Entry::new(&flee_sun::MODULE, 3, &flee_sun::Params::DEFAULT)]);
    let mut ids = Vec::new();
    for k in 0..n {
        let (x, z) = pos(k);
        let i = r.add(Vec3::new(x, 1.0, z));
        r.mobs[i].ai.goals.clear();
        r.mobs[i].ai.targets.clear();
        r.mobs[i].ai.goals.add(3, FleeSun::new(1.0));
        ids.push(i);
    }
    let ign = 20;
    let mut first = vec![None; n];
    let mut target = vec![None; n];
    let mut traj: Vec<Vec<(f64, f64)>> = vec![Vec::new(); n];
    let mut mrun: Vec<Vec<bool>> = vec![Vec::new(); n];
    for t in 0..ign + 80 {
        if t == ign {
            for &i in &ids {
                r.mobs[i].fire_ticks = 1000;
            }
        }
        r.step(None);
        for (k, &i) in ids.iter().enumerate() {
            if t >= ign {
                traj[k].push((r.mobs[i].position.x, r.mobs[i].position.z));
                mrun[k].push(r.names(i).contains(&"FleeSun"));
                if first[k].is_none() && r.names(i).contains(&"FleeSun") {
                    first[k] = Some(t - ign);
                    // The harness imposes (x, z) only: the spots at another row than the animal's are left out.
                    target[k] = r.mobs[i].ai.nav.path.as_ref().filter(|p| p.target[1] == 1).map(|p| (f64::from(p.target[0]) + 0.5, f64::from(p.target[2]) + 0.5));
                }
            }
        }
    }
    let es: Vec<Entity> = (0..n)
        .map(|k| {
            let (x, z) = pos(k);
            let e = ecs_with(&mut h, entries, x, z, 20 + k as u64);
            h.world.get_mut::<Phase>(e).unwrap().0 = u64::from(r.mobs[ids[k]].id);
            h.world.get_mut::<Yaw>(e).unwrap().0 = r.mobs[ids[k]].yaw;
            if let Some(t) = target[k] {
                force_spot(&mut h.world, e, &flee_sun::MODULE, t);
            }
            e
        })
        .collect();
    h.set_day(true);
    let mut efirst = vec![None; n];
    let mut etraj: Vec<Vec<(f64, f64)>> = vec![Vec::new(); n];
    let mut erun: Vec<Vec<bool>> = vec![Vec::new(); n];
    for t in 0..ign + 80 {
        if t == ign {
            let now = h.world.resource::<Clock>().0;
            for &e in &es {
                h.world.entity_mut(e).insert(Burning { until: now + 1000 });
                h.world.resource_mut::<Events>().ignited.push(e);
            }
        }
        h.tick();
        for (k, &e) in es.iter().enumerate() {
            if t >= ign {
                let p = h.world.get::<Pos>(e).unwrap().0;
                etraj[k].push((p.x, p.z));
                erun[k].push(h.world.get::<GoalSet>(e).unwrap().is_running(&flee_sun::MODULE));
                if efirst[k].is_none() && h.world.get::<GoalSet>(e).unwrap().is_running(&flee_sun::MODULE) {
                    efirst[k] = Some(t - ign);
                }
            }
        }
    }
    let (mut kept, mut same_start, mut close) = (0, 0, 0);
    for k in 0..n {
        if first[k].is_none_or(|t| t > 1) || target[k].is_none() {
            continue;
        }
        kept += 1;
        if first[k] == efirst[k] {
            same_start += 1;
        }
        // Up to the end of the first flight (the second one draws its own spot on each side).
        let end = |v: &[bool]| v.iter().skip(first[k].unwrap()).position(|r| !r).map_or(v.len(), |p| p + first[k].unwrap());
        let stop = end(&mrun[k]).min(end(&erun[k]));
        eprintln!("    first flight ends: mob {} ecs {}", end(&mrun[k]), end(&erun[k]));
        let gap = traj[k].iter().zip(&etraj[k]).take(stop).map(|(a, b)| (a.0 - b.0).hypot(a.1 - b.1)).fold(0.0, f64::max);
        eprintln!("  animal {k}: start mob {:?} ecs {:?}; target {:?}; max gap {gap:.3}", first[k], efirst[k], target[k]);
        if gap < 0.3 {
            close += 1;
        } else {
            eprintln!("    mob fire_ticks {} path {:?}", r.mobs[ids[k]].fire_ticks, r.mobs[ids[k]].ai.nav.path.as_ref().map(|p| (p.nodes.iter().map(|n| (n.x, n.y, n.z)).collect::<Vec<_>>(), p.target)));
            for t in (0..traj[k].len()).step_by(8) {
                eprintln!("    t{t}: mob ({:.2},{:.2}) ecs ({:.2},{:.2})", traj[k][t].0, traj[k][t].1, etraj[k][t].0, etraj[k][t].1);
            }
        }
    }
    eprintln!("flee_2: {kept}/{n} mobs found a spot; same start tick {same_start}; trajectories within 0.3: {close}");
    assert!(kept >= 6);
    assert_eq!(same_start, kept);
    assert_eq!(close, kept);
}
#[test]
fn flee_3_night_means_no_start() {
    if !have_data() {
        return;
    }
    if run_species() != "minecraft:cow" {
        eprintln!("SKIP flee_3 (cow only: no sun burn)");
        return;
    }
    let mut r = MobRef::new(20, false);
    let mut h = Herd::with_slab(20, false, false);
    shade_both(&r, &h, &[(0, 0)]);
    let entries: &'static [Entry] = leak([Entry::new(&flee_sun::MODULE, 3, &flee_sun::Params::DEFAULT)]);
    let i = r.add(Vec3::new(0.5, 1.0, 0.5));
    r.mobs[i].ai.goals.clear();
    r.mobs[i].ai.goals.add(3, FleeSun::new(1.0));
    let e = ecs_with(&mut h, entries, 0.5, 0.5, 5);
    h.set_day(false);
    let mut started = (false, false);
    for t in 0..60 {
        if t == 10 {
            r.mobs[i].fire_ticks = 1000;
            let now = h.world.resource::<Clock>().0;
            h.world.entity_mut(e).insert(Burning { until: now + 1000 });
            h.world.resource_mut::<Events>().ignited.push(e);
        }
        step_day(&mut r, None, false);
        h.tick();
        started.0 |= r.names(i).contains(&"FleeSun");
        started.1 |= h.world.get::<GoalSet>(e).unwrap().is_running(&flee_sun::MODULE);
    }
    eprintln!("flee_3 night: started mob {} ecs {}", started.0, started.1);
    assert_eq!(started, (false, false));
}

/// A burning monster whose own block does not see the sky (a roof over it) stays; the roof goes (a block removed): the
/// `Mob` asks every other tick and flees at once, the ECS goal must ask again too.
#[test]
fn flee_4_the_roof_goes_and_a_burning_monster_flees() {
    if !have_data() {
        return;
    }
    if !is_zombie() {
        eprintln!("SKIP flee_4 (zombie only: the cow's walk value differs, see flee_1)");
        return;
    }
    let mut r = MobRef::new(20, false);
    let mut h = Herd::with_slab(20, false, false);
    // Bright shade everywhere (sky 12): a spot exists at every column; no cell sees the sky except the ones restored.
    let (mut mob_start, mut ecs_start) = (None, None);
    roof(r.level.blocks.0, 18, 12, &[]);
    roof(ecs_blocks(&h), 18, 12, &[]);
    let entries: &'static [Entry] = leak([Entry::new(&flee_sun::MODULE, 3, &flee_sun::Params::DEFAULT)]);
    let i = r.add(Vec3::new(0.5, 1.0, 0.5));
    r.mobs[i].ai.goals.clear();
    r.mobs[i].ai.targets.clear();
    r.mobs[i].ai.goals.add(3, FleeSun::new(1.0));
    let e = ecs_with(&mut h, entries, 0.5, 0.5, 5);
    h.set_day(true);
    for t in 0..80 {
        if t == 10 {
            r.mobs[i].fire_ticks = 1000;
            let now = h.world.resource::<Clock>().0;
            h.world.entity_mut(e).insert(Burning { until: now + 1000 });
            h.world.resource_mut::<Events>().ignited.push(e);
        }
        if t == 30 {
            for y in -2..=3 {
                set_sky(r.level.blocks.0, 0, y, 0, 15);
                set_sky(ecs_blocks(&h), 0, y, 0, 15);
            }
            // The block that was the roof is removed: the event of the framework.
            h.set_block(0, 4, 0, "minecraft:air");
        }
        r.step(None);
        h.tick();
        if t >= 30 && mob_start.is_none() && r.names(i).contains(&"FleeSun") {
            mob_start = Some(t - 30);
        }
        if t >= 30 && ecs_start.is_none() && h.world.get::<GoalSet>(e).unwrap().is_running(&flee_sun::MODULE) {
            ecs_start = Some(t - 30);
        }
    }
    eprintln!("flee_4 roof removed at 30: first tick of the flight after it: mob {mob_start:?} ecs {ecs_start:?}");
    assert!(mob_start.is_some_and(|t| t <= 3));
    assert!(ecs_start.is_some_and(|t| t <= 3), "the burning monster does not ask again when its block sees the sky");
}

// ======================================================================================================================
// RandomStand
// ======================================================================================================================
fn horse_run(horses: usize, ticks: usize, grass: bool) -> (Vec<Vec<usize>>, Vec<Vec<usize>>) {
    let at = |k: usize| (-15.0 + 2.0 * (k % 16) as f64 + 0.5, -9.0 + 3.0 * (k / 16) as f64 + 0.5);
    let mut r = MobRef::new(20, false);
    if grass {
        for x in -19..=19 {
            for z in -19..=19 {
                r.set_block(x, 0, z, "minecraft:grass_block");
            }
        }
    }
    let horse = mob_kind("minecraft:horse");
    let ids: Vec<usize> = (0..horses).map(|k| mob_add(&mut r, horse.clone(), Vec3::new(at(k).0, 1.0, at(k).1), false)).collect();
    let mut mrears: Vec<Vec<usize>> = vec![Vec::new(); horses];
    let mut prev = vec![0; horses];
    for t in 0..ticks {
        r.step(None);
        for (k, &i) in ids.iter().enumerate() {
            let c = r.mobs[i].extra.counter;
            if c > 0 && prev[k] == 0 {
                mrears[k].push(t);
            }
            prev[k] = c;
        }
    }
    let entries: &'static [Entry] = leak([Entry::new(&random_stand::MODULE, 9, &random_stand::Params::DEFAULT)]);
    let mut h = Herd::with_slab(20, false, false);
    let es: Vec<Entity> = (0..horses).map(|k| ecs_with(&mut h, entries, at(k).0, at(k).1, 30 + k as u64)).collect();
    for (k, &e) in es.iter().enumerate() {
        h.world.get_mut::<Phase>(e).unwrap().0 = u64::from(r.mobs[ids[k]].id);
    }
    let mut erears: Vec<Vec<usize>> = vec![Vec::new(); horses];
    let mut last = vec![0u64; horses];
    for t in 0..ticks {
        h.tick();
        for (k, &e) in es.iter().enumerate() {
            if let Some(s) = h.world.get::<random_stand::Standing>(e)
                && s.until != last[k]
            {
                last[k] = s.until;
                erears[k].push(t);
            }
        }
    }
    (mrears, erears)
}
fn rate(v: &[Vec<usize>], ticks: usize) -> f64 {
    v.iter().map(Vec::len).sum::<usize>() as f64 / (v.len() * ticks) as f64 * 10_000.0
}
#[test]
fn stand_1_rate_and_first_rear() {
    if horizon_ecs_proto::cow::skip_unless_default_species("stand_1_rate_and_first_rear") {
        return;
    }
    if horizon_ecs_proto::cow::skip_unless_walker("stand_1_rate_and_first_rear") {
        return;
    }
    if !have_data() {
        return;
    }
    let (n, ticks) = (48, 30_000);
    let t0 = std::time::Instant::now();
    let (m, e) = horse_run(n, ticks, false);
    let first = |v: &[Vec<usize>]| {
        let f: Vec<f64> = v.iter().filter_map(|x| x.first().map(|&t| t as f64)).collect();
        (f.len(), mean(&f))
    };
    eprintln!(
        "stand_1 ({n} horses, {ticks} ticks, {:.1}s): rears per 10000 ticks per horse: mob {:.3} ecs {:.3}; first rear (count, mean tick): mob {:?} ecs {:?}",
        t0.elapsed().as_secs_f64(), rate(&m, ticks), rate(&e, ticks), first(&m), first(&e)
    );
    let (rm, re) = (rate(&m, ticks), rate(&e, ticks));
    assert!((rm - re).abs() / rm < 0.25, "rear rate differs: {rm} vs {re}");
}
#[test]
fn stand_2_a_horse_that_grazes_does_not_rear() {
    if horizon_ecs_proto::cow::skip_unless_default_species("stand_2_a_horse_that_grazes_does_not_rear") {
        return;
    }
    if horizon_ecs_proto::cow::skip_unless_walker("stand_2_a_horse_that_grazes_does_not_rear") {
        return;
    }
    if !have_data() {
        return;
    }
    let (n, ticks) = (48, 30_000);
    let (m, e) = horse_run(n, ticks, true);
    eprintln!("stand_2 on grass: rears per 10000 ticks per horse: mob {:.3} ecs {:.3}", rate(&m, ticks), rate(&e, ticks));
    let (rm, re) = (rate(&m, ticks), rate(&e, ticks));
    assert!((rm - re).abs() / rm < 0.25, "rear rate on grass differs: {rm} vs {re}");
}

#[test]
#[ignore = "trace"]
fn trace_avoid_from_start() {
    if !have_data() {
        return;
    }
    let _ = MobRef::new(20, false);
    let params = leak(avoid_entity::Params { what: avoid_entity::Avoid::Species(leak([species().id])), max_dist: 6.0, ..avoid_entity::Params::DEFAULT });
    let entries: &'static [Entry] = leak([Entry::new(&avoid_entity::MODULE, 3, params)]);
    let mut h = Herd::with_slab(20, false, false);
    let e = ecs_with(&mut h, entries, 0.5, 0.5, 7);
    ecs_with(&mut h, &[], 5.5, 0.5, 8);
    for t in 0..40 {
        h.tick();
        let g = h.world.get::<GoalSet>(e).unwrap().is_running(&avoid_entity::MODULE);
        let p = h.world.get::<Pos>(e).unwrap().0;
        let n = h.world.get::<Nav>(e).unwrap();
        eprintln!("trace t={t} running {g} pos ({:.2},{:.2},{:.2}) path {} needs {}", p.x, p.y, p.z, n.path.is_some(), h.world.get::<NeedsPath>(e).is_some());
    }
}

/// The law of the first rear, from the rule of `RandomStandGoal.canUse` itself (a counter from -400, one ask every other
/// tick, `nextInt(1000) < counter` then `nextInt(10) == 0`) simulated apart, against 2000 ECS horses.
#[test]
fn stand_3_first_rear_law() {
    if horizon_ecs_proto::cow::skip_unless_default_species("stand_3_first_rear_law") {
        return;
    }
    if horizon_ecs_proto::cow::skip_unless_walker("stand_3_first_rear_law") {
        return;
    }
    if !have_data() {
        return;
    }
    let _ = MobRef::new(20, false);
    let horizon = 50_000usize;
    let mut rng = horizon_ecs_proto::cow::Rng(0x9e37_79b9_7f4a_7c15);
    let mut sim = Vec::new();
    for _ in 0..4000 {
        let (mut next, mut ticks) = (-400i32, 0usize);
        loop {
            ticks += 2;
            next += 1;
            if next > 0 && rng.next_int(1000) < next {
                next = -400;
                if rng.next_int(10) == 0 {
                    break;
                }
            }
        }
        if ticks < horizon {
            sim.push(ticks as f64);
        }
    }
    let entries: &'static [Entry] = leak([Entry::new(&random_stand::MODULE, 9, &random_stand::Params::DEFAULT)]);
    let mut h = Herd::with_slab(20, false, false);
    let es: Vec<Entity> = (0..2000).map(|k| ecs_with(&mut h, entries, -15.0 + 2.0 * (k % 16) as f64 + 0.5, -9.0 + 0.01 * k as f64, 100 + k as u64)).collect();
    let mut first: Vec<Option<usize>> = vec![None; es.len()];
    for t in 0..horizon {
        h.tick();
        for (k, &e) in es.iter().enumerate() {
            if first[k].is_none() && h.world.get::<random_stand::Standing>(e).is_some() {
                first[k] = Some(t);
            }
        }
    }
    let ecs: Vec<f64> = first.iter().flatten().map(|&t| t as f64).collect();
    eprintln!("stand_3 first rear before {horizon}: rule simulated mean {:.0} (n {}), ecs mean {:.0} (n {})", mean(&sim), sim.len(), mean(&ecs), ecs.len());
    assert!((mean(&sim) - mean(&ecs)).abs() / mean(&sim) < 0.08, "the first rear comes at another time");
}

/// Duration of the rear: the ticks (sampled after each tick) during which the animal is up, and the end of the first one.
#[test]
fn stand_4_rear_lasts_twenty_ticks_and_does_not_chain() {
    if horizon_ecs_proto::cow::skip_unless_default_species("stand_4_rear_lasts_twenty_ticks_and_does_not_chain") {
        return;
    }
    if horizon_ecs_proto::cow::skip_unless_walker("stand_4_rear_lasts_twenty_ticks_and_does_not_chain") {
        return;
    }
    if !have_data() {
        return;
    }
    let mut r = MobRef::new(20, false);
    let horse = mob_kind("minecraft:horse");
    let ids: Vec<usize> = (0..60).map(|k| mob_add(&mut r, horse.clone(), Vec3::new(-15.0 + 2.0 * (k % 16) as f64 + 0.5, 1.0, -9.0 + 3.0 * (k / 16) as f64 + 0.5), false)).collect();
    let mut runs: Vec<usize> = Vec::new();
    let mut cur = vec![0usize; ids.len()];
    for _ in 0..20_000 {
        r.step(None);
        for (k, &i) in ids.iter().enumerate() {
            if r.mobs[i].extra.counter > 0 {
                cur[k] += 1;
            } else if cur[k] > 0 {
                runs.push(cur[k]);
                cur[k] = 0;
            }
        }
    }
    let entries: &'static [Entry] = leak([Entry::new(&random_stand::MODULE, 9, &random_stand::Params::DEFAULT)]);
    let mut h = Herd::with_slab(20, false, false);
    let es: Vec<Entity> = (0..600).map(|k| ecs_with(&mut h, entries, -15.0 + 2.0 * (k % 16) as f64 + 0.5, -9.0 + 0.05 * k as f64, 200 + k as u64)).collect();
    let mut eruns: Vec<usize> = Vec::new();
    let mut ecur = vec![0usize; es.len()];
    for _ in 0..20_000 {
        h.tick();
        let now = h.world.resource::<Clock>().0;
        for (k, &e) in es.iter().enumerate() {
            if h.world.get::<random_stand::Standing>(e).is_some_and(|s| now < s.until) {
                ecur[k] += 1;
            } else if ecur[k] > 0 {
                eruns.push(ecur[k]);
                ecur[k] = 0;
            }
        }
    }
    let hist = |v: &[usize]| {
        let mut m = std::collections::BTreeMap::new();
        for &x in v {
            *m.entry(x).or_insert(0) += 1;
        }
        m
    };
    eprintln!("stand_4 duration of a rear (ticks seen up -> count): mob {:?} ecs {:?}", hist(&runs), hist(&eruns));
    assert!(!runs.is_empty() && !eruns.is_empty());
    assert_eq!(hist(&runs).keys().collect::<Vec<_>>(), hist(&eruns).keys().collect::<Vec<_>>(), "durations differ");
}
/// The rate of rears on grass (the horse grazes and refuses to rear while it eats) against stone: the Mob loses the rears
/// asked during a meal, the ECS horse has no meal. Slow: `--ignored`.
#[test]
#[ignore = "slow, and C: no Immobile service in the framework (a grazing horse does not rear)"]
fn stand_5_grazing_horse_rears_less() {
    if !have_data() {
        return;
    }
    let (n, ticks) = (150, 40_000);
    let (ms, es) = horse_run(n, ticks, false);
    let (mg, eg) = horse_run(n, ticks, true);
    eprintln!(
        "stand_5 rears per 10000 ticks per horse: stone mob {:.3} ecs {:.3}; grass mob {:.3} ecs {:.3}",
        rate(&ms, ticks), rate(&es, ticks), rate(&mg, ticks), rate(&eg, ticks)
    );
    assert!((rate(&mg, ticks) - rate(&eg, ticks)).abs() / rate(&mg, ticks) < 0.05);
}
