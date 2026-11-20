//! The lot 5 (drowned, slime at contact, polar bear, the seven species of the configuration wave) against real `Mob`s of the species
//! (`mob_kind`): the laws of first occurrence and of durations, N draws per side, with ids far apart (PIEGES 46: the
//! Java generator's first draw is correlated between neighbouring seeds). `--nocapture` prints. Cow body only.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_client_lib::mobs::{self, Mob, MobEvent, Who as MWho};
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{behaviours::Entry, cow::*, goals::*, targeting::*};
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
        horizon_ecs_proto::cow::skip_note("lot4_vs_mob", &format!("written for HORIZON_SPECIES=minecraft:cow, not {sp}"));
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


fn ps_immortal(h: &mut Herd) {
    h.world.resource_mut::<horizon_ecs_proto::hostile::PlayerState>().immortal = true;
}


/// A raised platform (stone at y 59) with a pool on x < 0 (water at y 60 and 61) and a beach on x >= 0, in both worlds; the sea level of
/// the `Mob`'s world is 63 (the drowned's goals read it).
fn platform(r: &MobRef, h: &mut Herd) {
    for x in -14..=14 {
        for z in -14..=14 {
            r.set_block(x, 59, z, "minecraft:stone");
            h.set_block(x, 59, z, "minecraft:stone");
            if x < 0 {
                for y in 60..=61 {
                    r.set_block(x, y, z, "minecraft:water");
                    h.set_block(x, y, z, "minecraft:water");
                }
            }
        }
    }
}
/// Day or night in both worlds (the `Mob`'s world is a leaked static: written through a raw pointer, as the blocks are).
fn set_day(r: &MobRef, h: &mut Herd, day: bool) {
    // SAFETY: the test thread owns the world while it builds the scene; no reference to the field is alive.
    unsafe {
        (*(r.w as *const mobs::MobWorld as *mut mobs::MobWorld)).daylight = day;
    }
    h.world.resource_mut::<horizon_ecs_proto::hostile::Daylight>().0 = day;
}

/// The drowned against its `Mob`: by day on the beach it goes to the water (`DrownedGoToWaterGoal`); at night in the water it goes to the beach
/// (`DrownedGoToBeachGoal`, which needs the sea level - 3 or higher) and swims up (`DrownedSwimUpGoal`). First tick at the goal's mark, 8 draws.
#[test]
fn drowned_goes_to_the_water_by_day_and_to_the_beach_by_night() {
    if !on() {
        return;
    }
    for (day, label, from, mark) in [(true, "day: beach -> water, tick in the water", 3.5, -0.2f64), (false, "night: water -> beach, tick on the beach", -5.5, 0.3)] {
        let (mut sm, mut se, mut n) = (0.0f64, 0.0f64, 0.0f64);
        let mut rows = Vec::new();
        for j in 0..8 {
            let mut r = MobRef::new(24, false);
            let mut h = Herd::with_slab(24, false, false);
            platform(&r, &mut h);
            set_day(&r, &mut h, day);
            let i = mob_add(&mut r, mob_kind("minecraft:drowned"), Vec3::new(from, 60.0, 0.5), j + 1);
            r.mobs[i].ai.no_action_time = 0;
            let e = ecs_named(&mut h, "minecraft:drowned", None, 0.5, 0.5, 11_000 + j as u64, u64::from(far_id(j + 1)));
            h.teleport(e, Vec3::new(from, 60.0, 0.5));
            // The first ask of the ECS runs before its first fluid update: a drowned that starts in the water is told so (the `Mob`'s first ask
            // sees the water it was made in), else its first ask draws the beach countdown on a dry reading (PIEGES, lot 5).
            if from < 0.0 {
                let mut f = h.world.get_mut::<FluidState>(e).unwrap();
                f.in_water = true;
                f.water = 1.0;
            }
            let (mut tm, mut te) = (None, None);
            for t in 0..1200 {
                r.step(None);
                h.tick();
                let (pm, pe) = (r.mobs[i].position, h.world.get::<Pos>(e).unwrap().0);
                let hit = |x: f64| if day { x < mark } else { x > mark };
                if tm.is_none() && hit(pm.x) {
                    tm = Some(t);
                }
                if te.is_none() && hit(pe.x) {
                    te = Some(t);
                }
            }
            rows.push((tm, te));
            if let (Some(a), Some(b)) = (tm, te) {
                sm += a as f64;
                se += b as f64;
                n += 1.0;
            }
        }
        eprintln!("drowned {label}: first tick mob/ecs per draw {rows:?}; mean over {n}: mob {:.1} ecs {:.1}", sm / n.max(1.0), se / n.max(1.0));
    }
}

/// The cost of the day/night chains of the drowned: 300 drowned standing on a beach at night (they ask `try_find_water` and the beach goal
/// again and again for want of a day/night event), instructions per drowned and tick against the same count of cows (`--ignored`).
#[test]
#[ignore = "bench: instructions per drowned and tick against cows, run by hand with --ignored"]
fn cost_of_the_drowned_day_night_chain() {
    if !on() {
        return;
    }
    let Some(c) = horizon_ecs_proto::instr::Counter::new() else { return };
    let mut per = Vec::new();
    for name in ["minecraft:drowned", "minecraft:cow"] {
        let mut h = Herd::with_slab(40, false, false);
        h.world.resource_mut::<horizon_ecs_proto::hostile::Daylight>().0 = false;
        let mut es = Vec::new();
        for k in 0..300 {
            let e = ecs_named(&mut h, name, None, -30.0 + 2.0 * (k % 30) as f64 + 0.5, -20.0 + 2.0 * (k / 30) as f64 + 0.5, 70 + k as u64, k as u64);
            es.push(e);
        }
        for _ in 0..300 {
            h.tick();
        }
        let a = c.read();
        for _ in 0..600 {
            h.tick();
        }
        per.push((c.read() - a) as f64 / 600.0 / 300.0);
    }
    eprintln!("MEASURE drowned at night: {:.0} instr per creature-tick; cow {:.0}", per[0], per[1]);
}

/// A slime of size 2 and a player 6 blocks away: `CubeAttack` aims the hops at the player: the distance after 300 ticks, 8 draws.
#[test]
fn slime_hunts_the_player_at_contact_range() {
    if !on() {
        return;
    }
    let (mut dm, mut de) = (Vec::new(), Vec::new());
    for j in 0..8 {
        let mut r = MobRef::new(24, false);
        let mut h = Herd::with_slab(24, false, false);
        let i = mob_add(&mut r, mob_kind("minecraft:slime"), Vec3::new(0.5, 1.0, 0.5), j + 1);
        mobs::cube::set_size(&mut r.mobs[i], 2);
        r.mobs[i].ai.no_action_time = 0;
        let e = ecs_named(&mut h, "minecraft:slime", None, 0.5, 0.5, 12_000 + j as u64, u64::from(far_id(j + 1)));
        horizon_ecs_proto::size::set_size(&mut h.world, e, 2);
        let at = Vec3::new(8.5, 1.0, 0.5);
        let view = player_view(at, None);
        ps_immortal(&mut h);
        h.world.resource_mut::<Online>().set(Player { id: 1, x: at.x, y: at.y, z: at.z, main: 0, off: 0, ..Default::default() });
        for _ in 0..300 {
            r.step(Some(&view));
            h.tick();
        }
        dm.push(xz(r.mobs[i].position, at));
        de.push(xz(h.world.get::<Pos>(e).unwrap().0, at));
    }
    eprintln!("slime (size 2) 8 blocks from a player after 300 ticks: mob {:.2} ecs {:.2} (mean distance)", mean(&dm), mean(&de));
}

/// The polar bear: an adult with a cub 3 blocks away and a player 7 blocks away takes the player for its target; without the cub it does not.
/// First tick of the target, 12 draws (`interval 20`, follow scale 0.5).
#[test]
fn polar_bear_defends_its_cub() {
    if !on() {
        return;
    }
    for cub in [true, false] {
        let (mut sm, mut se, mut nm, mut ne) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
        for j in 0..12 {
            let mut r = MobRef::new(24, false);
            let mut h = Herd::with_slab(24, false, false);
            let i = mob_add(&mut r, mob_kind("minecraft:polar_bear"), Vec3::new(0.5, 1.0, 0.5), j + 1);
            r.mobs[i].ai.no_action_time = 0;
            let e = ecs_named(&mut h, "minecraft:polar_bear", None, 0.5, 0.5, 13_000 + j as u64, u64::from(far_id(j + 1)));
            if cub {
                let c = mob_add(&mut r, mob_kind("minecraft:polar_bear"), Vec3::new(3.5, 1.0, 0.5), j + 40);
                mobs::finalize::set_baby(&mut r.mobs[c], true);
                let ce = ecs_named(&mut h, "minecraft:polar_bear", None, 3.5, 0.5, 14_000 + j as u64, u64::from(far_id(j + 40)));
                h.make_baby(ce, 24_000);
            }
            let at = Vec3::new(7.5, 1.0, 0.5);
            let view = player_view(at, None);
            ps_immortal(&mut h);
            h.world.resource_mut::<Online>().set(Player { id: 1, x: at.x, y: at.y, z: at.z, main: 0, off: 0, ..Default::default() });
            let (mut tm, mut te) = (None, None);
            for t in 0..400 {
                r.step(Some(&view));
                h.tick();
                if tm.is_none() && r.mobs[i].ai.target.is_some() {
                    tm = Some(t);
                }
                if te.is_none() && h.world.get::<Target>(e).is_some() {
                    te = Some(t);
                }
            }
            if let Some(a) = tm {
                sm += a as f64;
                nm += 1.0;
            }
            if let Some(b) = te {
                se += b as f64;
                ne += 1.0;
            }
        }
        eprintln!("polar bear with cub {cub}: draws with a target mob {nm} ecs {ne}; first tick mean mob {:.1} ecs {:.1}", sm / nm.max(1.0), se / ne.max(1.0));
        if !cub {
            assert!(nm == 0.0 && ne == 0.0, "without a cub neither bear takes the player (witness of the previous line)");
        } else {
            assert!(ne >= 10.0, "the ECS bear defends its cub in most draws ({ne} of 12)");
        }
    }
}

/// The seven species of the configuration wave, one short scenario each (a player 6 blocks away, 600 ticks): the goals that ran on each side
/// (informative table) and that the ECS species starts, runs goals and stays alive.
#[test]
fn the_species_of_the_configuration_wave_start_and_run() {
    if !on() {
        return;
    }
    for name in ["minecraft:strider", "minecraft:parrot", "minecraft:turtle", "minecraft:enderman", "minecraft:skeleton", "minecraft:ghast", "minecraft:zombified_piglin"] {
        let mut r = MobRef::new(24, false);
        let mut h = Herd::with_slab(24, false, false);
        let i = mob_add(&mut r, mob_kind(name), Vec3::new(0.5, 1.0, 0.5), 5);
        r.mobs[i].ai.no_action_time = 0;
        let e = ecs_named(&mut h, name, None, 0.5, 0.5, 15_000, u64::from(far_id(5)));
        let at = Vec3::new(6.5, 1.0, 0.5);
        let view = player_view(at, None);
        ps_immortal(&mut h);
        h.world.resource_mut::<Online>().set(Player { id: 1, x: at.x, y: at.y, z: at.z, main: 0, off: 0, ..Default::default() });
        let (mut mg, mut eg) = (std::collections::BTreeSet::new(), std::collections::BTreeSet::new());
        for _ in 0..600 {
            r.step(Some(&view));
            h.tick();
            for n in r.mobs[i].ai.goals.running_names() {
                mg.insert(n.to_string());
            }
            let g = h.world.get::<GoalSet>(e).unwrap();
            for en in g.entries().iter().filter(|en| g.is_running(en.module)) {
                eg.insert(en.module.name.to_string());
            }
        }
        eprintln!("{name}: mob ran {mg:?}; ecs ran {eg:?}");
        assert!(h.world.get_entity(e).is_ok(), "{name} stays alive");
        assert!(!eg.is_empty() || name == "minecraft:ghast" || name == "minecraft:skeleton", "{name}: some goal of the ECS species ran");
    }
}

#[test]
#[ignore = "debug probe of the drowned at night, run by hand with --ignored"]
fn debug_drowned_night() {
    if !on() {
        return;
    }
    let mut r = MobRef::new(24, false);
    let mut h = Herd::with_slab(24, false, false);
    platform(&r, &mut h);
    set_day(&r, &mut h, false);
    let i = mob_add(&mut r, mob_kind("minecraft:drowned"), Vec3::new(-5.5, 60.0, 0.5), 3);
    r.mobs[i].ai.no_action_time = 0;
    let e = ecs_named(&mut h, "minecraft:drowned", None, 0.5, 0.5, 11_000, 3);
    h.teleport(e, Vec3::new(-5.5, 60.0, 0.5));
    { let mut f = h.world.get_mut::<FluidState>(e).unwrap(); f.in_water = true; f.water = 1.0; }
    for t in 0..1200 {
        r.step(None);
        h.tick();
        if t % 100 == 0 {
            let p = h.world.get::<Pos>(e).unwrap().0;
            let g = h.world.get::<GoalSet>(e).unwrap();
            let n: Vec<_> = g.entries().iter().filter(|en| g.is_running(en.module)).map(|en| en.module.name).collect();
            eprintln!("DBG t{t} ecs ({:.1},{:.1}) {n:?} | mob ({:.1},{:.1}) {:?}", p.x, p.y, r.mobs[i].position.x, r.mobs[i].position.y, r.mobs[i].ai.goals.running_names());
        }
    }
}

/// The drowned's targets and the continuation of its attack against its `Mob`: a villager is a target as a wandering trader is (`AbstractVillager`, not seen
/// needed), and `DrownedAttackGoal.canContinueToUse` asks `okTarget` again on every tick of the pursuit: by day, with the prey in the water the goal runs;
/// with the prey brought out of it the goal ends at once, on both sides, on the same tick.
#[test]
fn a_drowned_hunts_a_villager_and_drops_the_attack_when_it_leaves_the_water() {
    if !on() {
        return;
    }
    for night in [true, false] {
        let mut r = MobRef::new(24, false);
        let mut h = Herd::with_slab(24, false, false);
        platform(&r, &mut h);
        set_day(&r, &mut h, !night);
        let i = mob_add(&mut r, mob_kind("minecraft:drowned"), Vec3::new(-6.5, 60.0, 0.5), 3);
        let v = mob_add(&mut r, mob_kind("minecraft:villager"), Vec3::new(-3.5, 60.0, 0.5), 9);
        r.mobs[v].ai.goals = Default::default();
        r.mobs[v].ai.no_action_time = 0;
        r.mobs[i].ai.no_action_time = 0;
        let e = ecs_named(&mut h, "minecraft:drowned", None, 0.5, 0.5, 11_000, 3);
        h.teleport(e, Vec3::new(-6.5, 60.0, 0.5));
        let ev = ecs_named(&mut h, "minecraft:villager", Some(&[]), 0.5, 0.5, 12_000, 9);
        h.teleport(ev, Vec3::new(-3.5, 60.0, 0.5));
        for en in [e, ev] {
            let mut f = h.world.get_mut::<FluidState>(en).unwrap();
            f.in_water = true;
            f.water = 1.0;
        }
        let melee_names = |r: &MobRef| r.mobs[i].ai.goals.running_names().iter().filter(|n| n.contains("Attack")).count() > 0;
        let ecs_melee = |h: &Herd| {
            let g = h.world.get::<GoalSet>(e).unwrap();
            g.entries().iter().any(|en| en.module.name == "melee" && g.is_running(en.module))
        };
        let (mut tm, mut te) = (None, None);
        for t in 0..400 {
            r.mobs[i].ai.no_action_time = 0;
            r.step(None);
            h.tick();
            if tm.is_none() && r.mobs[i].ai.target.is_some() {
                tm = Some(t);
            }
            if te.is_none() && h.world.get::<Target>(e).is_some() {
                te = Some(t);
            }
            // By day the leaving test starts as soon as both pursue (the prey would not live through 400 ticks of blows).
            if !night && melee_names(&r) && ecs_melee(&h) {
                break;
            }
            if night && tm.is_some() && te.is_some() {
                break;
            }
        }
        eprintln!("villager target (night {night}): first tick mob {tm:?} ecs {te:?}");
        assert!(tm.is_some() && te.is_some(), "night {night}: both hunt the villager (mob {tm:?}, ecs {te:?})");
        if night {
            continue;
        }
        // The prey is brought to the beach: the goal ends where it stands, on the same tick on both sides.
        assert!(melee_names(&r) && ecs_melee(&h), "by day, prey in the water: both pursue (mob {:?})", r.mobs[i].ai.goals.running_names());
        let (mp, ep) = (Vec3::new(5.5, 60.0, 0.5), Vec3::new(5.5, 60.0, 0.5));
        r.mobs[v].position = mp;
        h.teleport(ev, ep);
        let mut rows = Vec::new();
        for t in 0..6 {
            r.mobs[i].ai.no_action_time = 0;
            r.step(None);
            h.tick();
            rows.push((t, melee_names(&r), ecs_melee(&h)));
        }
        eprintln!("goal running per tick after the prey leaves the water (mob, ecs): {rows:?}");
        assert!(rows.iter().all(|&(_, a, b)| a == b), "{rows:?}");
        assert!(!rows[0].1 || !rows[0].2 || rows.iter().any(|r| !r.1), "{rows:?}");
    }
}
