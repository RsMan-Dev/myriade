//! Lot 12 of the rules of the mobs, the tame animals (`tame.rs`): the shake of a wet wolf, a hit that stands a sitting pet up, the mate
//! of a sitting pet, the begging of a sitting wolf, the spectator who does not make a wolf beg, a cat that stops fleeing or being
//! tempted once tamed. Test names are those of the original `tame.rs` tests. Expected values are the vanilla 26.3 ones (`javap`:
//! `Wolf.tick` shakeAnim += 0.05 and `ENTITY_ACTION`, `Wolf.hurtServer` and `Parrot.hurtServer` call `setOrderedToSit(false)`, `Cat`
//! and `TamableAnimal` have no `hurtServer`, `Wolf.canMate` refuses a partner that sits, `TemptGoal.canContinueToUse` ends with `canUse`).
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_ecs_proto::behaviours::beg::Begging;
use horizon_ecs_proto::behaviours::breed::{can_mate, Love};
use horizon_ecs_proto::behaviours::shake_off_water::{Shaking, Wet};
use horizon_ecs_proto::behaviours::tempt::Tempts;
use horizon_ecs_proto::cow::{spawn_species, Herd, Pos, SpeciesTable};
use horizon_ecs_proto::goals::{Events, Occupied, Online, Player, OCC_SITTING};
use horizon_ecs_proto::host_out::HostEvent;
use horizon_ecs_proto::hostile::PlayerState;
use horizon_ecs_proto::items::item_id;
use horizon_ecs_proto::behaviours::celebrate::Kill;
use horizon_ecs_proto::behaviours::sit_when_ordered::SitPose;
use horizon_ecs_proto::tame::{order_sit, tame};
use horizon_ecs_proto::targeting::Who;

/// The species of the table by name.
fn spawn(h: &mut Herd, name: &str, x: f64, z: f64) -> Entity {
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == name).unwrap();
    spawn_species(&mut h.world, &sp, x, z, 5)
}

/// The host events of this tick: the entity events (codes) and the sounds and game events by name.
fn host_events(h: &mut Herd) -> (Vec<u8>, Vec<String>) {
    let mut codes = Vec::new();
    let mut names = Vec::new();
    for e in h.world.resource_mut::<Events>().host.drain() {
        match e {
            HostEvent::EntityEvent { code, .. } => codes.push(code),
            HostEvent::Sound { name, .. } => names.push(name.to_string()),
            HostEvent::GameEventBy { name, .. } => names.push(name.to_string()),
            _ => {}
        }
    }
    (codes, names)
}

/// A player at `x` holding `item`, a spectator when `spectator`.
fn player_with(h: &mut Herd, x: f64, item: u32, spectator: bool) {
    h.world.resource_mut::<Online>().set(Player { id: 1, x, y: 1.0, z: 0.5, main: item, off: 0, ..Default::default() });
    if spectator {
        h.world.resource_mut::<PlayerState>().spectator = vec![false, true];
    }
}

/// `Wolf.tick`: `isInWaterOrRain` wets a wolf that moves into the rain (read at the block it enters); without rain nothing is read.
#[test]
fn a_wolf_that_walks_into_the_rain_gets_wet() {
    if !have_data() || wrong_species(&["minecraft:wolf"]) {
        return;
    }
    for raining in [true, false] {
        let mut h = Herd::with_slab_awake(20, false, false);
        let e = spawn(&mut h, "minecraft:wolf", 0.0, 7.0);
        h.set_raining(raining);
        h.world.get_mut::<Pos>(e).unwrap().0.x += 1.0;
        h.tick();
        assert_eq!(h.world.get::<Wet>(e).is_some(), raining, "a wolf moving into the rain is wet only while it rains");
    }
}

/// `Wolf.aiStep`: a wet wolf that stands on the ground, out of the water and not pathing starts to shake, and the clients are told (8).
#[test]
fn a_server_tells_the_clients_when_a_wolf_shakes() {
    if !have_data() || wrong_species(&["minecraft:wolf"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    let e = spawn(&mut h, "minecraft:wolf", 0.0, 7.0);
    h.world.entity_mut(e).insert(Wet);
    h.tick();
    assert!(h.world.get::<Shaking>(e).is_some(), "the wet wolf shakes once it is out of the water");
    let (codes, _) = host_events(&mut h);
    assert!(codes.contains(&8), "the clients are told the wolf shakes (entity event 8)");
}

/// `Wolf.tick` on the first tick of a shake: the shake's sound and the `ENTITY_ACTION` game event.
#[test]
fn a_shake_posts_entity_action_on_the_server() {
    if !have_data() || wrong_species(&["minecraft:wolf"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    let e = spawn(&mut h, "minecraft:wolf", 0.0, 7.0);
    h.world.entity_mut(e).insert((Wet, Shaking(0.0)));
    h.tick();
    let (_, names) = host_events(&mut h);
    assert!(names.iter().any(|n| n == "minecraft:entity_action"), "the shake posts ENTITY_ACTION");
    assert!(names.iter().any(|n| n == "entity.wolf.shake"), "the shake plays the wolf's shake sound");
}

/// `Wolf.die`: a wolf killed mid-shake is dry and still, and no clients are told.
#[test]
fn a_wolf_that_dies_mid_shake_is_dry_and_still() {
    if !have_data() || wrong_species(&["minecraft:wolf"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    let e = spawn(&mut h, "minecraft:wolf", 0.0, 7.0);
    h.world.entity_mut(e).insert((Wet, Shaking(0.5)));
    h.world.resource_mut::<Events>().killed.push(Kill { victim: e, killer: None, species: u8::MAX, at: [0.0; 3] });
    h.tick();
    assert!(h.world.get::<Shaking>(e).is_none(), "the shake stops with the wolf");
    assert!(h.world.get::<Wet>(e).is_none(), "the wolf is dry once dead");
    let (codes, _) = host_events(&mut h);
    assert!(!codes.contains(&8) && !codes.contains(&56), "no shake event is sent for a dead wolf");
}

/// `Wolf.handleEntityEvent` 56 (`cancelShake`): entering water cancels a shake in progress, for the clients too.
#[test]
fn a_wolf_of_a_server_stops_shaking_on_its_cancel_event() {
    if !have_data() || wrong_species(&["minecraft:wolf"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    let e = spawn(&mut h, "minecraft:wolf", 0.0, 7.0);
    h.world.entity_mut(e).insert((Wet, Shaking(0.5)));
    h.world.resource_mut::<Events>().entered_fluid.push(e);
    h.tick();
    assert!(h.world.get::<Shaking>(e).is_none(), "entering water stops the shake");
    let (codes, _) = host_events(&mut h);
    assert!(codes.contains(&56), "the clients are told the shake is cancelled (entity event 56)");
}

/// `Wolf.hurtServer` for a wolf: a hit stands a sitting wolf up (`setOrderedToSit(false)`).
#[test]
fn a_hit_stands_a_sitting_wolf_or_parrot_up() {
    if !have_data() || wrong_species(&["minecraft:wolf"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    let e = spawn(&mut h, "minecraft:wolf", 0.0, 7.0);
    h.world.entity_mut(e).insert(Occupied(OCC_SITTING));
    h.world.resource_mut::<Events>().hurt.push(e);
    h.tick();
    let occ = h.world.get::<Occupied>(e).map_or(0, |o| o.0);
    assert_eq!(occ & OCC_SITTING, 0, "the hit releases the sitting order of a wolf");
}

/// `Parrot.hurtServer`: a hit stands a sitting parrot up (`setOrderedToSit(false)`).
#[test]
fn a_hit_stands_a_sitting_parrot_up() {
    if !have_data() || wrong_species(&["minecraft:parrot"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    let e = spawn(&mut h, "minecraft:parrot", 0.0, 7.0);
    h.world.entity_mut(e).insert(Occupied(OCC_SITTING));
    h.world.resource_mut::<Events>().hurt.push(e);
    h.tick();
    let occ = h.world.get::<Occupied>(e).map_or(0, |o| o.0);
    assert_eq!(occ & OCC_SITTING, 0, "the hit releases the sitting order of a parrot");
}

/// `Cat` has no `hurtServer` of its own, and `TamableAnimal` has none: a hit leaves a sitting cat sitting.
#[test]
fn a_hit_leaves_a_sitting_cat_sitting() {
    if !have_data() || wrong_species(&["minecraft:cat"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    let e = spawn(&mut h, "minecraft:cat", 0.0, 7.0);
    h.world.entity_mut(e).insert(Occupied(OCC_SITTING));
    h.world.resource_mut::<Events>().hurt.push(e);
    h.tick();
    let occ = h.world.get::<Occupied>(e).map_or(0, |o| o.0);
    assert_ne!(occ & OCC_SITTING, 0, "a hit does not stand a sitting cat up");
}

/// `Wolf.canMate` refuses a partner that sits (`isInSittingPose` on the other), not a seeker that sits.
#[test]
fn a_sitting_tame_mob_still_looks_for_a_mate() {
    if !have_data() || wrong_species(&["minecraft:wolf"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    let a = spawn(&mut h, "minecraft:wolf", 0.0, 7.0);
    let b = spawn(&mut h, "minecraft:wolf", 1.0, 7.0);
    for x in [a, b] {
        h.world.entity_mut(x).insert(Love(u64::MAX));
    }
    h.world.entity_mut(a).insert(SitPose);
    assert!(can_mate(&h.world, a, b), "a sitting wolf still looks for a standing partner");
    h.world.entity_mut(b).insert(SitPose);
    assert!(!can_mate(&h.world, a, b), "a wolf refuses a partner that sits");
}

/// `Wolf` begging: a spectator holding a bone does not make a tame wolf beg, and a player holding it does.
#[test]
fn a_spectator_holding_a_bone_does_not_make_a_wolf_beg() {
    if !have_data() || wrong_species(&["minecraft:wolf"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    let e = spawn(&mut h, "minecraft:wolf", 0.0, 7.0);
    tame(&mut h.world, e, Who::Player(1), 20.0);
    player_with(&mut h, 3.5, item_id("minecraft:bone"), true);
    h.tick();
    assert!(h.world.get::<Begging>(e).is_none(), "a spectator never makes a wolf beg");
}

/// `Wolf` sitting: a sitting wolf keeps its begging (`BegGoal` keeps the tilt while it sits).
#[test]
fn sitting_down_keeps_a_wolfs_begging_tilt() {
    if !have_data() || wrong_species(&["minecraft:wolf"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    let e = spawn(&mut h, "minecraft:wolf", 0.0, 7.0);
    tame(&mut h.world, e, Who::Player(1), 20.0);
    h.world.entity_mut(e).insert(Begging { player: 1, ticks: 40 });
    order_sit(&mut h.world, e, true);
    assert!(h.world.get::<Begging>(e).is_some(), "sitting down keeps the beg tilt");
}

/// `CatTemptGoal.canUse` tests `isTame`, and `TemptGoal.canContinueToUse` ends with `canUse`: a tamed cat stops being tempted.
#[test]
fn a_tamed_cat_stops_being_tempted() {
    if !have_data() || wrong_species(&["minecraft:cat"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    let e = spawn(&mut h, "minecraft:cat", 0.5, 1.0);
    player_with(&mut h, 3.5, item_id("minecraft:cod"), false);
    h.tick();
    assert_eq!(h.world.get::<Tempts>(e).map_or(0, |t| t.0[0].player), 1, "a wild cat is tempted by cod");
    tame(&mut h.world, e, Who::Player(1), 10.0);
    h.tick();
    assert_eq!(h.world.get::<Tempts>(e).map_or(0, |t| t.0.len()), 0, "a tamed cat stops being tempted");
}
