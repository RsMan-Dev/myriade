use super::*;
use crate::game::{
    commands::Target,
    native_entity_refs::tests::with_game,
};
use mod_api::{EntityEnteringSectionEvent, Hooks, HostSide};
use std::sync::{Arc, Mutex};

/// What each told crossing carried: client side, entity id, old, new, chunk changed.
type Calls = Arc<Mutex<Vec<(bool, u32, [i32; 3], [i32; 3], bool)>>>;

struct Listener(Calls);

#[mod_api::__subscriptions]
impl Hooks for Listener {
    fn entity_entering_section(&self, host: HostSide<'_>, event: &mut EntityEnteringSectionEvent) {
        self.0.lock().unwrap().push((
            matches!(host, HostSide::Client(_)),
            event.entity.id(),
            event.old_section,
            event.new_section,
            event.chunk_changed,
        ));
    }
}

fn observe(test: impl FnOnce(Calls)) {
    let calls = Calls::default();
    let mods = vec![mod_api::ModEntry::new(
        "sections",
        Box::new(Listener(calls.clone())),
    )]
    .into();
    mod_platform::testing::with_mods(mods, || test(calls));
}

fn scope(level: crate::game::Dim) -> Scope {
    Scope {
        epoch: WorldEpoch::new(1),
        level: level.key(),
    }
}

use crate::game::Dim::{Nether, Overworld};

/// The crossings queued since the last call, as a telling takes them.
fn told(sections: &mut EntitySections) -> Vec<Crossing> {
    match sections.begin_telling() {
        Some(batch) => {
            let out = batch.clone();
            sections.end_telling(batch);
            out
        }
        None => Vec::new(),
    }
}

fn crossing(id: u32, old: [i32; 3], new: [i32; 3]) -> Crossing {
    Crossing { id, old, new }
}

#[test]
fn a_section_is_the_floored_block_shifted_by_four() {
    // Blocks 0..=15 are section 0, -1..=-16 section -1, -17 section -2.
    assert_eq!(section_of([0.0, 15.999, 15.0]), [0, 0, 0]);
    assert_eq!(section_of([16.0, 64.0, 31.5]), [1, 4, 1]);
    assert_eq!(section_of([-0.001, -16.0, -16.001]), [-1, -1, -2]);
    // Java's cast saturates and NaN is 0; `Mth.floor` then wraps `MIN - 1` to `MAX`.
    assert_eq!(section_coord(f64::NAN), 0);
    assert_eq!(section_coord(1e300), i32::MAX >> 4);
    assert_eq!(section_coord(f64::from(i32::MIN)), -134_217_728);
    assert_eq!(section_coord(f64::from(i32::MIN) - 0.5), 134_217_727);
    assert_eq!(section_coord(f64::NEG_INFINITY), 134_217_727);
}

#[test]
fn moving_inside_a_section_is_quiet_and_each_edge_is_one_crossing() {
    let mut s = EntitySections::default();
    let w = scope(Overworld);
    s.placed(w, 5, [15.5, 70.0, 3.0]);
    s.moved(w, 5, None, [1.0, 64.0, 15.9]);
    assert!(told(&mut s).is_empty());
    // East over x = 16, then down over y = 64, then north into negative z.
    s.moved(w, 5, None, [16.2, 64.0, 15.9]);
    s.moved(w, 5, None, [16.2, 63.9, 15.9]);
    s.moved(w, 5, None, [16.2, 63.9, -0.1]);
    assert_eq!(
        told(&mut s),
        [
            crossing(5, [0, 4, 0], [1, 4, 0]),
            crossing(5, [1, 4, 0], [1, 3, 0]),
            crossing(5, [1, 3, 0], [1, 3, -1]),
        ]
    );
}

#[test]
fn two_moves_in_one_tick_through_three_sections_are_two_crossings() {
    let mut s = EntitySections::default();
    let w = scope(Overworld);
    // A, then B and C before anything is told: A -> B and B -> C, not A -> C.
    s.placed(w, 9, [8.0, 70.0, 8.0]);
    s.moved(w, 9, None, [24.0, 70.0, 8.0]);
    s.moved(w, 9, None, [-1000.0, -40.0, 5000.0]);
    assert_eq!(
        told(&mut s),
        [
            crossing(9, [0, 4, 0], [1, 4, 0]),
            crossing(9, [1, 4, 0], [-63, -3, 312]),
        ]
    );
}

#[test]
fn a_move_out_and_back_in_one_tick_is_two_crossings() {
    let mut s = EntitySections::default();
    let w = scope(Overworld);
    s.placed(w, 3, [0.5, 70.0, 0.5]);
    s.moved(w, 3, None, [-0.5, 70.0, 0.5]);
    s.moved(w, 3, None, [0.5, 70.0, 0.5]);
    assert_eq!(
        told(&mut s),
        [
            crossing(3, [0, 4, 0], [-1, 4, 0]),
            crossing(3, [-1, 4, 0], [0, 4, 0]),
        ]
    );
}

#[test]
fn a_move_before_the_first_tick_is_told() {
    let mut s = EntitySections::default();
    let w = scope(Overworld);
    // Filed where it joined, then moved before its first tick.
    s.placed(w, 1, [0.5, 70.0, 0.5]);
    s.moved(w, 1, None, [0.5, 90.0, 0.5]);
    // Moved before its join was filed: the boundary's own start is where it was.
    s.moved(w, 2, Some([40.0, 0.0, 40.0]), [56.0, 0.0, 40.0]);
    // Neither filed nor a known start: filed untold.
    s.moved(w, 4, None, [56.0, 0.0, 40.0]);
    assert_eq!(
        told(&mut s),
        [
            crossing(1, [0, 4, 0], [0, 5, 0]),
            crossing(2, [2, 0, 2], [3, 0, 2]),
        ]
    );
}

#[test]
fn another_level_a_respawn_and_another_world_start_untold() {
    let mut s = EntitySections::default();
    s.placed(scope(Overworld), 4, [0.5, 70.0, 0.5]);
    // Changing level files it in the new one, untold; a move there is told.
    s.moved(scope(Nether), 4, None, [100.0, 70.0, 0.5]);
    s.moved(scope(Nether), 4, None, [116.0, 70.0, 0.5]);
    assert_eq!(told(&mut s), [crossing(4, [6, 4, 0], [7, 4, 0])]);
    // Forgotten (a respawn, a removal): filed again untold.
    s.forget(4);
    s.moved(scope(Nether), 4, None, [0.5, 70.0, 0.5]);
    assert!(told(&mut s).is_empty());
    // Another world open reuses ids: what the last one filed is dropped.
    let next = Scope {
        epoch: WorldEpoch::new(2),
        level: Nether.key(),
    };
    s.moved(next, 4, None, [100.0, 70.0, 0.5]);
    assert!(told(&mut s).is_empty());
}

#[test]
fn what_a_listener_moves_waits_for_the_next_telling() {
    let mut s = EntitySections::default();
    let w = scope(Overworld);
    s.placed(w, 6, [0.5, 70.0, 0.5]);
    s.moved(w, 6, None, [16.5, 70.0, 0.5]);
    let batch = s.begin_telling().unwrap();
    assert_eq!(batch, [crossing(6, [0, 4, 0], [1, 4, 0])]);
    // A listener moves it on while the first crossing is told: not told from inside.
    s.moved(w, 6, None, [32.5, 70.0, 0.5]);
    assert!(s.begin_telling().is_none());
    s.end_telling(batch);
    assert_eq!(told(&mut s), [crossing(6, [1, 4, 0], [2, 4, 0])]);
}

#[test]
#[ignore = "daily"]
fn no_listener_files_no_entity() {
    with_game(|game| {
        let id = game
            .spawn_mob_spawned(
                "cow",
                [1.0, 70.0, 3.0],
                crate::game::creatures::SpawnSite::Plain,
            )
            .unwrap();
        game.teleport_to(Target::Mob(id), [40.0, 70.0, 3.0], None, true);
        assert!(game.native_lifecycle.entity_sections.filed.is_empty());
    });
}

#[test]
#[ignore = "daily"]
fn a_server_mob_moved_twice_in_one_tick_is_told_each_move() {
    observe(|calls| {
        with_game(|game| {
            let id = game
                .spawn_mob_spawned(
                    "cow",
                    [1.0, 70.0, 3.0],
                    crate::game::creatures::SpawnSite::Plain,
                )
                .unwrap();
            // Before its first tick, A -> B -> C by two teleports.
            game.teleport_to(Target::Mob(id), [-0.25, 70.0, 3.0], None, true);
            game.teleport_to(Target::Mob(id), [-0.25, 130.0, 3.0], None, true);
            // Out of the section and back: two more.
            game.teleport_to(Target::Mob(id), [-0.25, 130.0, 20.0], None, true);
            game.teleport_to(Target::Mob(id), [-0.25, 130.0, 3.0], None, true);
            // Inside the section: none.
            game.teleport_to(Target::Mob(id), [-8.0, 129.0, 9.0], None, true);
            assert_eq!(
                calls.lock().unwrap().as_slice(),
                &[
                    (false, id, [0, 4, 0], [-1, 4, 0], true),
                    (false, id, [-1, 4, 0], [-1, 8, 0], false),
                    (false, id, [-1, 8, 0], [-1, 8, 1], true),
                    (false, id, [-1, 8, 1], [-1, 8, 0], true),
                ]
            );
        })
    });
}

#[test]
#[ignore = "daily"]
fn a_mods_random_teleport_of_a_mob_is_told_at_once() {
    observe(|calls| {
        with_game(|game| {
            crate::game::native_living_position::tests::floor_and_zombies(game);
            // The zombie at (0.5, 64, 0.5) is filed where it stands, then sent to land on the
            // floor at (-3.5, 64, -3.5): one section west and one north.
            game.teleport_to(Target::Mob(12), [0.5, 64.0, 0.5], None, true);
            assert!(calls.lock().unwrap().is_empty());
            let zombie = game.living(game.entity(12).unwrap()).unwrap();
            assert!(game.living_random_teleport(zombie, [-3.5, 66.0, -3.5], None));
            assert_eq!(
                calls.lock().unwrap().as_slice(),
                &[(false, 12, [0, 4, 0], [-1, 4, -1], true)]
            );
        })
    });
}

/// A mod's `teleport_to_level` into another level is a change of level, not a crossing: the
/// recreated entity is filed in the new level untold (its join), this call tells nothing of its
/// own, and its next move there over a section edge is told once.
#[test]
#[ignore = "daily"]
fn an_entity_moved_into_another_level_is_told_once_at_its_next_crossing() {
    observe(|calls| {
        with_game(|game| {
            game.native_lifecycle.world_open_ready().unwrap();
            game.summon_with_nbt(
                "minecraft:item",
                [8.5, 80.0, 8.5],
                r#"{Item:{id:"minecraft:stick",count:1},NoGravity:1b}"#,
            )
            .unwrap();
            game.lan_assign_ids();
            let item = game.entity(game.dropped[0].id).unwrap();
            assert!(game.entity_teleport_to_level(
                item,
                "minecraft:the_nether",
                [8.5, 80.0, 8.5],
                None
            ));
            assert!(
                calls.lock().unwrap().is_empty(),
                "the change of level is not told"
            );
            game.enter_level(Nether).unwrap();
            let moved = game.dropped[0].id;
            game.teleport_to(Target::Item(moved), [24.5, 80.0, 8.5], None, true);
            assert_eq!(
                calls.lock().unwrap().as_slice(),
                &[(false, moved, [0, 5, 0], [1, 5, 0], true)]
            );
        })
    });
}
