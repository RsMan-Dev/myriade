//! A creature born by the level (`born_finalized`) carries what `finalizeSpawn` gives its seed, for its reason, and the host hears one `Spawned`
//! for it. The reference is the same birth finalized by hand (`finalize_in`) on a second world with the same seed.
use crate::common;
use common::*;
use bevy_ecs::prelude::*;
use horizon_core::random::LegacyRandom;
use horizon_ecs_proto::attr_mod::AttrMods;
use horizon_ecs_proto::cow::{spawn_species, species_table, GrowAt, Level, Species};
use horizon_ecs_proto::effects::Difficulty;
use horizon_ecs_proto::reinforcements::on_hurt;
use horizon_ecs_proto::targeting::Who;
use horizon_ecs_proto::goals::{equipped, Slot};
use horizon_ecs_proto::host_out::HostEvent;
use horizon_ecs_proto::left_handed::LeftHanded;
use horizon_ecs_proto::spawn_natural::{born_finalized, finalize_in};
use horizon_ecs_proto::spawn_reason::SpawnReason;
use horizon_ecs_proto::spawning::{born, Spawner};
use horizon_ecs_proto::spawn_env::SpawnEnv;
use horizon_ecs_proto::behaviours::spawn_draw::{Loner, Wide};

const AT: [f64; 3] = [0.5, 1.0, 0.5];

fn skip() -> bool {
    !have_data() || wrong_species(&["minecraft:zombie"])
}

fn species_named(name: &str) -> Species {
    *species_table().0.iter().find(|s| s.name == name).unwrap_or_else(|| panic!("{name}"))
}

/// The baby timer, the left hand, and the four armour slots then the weapon: what the finalization left on the mob.
fn kit(w: &World, e: Entity) -> (bool, bool, Vec<u32>) {
    let slots = [Slot::Head, Slot::Chest, Slot::Legs, Slot::Feet, Slot::Main];
    (w.get::<GrowAt>(e).is_some(), w.get::<LeftHanded>(e).is_some(), slots.iter().map(|s| equipped(w, e, *s)).collect())
}

/// The same birth as `born_finalized`, finalized by hand with the seed's draws, in `h`'s world.
fn by_hand(h: &mut Herd, sp: &Species, seed: u64, reason: SpawnReason) -> (bool, bool, Vec<u32>) {
    let mw = h.world.resource::<Level>().mw.0;
    let special = horizon_client_lib::mobs::spawning::LocalDifficulty::at(mw.world, AT).special();
    let e = born(&mut h.world, sp, AT, seed, f64::MAX);
    finalize_in(&mut h.world, mw, e, reason, special, None, &mut LegacyRandom::new(seed as i64));
    kit(&h.world, e)
}

/// A birth through `born_finalized` for each seed: the same kit as the hand finalization, and one `Spawned` for it, natural or not.
fn check_births(h: &mut Herd, reference: &mut Herd, sp: &Species, reason: SpawnReason) {
    let natural = reason == SpawnReason::Natural;
    for seed in 0..200u64 {
        let mw = h.world.resource::<Level>().mw.0;
        h.drain_host_out();
        let e = born_finalized(&mut h.world, mw, sp, AT, seed, f64::MAX, reason, &mut None).expect("no mod listens: no cancel");
        let announced = h.drain_host_out().iter().filter(|ev| matches!(ev, HostEvent::Spawned { entity, natural: n, reason: r } if *entity == e && *n == natural && *r == reason)).count();
        assert_eq!(announced, 1, "seed {seed}: one Spawned for {reason:?}");
        assert_eq!(kit(&h.world, e), by_hand(reference, sp, seed, reason), "seed {seed}, {reason:?}");
    }
}

/// A natural zombie birth carries the zombie's finalization for its seed, and the host hears one natural `Spawned`.
#[test]
fn a_natural_zombie_birth_is_finalized_as_its_seed_gives_and_announced_once() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let mut reference = Herd::with_slab_awake(60, true, false);
    check_births(&mut h, &mut reference, &species_named("minecraft:zombie"), SpawnReason::Natural);
}

/// A spawner's birth is finalized for the spawner's reason (`Mob.finalizeSpawn` with `EntitySpawnReason.SPAWNER`), announced as not natural.
#[test]
fn a_spawner_birth_is_finalized_for_the_spawner_and_announced_as_not_natural() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let mut reference = Herd::with_slab_awake(60, true, false);
    check_births(&mut h, &mut reference, &species_named("minecraft:zombie"), SpawnReason::Spawner);
}

/// `Zombie.hurtServer` on hard, at night, with the reinforcement chance at 1.0 and a player hitting: one copy is born, and the host hears
/// one `Spawned` of reason Reinforcement for it.
#[test]
fn a_reinforcement_is_finalized_once_and_announced_once() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    h.set_daylight(false);
    h.world.insert_resource(Difficulty(3));
    let z = spawn_species(&mut h.world, &species_named("minecraft:zombie"), 0.5, 0.5, 9);
    h.world.entity_mut(z).insert(AttrMods(vec![("spawn_reinforcements".to_string(), 1.0, Vec::new())]));
    h.drain_host_out();
    on_hurt(&mut h.world, z, Some(Who::Player(1)));
    let reinforcements = h.drain_host_out().iter().filter(|ev| matches!(ev, HostEvent::Spawned { natural: false, reason: SpawnReason::Reinforcement, .. })).count();
    assert_eq!(reinforcements, 1, "one copy, announced once");
}

const TICKS: usize = 800;
const PLAYER: (f64, f64) = (8.0, 8.0);

/// The tropical fish born by the pass in a lake, in birth order, cut into the groups the spawner traced (`Spawner.groups`): each group
/// with its members' `Wide` variant and whether each is a loner.
fn fish_groups(seed: i64) -> Vec<Vec<(u32, bool)>> {
    let mut h = Herd::with_spawn_world_awake(176, true);
    h.enable_spawning("minecraft:plains", seed);
    h.fill_water(4);
    h.set_online(&[PLAYER]);
    h.world.insert_resource(SpawnEnv { spawn_mobs: true, monsters: false, spawnable: (-10..=10).flat_map(|dx| (-10..=10).map(move |dz| (dx, dz))).collect(), border: None, world_spawn: [-200, 1, -200], changed: false });
    h.world.resource_mut::<Spawner>().groups = Some(Vec::new());
    let mut born = Vec::new();
    for _ in 0..TICKS {
        h.tick();
        for ev in h.drain_host_out() {
            if let HostEvent::Spawned { entity, natural: true, .. } = ev {
                // Every natural birth counts for the trace (a creature with no `Wide` has the variant 0).
                born.push((h.world.get::<Wide>(entity).map_or(0, |w| w.0), h.world.get::<Loner>(entity).is_some()));
            }
        }
    }
    let sizes = h.world.resource_mut::<Spawner>().groups.take().unwrap_or_default();
    let mut rest = born.into_iter();
    sizes.iter().map(|&n| rest.by_ref().take(n as usize).collect()).collect()
}

/// `TropicalFish.finalizeSpawn`: a school shares the variant its first member drew, and a loner (`isSchool` false) is the last of its group.
#[test]
fn a_school_shares_one_variant_and_a_loner_closes_its_group() {
    if skip() {
        return;
    }
    let groups = fish_groups(7);
    assert!(groups.iter().map(|g| g.len()).sum::<usize>() > 0, "fish were born");
    assert!(groups.iter().map(|g| g[0].0).collect::<std::collections::BTreeSet<_>>().len() > 1, "two groups of different variants");
    for g in &groups {
        let (key, _) = g[0];
        for (i, &(variant, loner)) in g.iter().enumerate() {
            assert!(!loner || i + 1 == g.len(), "a loner ends its group");
            if !loner {
                assert_eq!(variant, key, "a school member shares its group's variant");
            }
        }
    }
}

/// After a loner, its group is over and the next group starts: the loner-closed groups are followed by another group.
#[test]
fn a_loner_ends_its_group_and_the_next_group_is_born_after_it() {
    if skip() {
        return;
    }
    let groups = fish_groups(11);
    let closed = groups.iter().enumerate().filter(|(_, g)| g.last().is_some_and(|m| m.1)).map(|(i, _)| i).collect::<Vec<_>>();
    assert!(closed.iter().any(|&i| i + 1 < groups.len()), "a loner-closed group is followed by another group");
}
