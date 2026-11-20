//! The dragon's ports that Horizon's `Dragon` has no golden for: the kill credit, the choice of the four crystals, `no_ai`, the walls, the
//! rise of the parts, the `Dying` window and the crystal hook. Each asserts the vanilla rule it ports (`ETAT-dragon.md`).
use crate::dragon_support::*;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::blocks::{set_state, state_id, world_of};
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::dragon::death::DeathTime;
use horizon_ecs_proto::dragon::fight::{EndFight, FightEvents};
use horizon_ecs_proto::dragon::flight::Flight;
use horizon_ecs_proto::dragon::hurt::Hurt;
use horizon_ecs_proto::dragon::parts::Parts;
use horizon_ecs_proto::dragon::phases::PhaseId;

fn fight_herd() -> Herd {
    let mut h = herd();
    h.world.insert_resource(EndFight::default());
    h.world.init_resource::<FightEvents>();
    h
}

fn credits(h: &Herd) -> usize {
    h.world.resource::<FightEvents>().credit.iter().count()
}

#[test]
fn a_kill_within_100_ticks_of_a_player_hit_is_credited_and_one_after_is_not() {
    if !crate::common::have_data() {
        return;
    }
    let mut h = fight_herd();
    let e = dragon(&mut h, 10, 3.0);
    assert!(matches!(blow(&mut h, e, 0, 9.0, 'K'), Hurt::Landed { .. }));
    h.tick();
    assert_eq!(credits(&h), 1);
    let mut h = fight_herd();
    let e = dragon(&mut h, 10, 200.0);
    blow(&mut h, e, 0, 5.0, 'K');
    for _ in 0..101 {
        h.tick();
    }
    blow(&mut h, e, 0, 500.0, 'E');
    h.tick();
    assert_eq!(credits(&h), 0, "the hit is older than 100 ticks");
}

#[test]
fn try_respawn_needs_the_four_crystals_and_a_killed_fight() {
    if !crate::common::have_data() {
        return;
    }
    let mut h = fight_herd();
    h.world.resource_mut::<EndFight>().exit_portal = Some([0, 64, 0]);
    let at = |d: [i32; 2]| Vec3::new(f64::from(d[0] * 3) + 0.5, 65.0, f64::from(d[1] * 3) + 0.5);
    for d in [[0, -1], [0, 1], [-1, 0]] {
        horizon_ecs_proto::dragon::crystals::spawn(&mut h.world, at(d), false);
    }
    assert!(!horizon_ecs_proto::dragon::respawn::try_respawn(&mut h.world), "not killed");
    h.world.resource_mut::<EndFight>().killed = true;
    assert!(!horizon_ecs_proto::dragon::respawn::try_respawn(&mut h.world), "three sides only");
    assert!(h.world.resource::<EndFight>().respawn.is_none());
    horizon_ecs_proto::dragon::crystals::spawn(&mut h.world, at([1, 0]), false);
    assert!(horizon_ecs_proto::dragon::respawn::try_respawn(&mut h.world));
    let f = h.world.resource::<EndFight>();
    assert!(f.respawn.is_some());
    assert_eq!(f.respawn_crystals.len(), 4);
    assert!(!horizon_ecs_proto::dragon::respawn::try_respawn(&mut h.world), "a respawn is running");
}

#[test]
fn no_ai_freezes_the_dragon_and_clearing_it_frees_it() {
    if !crate::common::have_data() {
        return;
    }
    let mut h = herd();
    let e = dragon(&mut h, 0, 200.0);
    horizon_ecs_proto::dragon::flight::set_no_ai(&mut h.world, e, true);
    let before = h.world.get::<Pos>(e).unwrap().0;
    for _ in 0..5 {
        h.tick();
    }
    assert_eq!(h.world.get::<Pos>(e).unwrap().0, before);
    horizon_ecs_proto::dragon::flight::set_no_ai(&mut h.world, e, false);
    for _ in 0..5 {
        h.tick();
    }
    assert_ne!(h.world.get::<Pos>(e).unwrap().0, before);
}

fn fill(h: &mut Herd, name: &'static str) {
    for y in 72..=90 {
        for z in -8..=8 {
            for x in -8..=8 {
                set_state(&mut h.world, [x, y, z], name, &[]);
            }
        }
    }
}

/// One step that applies the queued blocks while the dragon is frozen (`no_ai`), so it has not touched them yet.
fn frozen_tick(h: &mut Herd, e: bevy_ecs::entity::Entity) {
    horizon_ecs_proto::dragon::flight::set_no_ai(&mut h.world, e, true);
    h.tick();
    horizon_ecs_proto::dragon::flight::set_no_ai(&mut h.world, e, false);
}

fn solid(h: &Herd) -> usize {
    let mw = world_of(&h.world);
    let mut n = 0;
    for y in 72..=90 {
        for z in -8..=8 {
            for x in -8..=8 {
                n += usize::from(state_id(mw, [x, y, z]) != state_id(mw, [30, 120, 30]));
            }
        }
    }
    n
}

#[test]
fn the_walls_break_for_the_dragon_unless_immune_and_then_it_is_in_a_wall() {
    if !crate::common::have_data() {
        return;
    }
    let mut h = herd();
    let e = dragon(&mut h, 10, 200.0);
    fill(&mut h, "minecraft:stone");
    frozen_tick(&mut h, e);
    let full = 17 * 19 * 17;
    assert_eq!(solid(&h), full);
    for _ in 0..3 {
        h.tick();
    }
    assert!(solid(&h) < full, "the dragon breaks stone");
    assert!(!h.world.get::<Flight>(e).unwrap().in_wall);
    let mut h = herd();
    let e = dragon(&mut h, 10, 200.0);
    fill(&mut h, "minecraft:obsidian");
    frozen_tick(&mut h, e);
    for _ in 0..3 {
        h.tick();
    }
    assert_eq!(solid(&h), full, "obsidian is dragon_immune");
    assert!(h.world.get::<Flight>(e).unwrap().in_wall);
}

#[test]
fn the_parts_rise_with_the_dragon_while_it_dies() {
    if !crate::common::have_data() {
        return;
    }
    let mut h = fight_herd();
    let e = dragon(&mut h, 10, 200.0);
    h.tick();
    h.world.entity_mut(e).insert(Health(0.0));
    h.tick();
    let (y0, p0) = (h.world.get::<Pos>(e).unwrap().0.y, h.world.get::<Parts>(e).unwrap().0[0].pos[1]);
    for _ in 0..10 {
        h.tick();
    }
    let (y1, p1) = (h.world.get::<Pos>(e).unwrap().0.y, h.world.get::<Parts>(e).unwrap().0[0].pos[1]);
    assert_eq!(y1 - y0, 10.0 * f64::from(0.1_f32));
    assert!((p1 - p0 - (y1 - y0)).abs() < 1e-9, "the head rises as much: {p0} -> {p1}");
}

#[test]
fn a_dying_dragon_refuses_blows_and_dies_in_200_ticks() {
    if !crate::common::have_data() {
        return;
    }
    let mut h = fight_herd();
    let e = dragon(&mut h, 0, 3.0);
    assert!(matches!(blow(&mut h, e, 0, 9.0, 'K'), Hurt::Landed { .. }));
    assert_eq!(h.world.get::<PhaseId>(e).unwrap().0, 9, "Dying");
    let last = h.world.get::<horizon_ecs_proto::dragon::hurt::DragonHurt>(e).unwrap().last;
    h.world.entity_mut(e).insert(horizon_ecs_proto::dragon::hurt::DragonHurt { window: 0, time: 0, last });
    assert!(matches!(blow(&mut h, e, 0, 9.0, 'K'), Hurt::Refused | Hurt::Shrugged));
    for _ in 0..400 {
        h.tick();
        if h.world.get_entity(e).is_err() {
            return;
        }
    }
    panic!("the dying dragon never ended: death time {:?}", h.world.get::<DeathTime>(e).map(|d| d.0));
}
