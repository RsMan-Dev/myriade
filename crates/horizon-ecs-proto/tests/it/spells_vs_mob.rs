//! The two spells that need a world (`HORIZON_TEST_RESOURCES` set), against the `Mob`'s: the evoker's wololo (a blue sheep within the box
//! turns red 40 ticks after the goal starts, the caster having no target) and the illusioner's blindness (on hard, on a target it did not
//! blind last, 20 ticks after the start; it does not blind the same target twice in a row).
use crate::common;
use common::*;
use horizon_client_lib::mobs::{self, spawning::LocalDifficulty, MobEvent};
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{
    cow::*,
    effects::{self, Difficulty, Kind},
    farm::Wool,
    goals::*,
    targeting::{set_target, Target},
};

fn named(h: &Herd, name: &str) -> Species {
    *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == name).unwrap()
}
fn idle(mut s: Species) -> Species {
    s.goals = horizon_ecs_proto::species::Config { entries: &[] };
    s
}

/// The tick of the Mob's `Wololo` event for an evoker with a blue sheep 5 blocks away, and the sheep's colour afterwards.
fn mob_wololo(blue: bool) -> Option<usize> {
    let mut r = MobRef::new(24, false);
    let mut m = mobs::Mob::new(61, mob_kind("minecraft:evoker"), Vec3::new(0.5, 1.0, 0.5), 0.0);
    m.persistent = true;
    m.ai.no_action_time = 100;
    r.mobs.push(m);
    r.searches.push(0);
    r.mobs[0].ai.targets.clear();
    let s = r.add_kind(Vec3::new(5.5, 1.0, 0.5), mob_kind("minecraft:sheep"));
    r.mobs[s].ai.goals.clear();
    r.mobs[s].extra.color = if blue { 11 } else { 0 };
    for t in 0..200 {
        let before = r.events.len();
        r.step(None);
        r.mobs[s].position = Vec3::new(5.5, 1.0, 0.5);
        if r.events[before..].iter().any(|e| matches!(e, MobEvent::Wololo { .. })) {
            return Some(t);
        }
    }
    None
}
fn ecs_wololo(blue: bool) -> Option<usize> {
    let mut h = Herd::with_slab(24, false, false);
    set_lazy_look(&mut h.world, false);
    let ev = named(&h, "minecraft:evoker");
    let sheep = idle(named(&h, "minecraft:sheep"));
    let e = spawn_species(&mut h.world, &ev, 0.5, 0.5, 7);
    let s = spawn_species(&mut h.world, &sheep, 5.5, 0.5, 8);
    h.world.get_mut::<Wool>(s).unwrap().color = if blue { 11 } else { 0 };
    for x in [e, s] {
        h.world.get_mut::<ActiveUntil>(x).unwrap().0 = u64::MAX;
        h.world.resource_mut::<Entered>().0.push(x);
    }
    for t in 0..200 {
        h.tick();
        teleport(&mut h.world, s, Vec3::new(5.5, 1.0, 0.5));
        if h.world.get::<Wool>(s).unwrap().color == 14 {
            return Some(t);
        }
    }
    None
}
#[test]
fn an_evoker_turns_a_blue_sheep_red_as_the_mob_s_does() {
    if !have_data() {
        return;
    }
    let (m, e) = (mob_wololo(true), ecs_wololo(true));
    eprintln!("wololo tick: mob {m:?} ecs {e:?}");
    let (m, e) = (m.expect("the Mob casts"), e.expect("the ECS casts"));
    assert!(m.abs_diff(e) <= 3, "mob {m} ecs {e}");
    // A sheep of another colour is left alone, on both sides.
    assert_eq!((mob_wololo(false), ecs_wololo(false)), (None, None));
}

/// The ticks of the Mob's blindness on the player (a target taken at tick 20), over 400 ticks.
fn mob_blind(hard: bool) -> Vec<usize> {
    let mut r = MobRef::new(24, false);
    r.w = Box::leak(Box::new(mobs::MobWorld { difficulty: LocalDifficulty::new(if hard { 3 } else { 1 }, 18000, 0, 0.0), ..r.w.clone() }));
    let mut m = mobs::Mob::new(62, mob_kind("minecraft:illusioner"), Vec3::new(0.5, 1.0, 0.5), 0.0);
    m.persistent = true;
    m.ai.no_action_time = 100;
    r.mobs.push(m);
    r.searches.push(0);
    r.mobs[0].ai.targets.clear();
    let view = player_view(Vec3::new(0.5, 1.0, 6.5), None);
    let mut at = Vec::new();
    for t in 0..400 {
        if t == 20 {
            r.mobs[0].ai.target = Some(mobs::Who::Player);
        }
        let before = r.events.len();
        r.step(Some(&view));
        if r.events[before..].iter().any(|e| matches!(e, MobEvent::EffectPlayer { .. })) {
            at.push(t);
        }
        r.mobs[0].position = Vec3::new(0.5, 1.0, 0.5);
    }
    at
}
fn ecs_blind(hard: bool) -> Vec<usize> {
    let mut h = Herd::with_slab(24, false, false);
    set_lazy_look(&mut h.world, false);
    h.world.insert_resource(Difficulty(if hard { 3 } else { 1 }));
    h.set_online(&[(0.5, 6.5)]);
    let p = named(&h, "minecraft:illusioner");
    let e = spawn_species(&mut h.world, &p, 0.5, 0.5, 9);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.resource_mut::<Entered>().0.push(e);
    h.tick();
    let mut at = Vec::new();
    let mut had = false;
    for t in 0..400 {
        if t == 20 {
            set_target(&mut h.world, e, Target::player(1, 60));
        }
        h.tick();
        let has = effects::player_has(&h.world, 1, Kind::Blindness);
        if has && !had {
            at.push(t);
        }
        had = has;
        teleport(&mut h.world, e, Vec3::new(0.5, 1.0, 0.5));
    }
    at
}
#[test]
fn an_illusioner_blinds_a_target_once_as_the_mob_s_does() {
    if !have_data() {
        return;
    }
    let (m, e) = (mob_blind(true), ecs_blind(true));
    eprintln!("blindness ticks (hard): mob {m:?} ecs {e:?}");
    assert_eq!(m.len(), 1, "the Mob blinds a target once, then waits for another: {m:?}");
    assert_eq!(e.len(), 1, "{e:?}");
    assert!(m[0].abs_diff(e[0]) <= 3, "mob {m:?} ecs {e:?}");
    // Not on easy.
    assert_eq!((mob_blind(false).len(), ecs_blind(false).len()), (0, 0));
}
