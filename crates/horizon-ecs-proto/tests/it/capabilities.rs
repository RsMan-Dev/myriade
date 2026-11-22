//! The proof that an entity is what its capability components make of it: an inert object with only a physical body, and a
//! living entity without goals (an armour stand). Each system that has to be changed for these tests to pass is a
//! violation of "no capability supposes another" (see `suivi/ecs-portage/AUDIT-ESPECES.md`, section 9).
//! `HORIZON_TEST_RESOURCES` set.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::cow::*;

/// The components of a physical body and nothing else: no life, no goals, no walking, no species.
fn inert(h: &mut Herd, x: f64, y: f64, z: f64) -> Entity {
    let (w, hgt) = (0.25_f32, 0.25_f32);
    let p = Vec3::new(x, y, z);
    let e = h
        .world
        .spawn((
            Pos(p),
            Vel(Vec3::ZERO),
            Body { width: w, height: hgt, step_height: 0.0, gravity: 0.04, speed: 0.0, fall_factor: 1.0, object: false, orb: false, tnt: false, lava_walker: false },
            Bounds(box_at(p, w, hgt)),
            Cell(u32::MAX, [i32::MIN; 8], u32::MAX),
            Contact { on_ground: false, horizontal: false, fall_distance: 0.0, stuck: 0 },
            FluidState::default(),
            Moving,
        ))
        .id();
    enter_index(&mut h.world, e);
    e
}

/// It falls, comes to rest on the floor, and enters the spatial index.
#[test]
fn an_inert_object_with_only_a_body_falls_and_rests() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let e = inert(&mut h, 0.5, 6.0, 0.5);
    for _ in 0..80 {
        h.tick();
    }
    let y = h.world.get::<Pos>(e).unwrap().0.y;
    assert!(y < 2.0, "the object did not fall (y = {y}): `travel` asks for more than a body");
    assert!(h.world.get::<Contact>(e).unwrap().on_ground, "the object did not land");
    assert!(h.world.get::<Cell>(e).unwrap().0 != u32::MAX, "the object is not in the index");
    assert!(h.world.get::<Moving>(e).is_none(), "the object at rest did not go to sleep");
}

/// The floor under it removed, it falls again (the block-change recheck needs no goals).
#[test]
fn an_inert_object_follows_the_block_changes_under_it() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let e = inert(&mut h, 0.5, 1.0, 0.5);
    for _ in 0..40 {
        h.tick();
    }
    let rest = h.world.get::<Pos>(e).unwrap().0.y;
    h.set_block(0, 0, 0, "minecraft:air");
    for _ in 0..40 {
        h.tick();
    }
    let down = h.world.get::<Pos>(e).unwrap().0.y;
    assert!(down < rest - 0.5, "rest {rest}, after the floor went {down}: the block change did not wake the object");
}

/// A living entity with a body, a health and no goal: a hit hurts it, enough hits kill it, with no selector.
#[test]
fn a_living_entity_without_goals_takes_damage_and_dies() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let e = inert(&mut h, 0.5, 1.0, 0.5);
    h.world.entity_mut(e).insert((Health(6.0), HurtAt { at: 0, last: 0.0 }));
    for _ in 0..5 {
        h.tick();
    }
    h.world.resource_mut::<DamageQueue>().add(e, 2.0, Cause::Entity);
    h.tick();
    let hp = h.world.get::<Health>(e).map(|x| x.0);
    assert_eq!(hp, Some(4.0), "the hit was not applied: `apply_damage` asks for more than a living entity");
    for _ in 0..4 {
        h.world.resource_mut::<DamageQueue>().add(e, 10.0, Cause::Entity);
        for _ in 0..12 {
            h.tick();
        }
    }
    assert!(h.world.get_entity(e).is_err(), "the entity did not die");
}

/// The framework's events: an object that falls lands (`landed`); a burning entity sets `ignited` and puts out (`extinguished`).
#[test]
fn the_framework_events_report_landing_and_fire() {
    if !have_data() {
        return;
    }
    use horizon_ecs_proto::goals::Events;
    let mut h = Herd::with_slab_awake(8, true, false);
    let e = inert(&mut h, 0.5, 5.0, 0.5);
    let mut landed = false;
    for _ in 0..60 {
        h.tick();
        landed |= h.world.resource::<Events>().landed.iter().any(|x| x == e);
    }
    assert!(landed, "no `landed` event for the object that fell");
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let z = spawn_species(&mut h.world, &table[2], 3.5, 3.5, 77);
    h.set_day(true);
    let (mut lit, mut out) = (false, false);
    for _ in 0..400 {
        h.tick();
        let ev = h.world.resource::<Events>();
        lit |= ev.ignited.iter().any(|x| x == z);
        out |= ev.extinguished.iter().any(|x| x == z);
        if h.world.get_entity(z).is_err() {
            break;
        }
    }
    assert!(lit, "no `ignited` event for the zombie in the sun");
    // A death lingers 20 ticks (the fall animation): dead is health 0 as well as gone.
    let dead = h.world.get_entity(z).is_err() || h.world.get::<Health>(z).is_some_and(|x| x.0 <= 0.0);
    assert!(out || dead, "the fire neither went out nor killed");
}

/// A configuration declares its capabilities and `spawn_species` creates those components and no others.
#[test]
fn spawn_makes_only_the_declared_capabilities() {
    if !have_data() {
        return;
    }
    use horizon_ecs_proto::{behaviours::Cap, goals::GoalSet};
    let mut h = Herd::with_slab_awake(8, true, false);
    let mut sp = h.world.resource::<SpeciesTable>().0[0];
    let mut spawn = |h: &mut Herd, caps: &'static [Cap], x: f64| {
        sp.caps = caps;
        spawn_species(&mut h.world, &sp, x, 0.5, 11)
    };
    let object = spawn(&mut h, &[Cap::Transform, Cap::Solid, Cap::Body], 0.5);
    let stand = spawn(&mut h, &[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living], 2.5);
    let full = spawn(&mut h, &[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Locomotion, Cap::Goals], 4.5);
    let has = |h: &Herd, e: Entity| {
        (
            h.world.get::<Health>(e).is_some(),
            h.world.get::<Yaw>(e).is_some() || h.world.get::<Stroll>(e).is_some() || h.world.get::<Nav>(e).is_some(),
            h.world.get::<GoalSet>(e).is_some() || h.world.get::<Rng>(e).is_some() || h.world.get::<ActiveUntil>(e).is_some(),
        )
    };
    assert_eq!(has(&h, object), (false, false, false), "the object has a body only");
    assert_eq!(has(&h, stand), (true, false, false), "the stand lives and has no goals nor walking");
    assert_eq!(has(&h, full), (true, true, true));
    for _ in 0..60 {
        h.tick();
    }
    assert!(h.world.get::<Contact>(object).unwrap().on_ground && h.world.get::<Contact>(stand).unwrap().on_ground);
}

/// An entity with only a position and a velocity (a projectile in flight, a display) is visited by no system of the
/// framework: after a hundred ticks next to creatures it is where it was, with the components it had.
#[test]
fn no_system_of_the_framework_visits_an_entity_with_only_a_position_and_a_velocity() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    for i in 0..4 {
        spawn_species(&mut h.world, &table[0], 0.5 + f64::from(i), 0.5, 5 + i as u64);
    }
    let p = Vec3::new(1.5, 5.0, 0.5);
    let e = h.world.spawn((Pos(p), Vel(Vec3::new(0.1, 0.0, 0.0)))).id();
    let (n, vel) = (h.world.entity(e).archetype().components().len(), Vec3::new(0.1, 0.0, 0.0));
    for _ in 0..100 {
        h.tick();
    }
    assert_eq!(h.world.get::<Pos>(e).unwrap().0, p, "a system moved the entity");
    assert_eq!(h.world.get::<Vel>(e).unwrap().0, vel, "a system changed its velocity");
    assert_eq!(h.world.entity(e).archetype().components().len(), n, "a system added or removed a component");
}

/// `Entry::when`: the framework asks the species' condition before the module's `canUse`. The panic of an adult is given
/// `horizon:is_baby`: struck, the adult does not run, a control adult does; combinations (`Not`, `All`, `Any`) evaluate.
#[test]
fn a_named_condition_gates_an_entry_before_its_module() {
    if !have_data() {
        return;
    }
    use horizon_ecs_proto::{
        behaviours::{panic, Cond, Entry},
        goals::{GoalSet, Phase},
    };
    if horizon_ecs_proto::cow::skip_unless(&panic::MODULE, "a_named_condition_gates_an_entry_before_its_module") {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let (gated, control) = (spawn_species(&mut h.world, &table[0], 0.5, 0.5, 21), spawn_species(&mut h.world, &table[0], 4.5, 4.5, 22));
    for _ in 0..20 {
        h.tick();
    }
    static BABY: Cond = Cond::Is("horizon:is_baby");
    static NOT_BABY_AND_DAY: Cond = Cond::All(&[Cond::Not(&BABY), Cond::Is("horizon:on_ground")]);
    assert!(!BABY.eval(&h.world, gated) && NOT_BABY_AND_DAY.eval(&h.world, gated), "the conditions read the entity");
    let entries: Vec<Entry> = h.world.get::<GoalSet>(gated).unwrap().entries().iter().map(|e| if horizon_ecs_proto::behaviours::same(e.module, &panic::MODULE) { e.when(&BABY) } else { *e }).collect();
    h.world.get_mut::<GoalSet>(gated).unwrap().replace_entries(Box::leak(entries.into_boxed_slice()));
    let _ = Phase(0);
    for e in [gated, control] {
        h.world.resource_mut::<DamageQueue>().add(e, 1.0, Cause::Entity);
    }
    let (mut c, mut g) = (false, false);
    for _ in 0..12 {
        h.tick();
        c |= h.world.get::<horizon_ecs_proto::behaviours::panic::Panicking>(control).is_some();
        g |= h.world.get::<horizon_ecs_proto::behaviours::panic::Panicking>(gated).is_some();
    }
    assert!(c, "the control adult flees");
    assert!(!g, "the adult whose panic needs a baby does not");
}

/// `Module::interruptable`: a running goal that cannot be interrupted keeps its flags against a goal of a better priority;
/// the same goal interruptible gives them up.
#[test]
fn a_goal_that_is_not_interruptable_keeps_its_flags() {
    use horizon_ecs_proto::{behaviours::{Entry, Module}, goals::{GoalSet, MOVE}};
    static STUBBORN: Module = Module { name: "stubborn", flags: MOVE, interruptable: false, ..Module::DEFAULT };
    static SOFT: Module = Module { name: "soft", flags: MOVE, ..Module::DEFAULT };
    static URGENT: Module = Module { name: "urgent", flags: MOVE, ..Module::DEFAULT };
    static E1: [Entry; 2] = [Entry::new(&STUBBORN, 5, &()), Entry::new(&URGENT, 1, &())];
    static E2: [Entry; 2] = [Entry::new(&SOFT, 5, &()), Entry::new(&URGENT, 1, &())];
    let mut a = GoalSet::new(&E1);
    a.start(0);
    assert!(!a.replaceable(1), "the stubborn goal keeps MOVE");
    let mut b = GoalSet::new(&E2);
    b.start(0);
    assert!(b.replaceable(1), "an interruptible goal gives MOVE to a better priority");
}

/// `goals::interact`: the first module that answers handles the item (the cow's food puts it in love), then the rules (the
/// sheep's shears); anything else is refused.
#[test]
fn a_player_item_is_handled_by_the_module_or_the_rule_that_wants_it() {
    if !have_data() {
        return;
    }
    use horizon_ecs_proto::{behaviours::Interaction, goals::{interact, item_id, Love}};
    let mut h = Herd::with_slab_awake(8, true, false);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let named = |n: &str| table.iter().find(|s| s.name == n).copied().unwrap();
    let cow = spawn_species(&mut h.world, &named("minecraft:cow"), 0.5, 0.5, 1);
    let sheep = spawn_species(&mut h.world, &named("minecraft:sheep"), 3.5, 0.5, 2);
    for _ in 0..10 {
        h.tick();
    }
    let with = |item: &str| Interaction { player: 1, item: item_id(item) };
    assert!(!interact(&mut h.world, cow, &with("minecraft:shears")), "a cow is not shorn");
    assert!(!interact(&mut h.world, cow, &with("minecraft:carrot")), "a cow does not eat a carrot");
    assert!(interact(&mut h.world, cow, &with("minecraft:wheat")) && h.world.get::<Love>(cow).is_some(), "wheat puts a cow in love");
    assert!(interact(&mut h.world, sheep, &with("minecraft:shears")), "the sheep is shorn");
    assert!(!interact(&mut h.world, sheep, &with("minecraft:shears")), "a shorn sheep is not shorn again");
}

/// Block states with properties: a bush placed with `age = 3` reads back as 3; an unknown property or value is refused.
#[test]
fn a_block_is_written_and_read_with_its_properties() {
    if !have_data() {
        return;
    }
    use horizon_ecs_proto::blocks;
    let mut h = Herd::with_slab_awake(8, true, false);
    h.set_block_state(2, 1, 2, "minecraft:sweet_berry_bush", &[("age", "3")]);
    h.set_block_state(3, 1, 2, "minecraft:sweet_berry_bush", &[("age", "9")]);
    h.set_block_state(4, 1, 2, "minecraft:sweet_berry_bush", &[("size", "1")]);
    h.tick();
    let mw = blocks::world_of(&h.world);
    assert_eq!(blocks::int_property(mw, [2, 1, 2], "age"), Some(3));
    assert_eq!(blocks::property(mw, [2, 1, 2], "size"), None, "a bush has no `size`");
    assert_ne!(mw.block_name(3, 1, 2), "minecraft:sweet_berry_bush", "age 9 does not exist: nothing is placed");
    assert_ne!(mw.block_name(4, 1, 2), "minecraft:sweet_berry_bush", "an unknown property: nothing is placed");
    h.set_block_state(2, 1, 2, "minecraft:sweet_berry_bush", &[("age", "1")]);
    h.tick();
    assert_eq!(blocks::int_property(blocks::world_of(&h.world), [2, 1, 2], "age"), Some(1));
}

/// Range events: `PlayerWatch(10)` reports a player coming within 10 blocks and going, once each, by the player's moves; the gaze
/// change and the feet leaving the ground are events too.
#[test]
fn players_entering_and_leaving_a_radius_the_gaze_and_leaving_the_ground_are_events() {
    if !have_data() {
        return;
    }
    use horizon_ecs_proto::{goals::{Events, Online, Player, PlayerWatch}, hostile::PlayerState};
    let mut h = Herd::with_slab_awake(30, true, false);
    let e = inert(&mut h, 0.5, 1.0, 0.5);
    h.world.entity_mut(e).insert(PlayerWatch::new(10.0));
    for _ in 0..30 {
        h.tick();
    }
    let (mut entered, mut left, mut gaze, mut off) = (0, 0, 0, 0);
    let mut count = |h: &Herd, entered: &mut i32, left: &mut i32, gaze: &mut i32, off: &mut i32| {
        let ev = h.world.resource::<Events>();
        *entered += ev.player_entered.iter().filter(|&(w, p)| w == e && p == 1).count() as i32;
        *left += ev.player_left.iter().filter(|&(w, p)| w == e && p == 1).count() as i32;
        *gaze += ev.gaze_changed.iter().filter(|&p| p == 1).count() as i32;
        *off += ev.left_ground.iter().filter(|&x| x == e).count() as i32;
    };
    let at = |h: &mut Herd, x: f64| h.world.resource_mut::<Online>().set(Player { id: 1, x, y: 1.0, z: 0.5, main: 0, off: 0, ..Default::default() });
    at(&mut h, 25.0);
    for _ in 0..3 { h.tick(); count(&h, &mut entered, &mut left, &mut gaze, &mut off); }
    assert_eq!((entered, left), (0, 0), "far away: nothing");
    at(&mut h, 6.0);
    for _ in 0..3 { h.tick(); count(&h, &mut entered, &mut left, &mut gaze, &mut off); }
    assert_eq!((entered, left), (1, 0), "within 10 blocks: once");
    at(&mut h, 4.0);
    for _ in 0..3 { h.tick(); count(&h, &mut entered, &mut left, &mut gaze, &mut off); }
    assert_eq!((entered, left), (1, 0), "still inside: no second event");
    at(&mut h, 20.0);
    for _ in 0..3 { h.tick(); count(&h, &mut entered, &mut left, &mut gaze, &mut off); }
    assert_eq!((entered, left), (1, 1), "gone: once");
    h.world.resource_mut::<PlayerState>().set_gaze(1, [1.0, 0.0, 0.0]);
    horizon_ecs_proto::goals::impulse(&mut h.world, e, Vec3::new(0.0, 0.5, 0.0));
    for _ in 0..4 { h.tick(); count(&h, &mut entered, &mut left, &mut gaze, &mut off); }
    assert_eq!(gaze, 1, "the gaze changed once");
    assert_eq!(off, 1, "the feet left the ground once");
}

/// A hundred players around one watcher: each one is reported once on entering and once on leaving, none is taken for another
/// (the watch held a 64-bit mask by player id, so the 70th was the 6th).
#[test]
fn a_hundred_players_are_each_reported_once_by_a_watcher() {
    if !have_data() {
        return;
    }
    use horizon_ecs_proto::goals::{Events, Online, Player, PlayerWatch};
    let mut h = Herd::with_slab_awake(30, true, false);
    let e = inert(&mut h, 0.5, 1.0, 0.5);
    h.world.entity_mut(e).insert(PlayerWatch::new(10.0));
    for _ in 0..30 {
        h.tick();
    }
    let mut entered = std::collections::HashMap::<u32, i32>::new();
    let mut left = std::collections::HashMap::<u32, i32>::new();
    let mut run = |h: &mut Herd, entered: &mut std::collections::HashMap<u32, i32>, left: &mut std::collections::HashMap<u32, i32>| {
        for _ in 0..3 {
            h.tick();
            let ev = h.world.resource::<Events>();
            for (w, p) in ev.player_entered.iter() {
                if w == e {
                    *entered.entry(p).or_default() += 1;
                }
            }
            for (w, p) in ev.player_left.iter() {
                if w == e {
                    *left.entry(p).or_default() += 1;
                }
            }
        }
    };
    for id in 1..=100u32 {
        h.world.resource_mut::<Online>().set(Player { id, x: 4.0 + f64::from(id % 5) * 0.1, y: 1.0, z: 0.5, main: 0, off: 0, ..Default::default() });
    }
    run(&mut h, &mut entered, &mut left);
    assert_eq!(entered.len(), 100, "every player entered: {}", entered.len());
    assert!(entered.values().all(|&n| n == 1) && left.is_empty());
    for id in 1..=100u32 {
        h.world.resource_mut::<Online>().set(Player { id, x: 30.0, y: 1.0, z: 0.5, main: 0, off: 0, ..Default::default() });
    }
    run(&mut h, &mut entered, &mut left);
    assert_eq!(left.len(), 100, "every player left: {}", left.len());
    assert!(left.values().all(|&n| n == 1));
}

/// `PlayerWatch` covers the whole player id range (it was a `u64` mask: ids >= 64 never fired).
#[test]
fn range_events_work_for_player_ids_beyond_64() {
    if !have_data() {
        return;
    }
    use horizon_ecs_proto::goals::{Events, Online, Player, PlayerWatch};
    let mut h = Herd::with_slab_awake(30, true, false);
    let e = inert(&mut h, 0.5, 1.0, 0.5);
    h.world.entity_mut(e).insert(PlayerWatch::new(10.0));
    for _ in 0..30 {
        h.tick();
    }
    for id in [70u32, 5000] {
        let (mut entered, mut left) = (0, 0);
        for x in [25.0, 6.0, 4.0, 20.0] {
            h.world.resource_mut::<Online>().set(Player { id, x, y: 1.0, z: 0.5, main: 0, off: 0, ..Default::default() });
            for _ in 0..3 {
                h.tick();
                let ev = h.world.resource::<Events>();
                entered += ev.player_entered.iter().filter(|&(w, p)| w == e && p == id).count();
                left += ev.player_left.iter().filter(|&(w, p)| w == e && p == id).count();
            }
        }
        assert_eq!((entered, left), (1, 1), "player {id}: one entry, one exit");
    }
}
