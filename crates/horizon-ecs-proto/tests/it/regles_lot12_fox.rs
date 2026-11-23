//! Lot 12 of the rules of the old fox (`mobs/fox.rs`): the two trusted slots, the trust a baby takes from its parents' feeders,
//! what a fox holds in its mouth, and the alarm of an invisible player. Expected values are the vanilla 26.3 ones (`javap` of
//! `Fox` and `Fox$FoxBreedGoal`).
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_client_lib::items::ItemStack;
use horizon_ecs_proto::behaviours::breed::{feed, set_love_cause};
use horizon_ecs_proto::behaviours::defend_trusted;
use horizon_ecs_proto::behaviours::eat_held;
use horizon_ecs_proto::behaviours::fox_sleep::FoxSleeping;
use horizon_ecs_proto::behaviours::shelter;
use horizon_ecs_proto::behaviours::trust::{self, Trust};
use horizon_ecs_proto::cow::{now_of, spawn_armed, spawn_species, Drive, FluidState, Herd, SpeciesTable};
use horizon_ecs_proto::dragon::flight::NoAi;
use horizon_ecs_proto::goals::{set_main_hand, GoalSet, Online, Player};
use horizon_ecs_proto::host_out::HostEvent;
use horizon_ecs_proto::hostile::PlayerState;
use horizon_ecs_proto::rules::fox_pick_up_loot::can_hold_item;
use horizon_ecs_proto::rules::fox_sleep::{alarm_range, player_alarm_reach};
use horizon_ecs_proto::targeting::Who;

#[test]
fn a_fox_trusts_two_players_and_the_third_replaces_the_second() {
    let mut t = Trust::default();
    t.add(Who::Player(1));
    t.add(Who::Player(2));
    assert!(t.trusts_player(1) && t.trusts_player(2), "the first two players fill the two slots");
    t.add(Who::Player(3));
    assert!(t.trusts_player(1) && t.trusts_player(3), "the third player takes the second slot");
    assert!(!t.trusts_player(2), "the second player is replaced");
}

#[test]
fn a_baby_fox_trusts_both_players_who_fed_its_parents() {
    let both = trust::breeders(Some(1), Some(2));
    assert!(both.trusts_player(1) && both.trusts_player(2), "the mother's and the father's feeders are trusted");
    let same = trust::breeders(Some(1), Some(1));
    assert_eq!(same.0.iter().flatten().count(), 1, "the same feeder is added once");
    assert!(trust::breeders(None, None).0.iter().all(Option::is_none), "no feeder, no trust");
}

#[test]
fn a_fox_holds_what_its_mouth_has_room_for() {
    assert!(can_hold_item(true, 0, false, false), "an empty mouth takes anything");
    assert!(!can_hold_item(false, 0, false, false), "a stick is refused while the mouth is full");
    assert!(!can_hold_item(false, 1, false, false), "a stick is refused even one tick after eating");
    assert!(!can_hold_item(false, 0, true, false), "food is refused right after eating");
    assert!(can_hold_item(false, 1, true, false), "food is taken one tick after eating");
    assert!(!can_hold_item(false, 1, true, true), "food does not replace food in the mouth");
}

#[test]
fn an_invisible_player_far_away_is_not_alerting() {
    // Three blocks away (squared distance 9): the visibility 0.1 scales the alarm to 2 blocks, the visibility 1 keeps 12.
    assert_eq!(alarm_range(0.1), 2.0, "an invisible player alarms within 2 blocks");
    assert!(!player_alarm_reach(9.0, alarm_range(0.1)), "an invisible player at 3 blocks does not alarm the fox");
    assert!(player_alarm_reach(9.0, alarm_range(1.0)), "a visible player at 3 blocks alarms the fox");
}

/// A fox at the origin, by day, under a stone roof (the nap's shade), not yet asleep.
fn under_roof(seed: u64) -> (Herd, Entity) {
    let mut h = Herd::with_slab_awake(30, false, false);
    h.set_day(true);
    for x in -2..=3 {
        for z in -2..=3 {
            h.set_block(x, 4, z, "minecraft:stone");
        }
    }
    h.tick();
    let sp = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == "minecraft:fox").unwrap();
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, seed);
    (h, e)
}

/// Whether the fox naps within 400 ticks; `moving` gives it a movement input every tick.
fn naps(h: &mut Herd, e: Entity, moving: bool) -> bool {
    for _ in 0..400 {
        if moving {
            h.world.get_mut::<Drive>(e).unwrap().zza = 0.5;
        }
        h.tick();
        if h.world.get::<FoxSleeping>(e).is_some() {
            return true;
        }
    }
    false
}

#[test]
fn a_sleeping_fox_wakes_in_water_under_ai_and_keeps_its_nap_under_no_ai() {
    if !have_data() || wrong_species(&["minecraft:fox"]) {
        return;
    }
    let water = || FluidState { in_water: true, eye_in_water: true, ..Default::default() };
    let (mut h, e) = under_roof(11);
    assert!(naps(&mut h, e, false), "the fox naps under the roof");
    h.world.entity_mut(e).insert(water());
    h.tick();
    assert!(h.world.get::<FoxSleeping>(e).is_none(), "under AI a sleeping fox in water wakes");
    let (mut h, e) = under_roof(11);
    assert!(naps(&mut h, e, false), "the fox naps under the roof");
    h.world.entity_mut(e).insert((NoAi, water()));
    for _ in 0..20 {
        h.tick();
    }
    assert!(h.world.get::<FoxSleeping>(e).is_some(), "under NoAi the nap is kept in water");
}

#[test]
fn a_fox_with_movement_input_does_not_sleep() {
    if !have_data() || wrong_species(&["minecraft:fox"]) {
        return;
    }
    let (mut h, e) = under_roof(3);
    assert!(naps(&mut h, e, false), "a still fox naps under the roof");
    let (mut h, e) = under_roof(3);
    assert!(!naps(&mut h, e, true), "a fox with a movement input does not nap");
}

#[test]
fn a_village_fox_does_not_look_for_shade() {
    if !have_data() || wrong_species(&["minecraft:fox"]) {
        return;
    }
    // A bell in the fox's section makes it a village section; without it the fox is outside any village.
    let seeks = |bell: bool| -> bool {
        let mut h = Herd::with_slab_awake(30, false, false);
        h.set_day(true);
        if bell {
            h.set_block(5, 1, 3, "minecraft:bell");
            h.tick();
        }
        let sp = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == "minecraft:fox").unwrap();
        let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 5);
        for _ in 0..400 {
            h.tick();
            if h.world.get::<GoalSet>(e).is_some_and(|g| g.is_running(&shelter::MODULE)) {
                return true;
            }
        }
        false
    };
    assert!(seeks(false), "outside a village the fox looks for shade");
    assert!(!seeks(true), "inside a village it stays in the open");
}

#[test]
fn a_fox_defends_the_player_it_trusts() {
    if !have_data() || wrong_species(&["minecraft:fox", "minecraft:zombie"]) {
        return;
    }
    let defends = |trusted: bool| -> bool {
        let mut h = Herd::with_slab_awake(30, false, false);
        let sp = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == "minecraft:fox").unwrap();
        let fox = spawn_species(&mut h.world, &sp, 0.5, 0.5, 6);
        let zsp = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == "minecraft:zombie").unwrap();
        let zombie = spawn_species(&mut h.world, &zsp, 2.5, 0.5, 7);
        h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5, y: 1.0, z: 1.5, main: 0, off: 0, ..Default::default() });
        if trusted {
            let mut t = Trust::default();
            t.add(Who::Player(1));
            h.world.entity_mut(fox).insert(t);
        }
        // The player was hurt by the zombie just now (`getLastHurtByMob`).
        let now = now_of(&h.world, fox);
        h.world.resource_mut::<PlayerState>().set_hurt_by(1, Who::Entity(zombie), now);
        // The target goal rolls one try in five: give it turns.
        for _ in 0..40 {
            h.tick();
            if defend_trusted::is_defending(&h.world, fox) {
                return true;
            }
        }
        false
    };
    assert!(defends(true), "a fox defends the player it trusts");
    assert!(!defends(false), "a fox does not defend a player it does not trust");
}

/// Two foxes in love, fed by players 1 and 2, give birth on the ECS path: the baby trusts both feeders.
#[test]
fn a_fox_birth_gives_the_baby_the_trust_of_its_feeders() {
    if !have_data() || wrong_species(&["minecraft:fox"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let sp = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == "minecraft:fox").unwrap();
    let a = spawn_armed(&mut h.world, &sp, 0.5, 0.5, 71);
    let b = spawn_armed(&mut h.world, &sp, 2.5, 0.5, 72);
    for e in [a, b] {
        h.world.get_mut::<horizon_ecs_proto::cow::ActiveUntil>(e).unwrap().0 = u64::MAX;
    }
    h.tick();
    assert!(feed(&mut h.world, a) && feed(&mut h.world, b), "both foxes are fed into love");
    set_love_cause(&mut h.world, a, Some(1));
    set_love_cause(&mut h.world, b, Some(2));
    let mut baby = None;
    for _ in 0..600 {
        h.tick();
        baby = h.world.query::<(Entity, &Trust)>().iter(&h.world).map(|(e, t)| (e, *t)).find(|(e, _)| *e != a && *e != b).map(|(_, t)| t);
        if baby.is_some() {
            break;
        }
    }
    let trust = baby.expect("a baby fox is born and carries a trust");
    assert!(trust.trusts_player(1) && trust.trusts_player(2), "the baby trusts the players who fed its parents");
}

/// A fox with a berry in its mouth and no target asks the host to eat it, once its delay is over: the event carries the stack.
#[test]
fn a_fox_asks_the_host_to_eat_the_berry_in_its_mouth() {
    if !have_data() || wrong_species(&["minecraft:fox"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(30, false, false);
    let sp = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == "minecraft:fox").unwrap();
    let e = spawn_armed(&mut h.world, &sp, 0.5, 0.5, 9);
    h.tick();
    h.drain_host_out();
    set_main_hand(&mut h.world, e, Some(ItemStack::new("minecraft:sweet_berries", 2)));
    eat_held::restart(&mut h.world, e);
    let mut asked = None;
    for _ in 0..700 {
        h.tick();
        asked = h.drain_host_out().into_iter().find_map(|ev| if let HostEvent::HeldConsume { stack, .. } = ev { Some(stack) } else { None });
        if asked.is_some() {
            break;
        }
    }
    let stack = asked.expect("the host is asked to eat the berry");
    assert_eq!((&*stack.item, stack.count), ("minecraft:sweet_berries", 2), "the stack held is the one eaten");
}
