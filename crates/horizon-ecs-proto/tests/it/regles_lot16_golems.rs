//! Lot 16d of the rules of the old golem goals (`mobs/golem.rs` tests): the poppy box of the flower offer, a creeper that hurts an iron
//! golem is never fought back (`IronGolem.canAttack`), a golem does not hunt a mount of the monster category (`Enemy`), the absorption
//! hearts take a player's blow and are credited to it (`dealt_absorbed`, in tenths), the crack sound plays once at each crack level
//! (`IronGolem.hurtServer`), and a snow golem's trail posts a `block_place` (`SnowGolem.aiStep`). Expected values are the ones of the
//! origin tests.
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_ecs_proto::absorption::Absorption;
use horizon_ecs_proto::behaviours::gossip::{add as gossip_add, Kind};
use horizon_ecs_proto::behaviours::neutral_anger::anger_at;
use horizon_ecs_proto::behaviours::stroll;
use horizon_ecs_proto::cow::{spawn_species, DamageQueue, Herd, SpeciesTable};
use horizon_ecs_proto::goals::{Events, Online, Player};
use horizon_ecs_proto::host_out::HostEvent;
use horizon_ecs_proto::hostile::{HurtBy, PlayerState};
use horizon_ecs_proto::rules::offer_flower::{beyond_offer_height, in_offer_reach};
use horizon_ecs_proto::targeting::{set_target, Target, Who};

/// A species of the table, spawned at (x, z) with a fixed seed.
fn spawn(h: &mut Herd, name: &str, x: f64, z: f64, seed: u64) -> Entity {
    let sp = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == name).unwrap();
    spawn_species(&mut h.world, &sp, x, z, seed)
}

/// The host events of the last tick, drained.
fn heard(h: &mut Herd) -> Vec<HostEvent> {
    h.world.resource_mut::<Events>().host.drain().collect()
}

/// `OfferFlowerGoal`: the search box is the golem's box inflated by (6, 2, 6), and the candidate must be within six blocks (Euclidean).
/// The golem stands at x 0.5; the cases are the origin's: 4.0 blocks on x is in reach, 6.5 on one axis is not, 8.5 diagonal is not,
/// and 5 blocks up is beyond the box's rise of two.
#[test]
fn the_poppy_goes_to_a_villager_in_the_box_within_six_blocks() {
    let d2 = |dx: f64, dz: f64| dx * dx + dz * dz;
    assert!(in_offer_reach(d2(4.0, 0.0)), "4 blocks on x is in reach");
    assert!(!in_offer_reach(d2(6.5, 0.0)), "6.5 blocks on one axis is out of reach");
    assert!(!in_offer_reach(d2(5.5, 5.5)), "about 7.8 blocks diagonal is out of reach");
    assert!(beyond_offer_height(5.0), "5 blocks up is beyond the box");
    assert!(!beyond_offer_height(1.0), "1 block up is inside the box");
}

/// `IronGolem.canAttack` refuses the creeper: a creeper's blow is never answered, a zombie's blow is.
#[test]
fn a_creeper_that_hurts_an_iron_golem_is_never_fought_back() {
    if !have_data() || wrong_species(&["minecraft:iron_golem", "minecraft:creeper", "minecraft:zombie"]) {
        return;
    }
    for (kind, answered) in [("minecraft:creeper", false), ("minecraft:zombie", true)] {
        let mut h = Herd::with_slab_awake(20, false, false);
        let g = spawn(&mut h, "minecraft:iron_golem", 0.5, 0.5, 5);
        let attacker = spawn(&mut h, kind, 3.5, 0.5, 6);
        h.world.resource_mut::<HurtBy>().0.push((g, Who::Entity(attacker)));
        h.tick();
        h.tick();
        assert_eq!(h.world.get::<Target>(g).is_some(), answered, "a golem hurt by a {kind}");
    }
}

/// `golem_prey` is `instanceof Enemy` and not a creeper: a zombie horse and a camel husk share the monster category but are not prey.
#[test]
fn mounts_in_the_monster_category_are_not_golem_prey() {
    if !have_data() || wrong_species(&["minecraft:iron_golem", "minecraft:zombie", "minecraft:zombie_horse", "minecraft:camel_husk"]) {
        return;
    }
    for (kind, prey) in [("minecraft:zombie", true), ("minecraft:zombie_horse", false), ("minecraft:camel_husk", false)] {
        let mut h = Herd::with_slab_awake(20, false, false);
        let g = spawn(&mut h, "minecraft:iron_golem", 0.5, 0.5, 5);
        spawn(&mut h, kind, 3.5, 0.5, 6);
        for _ in 0..10 {
            h.tick();
        }
        assert_eq!(h.world.get::<Target>(g).is_some(), prey, "a golem next to a {kind}");
    }
}

/// `LivingEntity.actuallyHurt`: absorbed damage from a player is credited in tenths; a hit with nothing absorbed credits nothing.
#[test]
fn player_hit_credits_dealt_absorbed() {
    if !have_data() || wrong_species(&["minecraft:iron_golem"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    let g = spawn(&mut h, "minecraft:iron_golem", 0.5, 0.5, 5);
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 3.5, y: 1.0, z: 0.5, main: 0, off: 0, ..Default::default() });
    h.world.entity_mut(g).insert(Absorption(4.0));
    let absorbed = |h: &mut Herd| -> Vec<f32> {
        heard(h)
            .into_iter()
            .filter_map(|ev| match ev {
                HostEvent::HitDealt { absorbed, .. } => Some(absorbed),
                _ => None,
            })
            .collect()
    };
    // The hit of 1.5 is taken whole by the 4 hearts of absorption.
    h.world.resource_mut::<DamageQueue>().add_by(g, 1.5, (3.5, 0.5), 1);
    h.tick();
    assert_eq!(absorbed(&mut h), vec![1.5], "1.5 absorbed, credited as 15 tenths");
    // Out of the hurt frames, the 2.5 hearts left take 2.5 of a hit of 10.
    for _ in 0..11 {
        h.tick();
    }
    heard(&mut h);
    h.world.resource_mut::<DamageQueue>().add_by(g, 10.0, (3.5, 0.5), 1);
    h.tick();
    assert_eq!(absorbed(&mut h), vec![2.5], "the remaining 2.5 absorbed");
    // No absorption left: nothing is credited.
    for _ in 0..11 {
        h.tick();
    }
    heard(&mut h);
    h.world.resource_mut::<DamageQueue>().add_by(g, 2.0, (3.5, 0.5), 1);
    h.tick();
    assert_eq!(absorbed(&mut h), Vec::<f32>::new(), "no absorption, no credit");
}

/// `IronGolem.hurtServer`: a hit that leaves the golem in a new crack level plays `entity.iron_golem.damage` once; a hit inside one
/// level plays nothing. The levels are 0.75, 0.5 and 0.25 of its 100 health.
#[test]
fn a_crack_is_heard_once_at_the_threshold() {
    if !have_data() || wrong_species(&["minecraft:iron_golem"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    let g = spawn(&mut h, "minecraft:iron_golem", 0.5, 0.5, 5);
    let crack = |h: &mut Herd, amount: f32| -> usize {
        for _ in 0..11 {
            h.tick();
        }
        heard(h);
        h.world.resource_mut::<DamageQueue>().add_by(g, amount, (3.5, 0.5), 1);
        h.tick();
        heard(h)
            .iter()
            .filter(|ev| matches!(ev, HostEvent::Sound { name, .. } if &**name == "entity.iron_golem.damage"))
            .count()
    };
    assert_eq!(crack(&mut h, 30.0), 1, "100 to 70 crosses 0.75");
    assert_eq!(crack(&mut h, 10.0), 0, "70 to 60 stays at the same crack");
    assert_eq!(crack(&mut h, 20.0), 1, "60 to 40 crosses 0.5");
    assert_eq!(crack(&mut h, 10.0), 0, "40 to 30 stays at the same crack");
    assert_eq!(crack(&mut h, 10.0), 1, "30 to 20 crosses 0.25");
}

/// `SnowGolem.aiStep` with mob griefing: its trail posts a `block_place` by the golem, at the centre of the block it places.
#[test]
fn snow_trail_posts_block_place_with_golem_and_snow_state() {
    if !have_data() || wrong_species(&["minecraft:snow_golem"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    let g = spawn(&mut h, "minecraft:snow_golem", 0.5, 0.5, 42);
    h.tick();
    let mw = h.world.resource::<horizon_ecs_proto::cow::Level>().mw.0;
    let placed: Vec<([f64; 3], _)> = heard(&mut h).into_iter().filter_map(|ev| match ev { HostEvent::BlockPlace { entity, at, state } if entity == g => Some((at, state)), _ => None }).collect();
    assert!(!placed.is_empty(), "the trail of the golem posts a block_place");
    for (at, state) in placed {
        assert_eq!((at[0].rem_euclid(1.0), at[2].rem_euclid(1.0)), (0.5, 0.5), "at the centre of the block");
        assert_eq!(Some(state), horizon_ecs_proto::blocks::resolve(mw, "minecraft:snow", &[("layers", "1")]), "the state posted is one layer of snow");
    }
}

/// `IronGolem.angry_at_player`: a village golem angry at a player (`updatePersistentAnger`) goes after that player.
#[test]
fn a_village_golem_still_targets_the_player_who_angered_it() {
    if !have_data() || wrong_species(&["minecraft:iron_golem"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    h.set_online(&[(3.5, 0.5)]);
    let g = spawn(&mut h, "minecraft:iron_golem", 0.5, 0.5, 5);
    set_target(&mut h.world, g, Target::player(1, 20));
    h.tick();
    assert_eq!(anger_at(&h.world, g), Some(Who::Player(1)), "the village golem is angry at the player");
    assert!(h.world.get::<Target>(g).is_some(), "the village golem goes after the player");
}

/// `SnowGolem.registerGoals`: one water-avoiding stroll goal at 1.0000001E-5, not the default 0.001 of the two-argument constructor.
#[test]
fn a_snow_golem_strolls_with_the_rare_probability() {
    if !have_data() {
        return;
    }
    let sp = horizon_ecs_proto::species::registry().by_name("minecraft:snow_golem").expect("the snow golem");
    let p = sp.config.param::<stroll::Params>(&stroll::MODULE, 0).expect("a stroll goal");
    assert_eq!(p.water_avoiding, 1.000_000_1e-5);
    assert!(sp.config.param::<stroll::Params>(&stroll::MODULE, 1).is_none(), "one stroll goal, not two");
}

/// `DefendVillageTargetGoal`: a villager holds player 1 in major disrepute (-100); the golem defends against a visible player, not an invisible one.
#[test]
fn an_invisible_player_does_not_make_a_golem_defend_the_village() {
    if !have_data() || wrong_species(&["minecraft:iron_golem", "minecraft:villager"]) {
        return;
    }
    for invisible in [false, true] {
        let mut h = Herd::with_slab_awake(20, false, false);
        h.set_online(&[(9.5, 0.5)]);
        h.world.resource_mut::<PlayerState>().visibility = vec![1.0, if invisible { 0.0 } else { 1.0 }];
        let v = spawn(&mut h, "minecraft:villager", 2.5, 0.5, 6);
        gossip_add(&mut h.world, v, 1, Kind::MajorNegative, 100);
        let g = spawn(&mut h, "minecraft:iron_golem", 0.5, 0.5, 5);
        for _ in 0..3 {
            h.tick();
        }
        assert_eq!(h.world.get::<Target>(g).is_some(), !invisible, "a golem next to a player, invisible: {invisible}");
    }
}
