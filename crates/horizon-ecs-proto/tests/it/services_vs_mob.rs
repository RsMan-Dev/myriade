//! The services S1 to S5 against real `Mob`s of the species (zoglin, panda, rabbit, snow golem, armadillo, turtle)
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
        horizon_ecs_proto::cow::skip_note("lot2_vs_mob", &format!("written for HORIZON_SPECIES=minecraft:cow, not {sp}"));
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
/// S1 and S2: a zoglin and a player 1.5 blocks away, 700 ticks, adult and baby: the ticks with a target and the gaps between the
/// strikes (`melee_attack_by_age(40, 15)`: 41 then 16 on the `Mob`), on each side.
#[test]
fn zoglin_keeps_its_target_and_strikes_by_age() {
    if !on() {
        return;
    }
    for baby in [false, true] {
        let mut r = MobRef::new(24, false);
        let mut h = Herd::with_slab_awake(24, false, false);
        let i = mob_add(&mut r, mob_kind("minecraft:zoglin"), Vec3::new(0.5, 1.0, 0.5), 5);
        r.mobs[i].ai.no_action_time = 0;
        if baby {
            mobs::finalize::set_baby(&mut r.mobs[i], true);
        }
        let e = ecs_named(&mut h, "minecraft:zoglin", None, 0.5, 0.5, 11000, u64::from(far_id(5)));
        if baby {
            h.make_baby(e, 24_000);
        }
        let player = Vec3::new(2.0, 1.0, 0.5);
        let view = player_view(player, None);
        ps_immortal(&mut h);
        h.world.resource_mut::<Online>().set(Player { id: 1, x: player.x, y: player.y, z: player.z, main: 0, off: 0, ..Default::default() });
        h.tick();
        let at = *h.world.get::<horizon_ecs_proto::hostile::Attack>(e).unwrap();
        eprintln!("zoglin ECS attack data: interval {} baby {:?} slot {}", at.interval, at.interval_baby, at.melee_slot);
        let (mut mt, mut et) = (0, 0);
        let (mut ms, mut es): (Vec<i64>, Vec<i64>) = (Vec::new(), Vec::new());
        for t in 0..700 {
            let from = r.events.len();
            r.step(Some(&view));
            for ev in &r.events[from..] {
                if matches!(ev, MobEvent::AttackPlayer(..)) {
                    ms.push(t);
                }
            }
            h.tick();
            mt += i32::from(r.mobs[i].ai.target.is_some());
            et += i32::from(h.world.get::<Target>(e).is_some());
            let hits = h.world.resource::<horizon_ecs_proto::hostile::PlayerState>().hits.len();
            while es.len() < hits {
                es.push(t);
            }
        }
        {
            let ps = h.world.resource::<horizon_ecs_proto::hostile::PlayerState>();
            eprintln!("first ECS hits: {:?}", ps.hits.iter().take(5).map(|x| (x.tick, x.amount, x.from)).collect::<Vec<_>>());
        }
        let gaps = |v: &[i64]| v.windows(2).map(|w| w[1] - w[0]).take(5).collect::<Vec<_>>();
        eprintln!("zoglin {}: ticks with a target mob {mt} ecs {et}; strikes mob {} ecs {}; first gaps mob {:?} ecs {:?}", if baby { "baby" } else { "adult" }, ms.len(), es.len(), gaps(&ms), gaps(&es));
        assert!(et > 600, "the ECS zoglin keeps its target ({et})");
        assert!((ms.len() as i64 - es.len() as i64).abs() <= 3, "strikes {} vs {}", ms.len(), es.len());
        // A restart of the goal at the end of a path costs the ECS adult one tick (42 for 41): D, written in the report.
    }
}

/// S3: a worried panda and a player at 5 blocks: the tick at which it runs from the player, on each side; a normal panda does not.
#[test]
fn panda_avoids_only_when_worried() {
    if !on() {
        return;
    }
    for worried in [true, false] {
        let mut r = MobRef::new(24, false);
        let mut h = Herd::with_slab_awake(24, false, false);
        let i = mob_add(&mut r, mob_kind("minecraft:panda"), Vec3::new(0.5, 1.0, 0.5), 8);
        r.mobs[i].ai.no_action_time = 0;
        if worried {
            r.mobs[i].extra.variant = mobs::panda::WORRIED;
            r.mobs[i].extra.variant2 = mobs::panda::WORRIED;
        }
        let e = ecs_named(&mut h, "minecraft:panda", None, 0.5, 0.5, 11100, u64::from(far_id(8)));
        if worried {
            h.world.entity_mut(e).insert(horizon_ecs_proto::behaviours::panda_state::Gene::Worried);
        }
        let player = Vec3::new(5.5, 1.0, 0.5);
        let view = player_view(player, None);
        ps_immortal(&mut h);
        h.world.resource_mut::<Online>().set(Player { id: 1, x: player.x, y: player.y, z: player.z, main: 0, off: 0, ..Default::default() });
        let (mut a, mut b) = (None, None);
        for t in 0..200 {
            r.step(Some(&view));
            h.tick();
            if a.is_none() && r.mobs[i].ai.goals.running_names().iter().any(|n| n.contains("AvoidEntity")) {
                a = Some(t);
            }
            if b.is_none() && h.world.get::<horizon_ecs_proto::behaviours::avoid_entity::Avoiding>(e).is_some() {
                b = Some(t);
            }
        }
        eprintln!("panda worried {worried}: first run from the player: mob {a:?} ecs {b:?}");
        assert_eq!(a.is_some(), b.is_some(), "worried {worried}");
    }
}

/// S3 and S2: a rabbit and a player at 5 blocks (then a zombie at 3 blocks): the tick it runs, on each side.
#[test]
fn rabbit_flees_the_player_and_the_monsters() {
    if !on() {
        return;
    }
    for what in ["player", "zombie"] {
        let mut r = MobRef::new(24, false);
        let mut h = Herd::with_slab_awake(24, false, false);
        let i = mob_add(&mut r, mob_kind("minecraft:rabbit"), Vec3::new(0.5, 1.0, 0.5), 9);
        r.mobs[i].ai.no_action_time = 100;
        let e = ecs_named(&mut h, "minecraft:rabbit", None, 0.5, 0.5, 11200, u64::from(far_id(9)));
        let view = if what == "player" { player_view(Vec3::new(5.5, 1.0, 0.5), None) } else { player_view(Vec3::new(40.5, 1.0, 40.5), None) };
        ps_immortal(&mut h);
        if what == "player" {
            h.world.resource_mut::<Online>().set(Player { id: 1, x: 5.5, y: 1.0, z: 0.5, main: 0, off: 0, ..Default::default() });
        } else {
            let k = r.kind.clone();
            let _ = k;
            let z = mob_add(&mut r, mob_kind("minecraft:zombie"), Vec3::new(3.5, 1.0, 0.5), 10);
            r.mobs[z].ai.goals.clear();
            r.mobs[z].ai.targets.clear();
            let ze = ecs_named(&mut h, "minecraft:zombie", Some(Box::leak(Box::new([]))), 3.5, 0.5, 11201, u64::from(far_id(10)));
            let _ = ze;
        }
        let (mut a, mut b) = (None, None);
        for t in 0..120 {
            r.step(Some(&view));
            h.tick();
            if a.is_none() && r.mobs[i].ai.goals.running_names().iter().any(|n| n.contains("RabbitAvoid")) {
                a = Some(t);
            }
            if b.is_none() && h.world.get::<horizon_ecs_proto::behaviours::avoid_entity::Avoiding>(e).is_some() {
                b = Some(t);
            }
        }
        eprintln!("rabbit vs {what}: first flight: mob {a:?} ecs {b:?}");
        assert_eq!(a.is_some(), b.is_some(), "{what}");
    }
}

/// S2: a snow golem and a zombie at 8 blocks (still, no goals): the tick at which the golem takes it as its target, on each side;
/// a cow at 8 blocks is not taken.
#[test]
fn snow_golem_targets_the_hostiles_only() {
    if !on() {
        return;
    }
    for victim in ["minecraft:zombie", "minecraft:cow"] {
        let mut r = MobRef::new(24, false);
        let mut h = Herd::with_slab_awake(24, false, false);
        let g = mob_add(&mut r, mob_kind("minecraft:snow_golem"), Vec3::new(0.5, 1.0, 0.5), 11);
        r.mobs[g].ai.no_action_time = 100;
        let v = mob_add(&mut r, mob_kind(victim), Vec3::new(8.5, 1.0, 0.5), 12);
        r.mobs[v].ai.goals.clear();
        r.mobs[v].ai.targets.clear();
        let e = ecs_named(&mut h, "minecraft:snow_golem", None, 0.5, 0.5, 11300, u64::from(far_id(11)));
        let _ve = ecs_named(&mut h, victim, Some(Box::leak(Box::new([]))), 8.5, 0.5, 11301, u64::from(far_id(12)));
        let (mut a, mut b) = (None, None);
        for t in 0..300 {
            r.step(None);
            h.tick();
            if a.is_none() && r.mobs[g].ai.target.is_some() {
                a = Some(t);
            }
            if b.is_none() && h.world.get::<Target>(e).is_some() {
                b = Some(t);
            }
        }
        eprintln!("snow golem vs {victim}: first target: mob {a:?} ecs {b:?}");
        assert_eq!(a.is_some(), b.is_some(), "{victim}");
    }
}

/// S5: an armadillo hit by a creature does not panic (it rolls up); the environment (fire) does. Both sides.
#[test]
fn armadillo_panics_at_the_environment_only() {
    if !on() {
        return;
    }
    let mut r = MobRef::new(24, false);
    let mut h = Herd::with_slab_awake(24, false, false);
    let i = mob_add(&mut r, mob_kind("minecraft:armadillo"), Vec3::new(0.5, 1.0, 0.5), 13);
    r.mobs[i].ai.no_action_time = 100;
    let e = ecs_named(&mut h, "minecraft:armadillo", None, 0.5, 0.5, 11400, u64::from(far_id(13)));
    for _ in 0..10 {
        r.step(None);
        h.tick();
    }
    r.hit(i, 1.0, [4.0, 1.0, 0.5]);
    h.hit(e, 1.0, (4.0, 0.5));
    let (mut a, mut b) = (false, false);
    for _ in 0..80 {
        r.step(None);
        h.tick();
        a |= r.mobs[i].ai.goals.running_names().iter().any(|n| n.contains("Panic"));
        b |= h.world.get::<horizon_ecs_proto::behaviours::panic::Panicking>(e).is_some();
    }
    eprintln!("armadillo hit by a creature: panics mob {a} ecs {b}");
    assert_eq!(a, b);
}

/// S5 (ECS side): a hit panda calls only the aggressive pandas (`hurt_by.alert_when`).
#[test]
fn a_hurt_panda_alerts_the_aggressive_ones_only() {
    use horizon_ecs_proto::behaviours::panda_state::Gene;
    if !on() {
        return;
    }
    let mut h = Herd::with_slab_awake(24, false, false);
    ps_immortal(&mut h);
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 6.5, y: 1.0, z: 0.5, main: 0, off: 0, ..Default::default() });
    let victim = ecs_named(&mut h, "minecraft:panda", None, 0.5, 0.5, 11500, 3);
    let aggressive = ecs_named(&mut h, "minecraft:panda", None, 2.5, 0.5, 11501, 5);
    let calm = ecs_named(&mut h, "minecraft:panda", None, -1.5, 0.5, 11502, 7);
    h.world.entity_mut(aggressive).insert(Gene::Aggressive);
    for _ in 0..5 {
        h.tick();
    }
    h.hit_by(victim, 1.0, 1);
    for _ in 0..10 {
        h.tick();
    }
    let (a, c) = (h.world.get::<Target>(aggressive).is_some(), h.world.get::<Target>(calm).is_some());
    eprintln!("hurt panda: aggressive neighbour called {a}, calm neighbour called {c}");
    assert!(a && !c);
}

/// S2 (ECS side): `avoid_entity.target_roll` draws once the target is found: a roll that always fails never starts the flight, one that
/// passes half the time does.
#[test]
fn avoid_target_roll_gates_the_flight() {
    use horizon_ecs_proto::behaviours::avoid_entity::{self, Avoid, Avoiding};
    fn never(_: &World, _: Entity, _: f64) -> bool {
        false
    }
    fn half(_: &World, _: Entity, u: f64) -> bool {
        u < 0.5
    }
    if !on() {
        return;
    }
    for (name, roll) in [("never", never as fn(&World, Entity, f64) -> bool), ("half", half)] {
        let params: &'static avoid_entity::Params = Box::leak(Box::new(avoid_entity::Params { what: Avoid::Named(&["minecraft:zombie"]), max_dist: 8.0, target_roll: Some(roll), ..avoid_entity::Params::DEFAULT }));
        let entries: &'static [Entry] = Box::leak(Box::new([Entry::new(&avoid_entity::MODULE, 3, params)]));
        let mut h = Herd::with_slab_awake(24, false, false);
        let e = ecs_named(&mut h, "minecraft:cow", Some(entries), 0.5, 0.5, 11600, 3);
        let z = ecs_named(&mut h, "minecraft:zombie", Some(Box::leak(Box::new([]))), 3.5, 0.5, 11601, 5);
        let mut starts = 0;
        let mut was = false;
        for t in 0..600 {
            // The avoider is asked by the moves of the listed animals: the zombie steps back and forth, the cow put back each time.
            if t % 12 == 0 {
                h.teleport(z, Vec3::new(if t % 24 == 0 { 3.5 } else { 4.5 }, 1.0, 0.5));
                h.teleport(e, Vec3::new(0.5, 1.0, 0.5));
            }
            h.tick();
            let on_now = h.world.get::<Avoiding>(e).is_some();
            if on_now && !was {
                starts += 1;
            }
            was = on_now;
        }
        eprintln!("avoid target_roll {name}: flights started in 600 ticks: {starts}");
        assert_eq!(starts > 0, name == "half", "{name}");
    }
}
