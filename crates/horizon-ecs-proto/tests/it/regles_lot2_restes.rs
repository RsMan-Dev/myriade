//! Restes du lot 2 : les tests du Mob qui n'avaient pas leur pendant complet sur l'ECS. Le boost de vie d'un loup qu'on apprivoise
//! (`finalize_load_tests.rs`), l'araignée et les équins (`living_climbable_tests.rs`), la chèvre qui choisit une autre fuite
//! (`panic_tests.rs`), les paquets de lapins et d'axolotls (`spawn_group_variant_tests.rs`). Les valeurs viennent du jar 26.3.
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_core::random::{LegacyRandom, RandomSource};
use horizon_ecs_proto::attr_mod::touched;
use horizon_ecs_proto::behaviours::animal_panic::{self, AnimalPanicking};
use horizon_ecs_proto::behaviours::spawn_draw::{run, When};
use horizon_ecs_proto::behaviours::variant::Variant;
use horizon_ecs_proto::behaviours::Entry;
use horizon_ecs_proto::brain::LastDamage;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::effects::{self, Kind};
use horizon_ecs_proto::goals::*;
use horizon_ecs_proto::rules::climbable::on_climbable_entity;
use horizon_ecs_proto::rules::spawn_draw::Ctx;
use horizon_ecs_proto::species::{axolotl, rabbit, Config};
use horizon_ecs_proto::{tame, targeting::Who};

const EQUINES: [&str; 9] = [
    "minecraft:horse",
    "minecraft:donkey",
    "minecraft:mule",
    "minecraft:skeleton_horse",
    "minecraft:zombie_horse",
    "minecraft:camel",
    "minecraft:camel_husk",
    "minecraft:llama",
    "minecraft:trader_llama",
];

const FLIERS: [&str; 3] = ["minecraft:ghast", "minecraft:happy_ghast", "minecraft:phantom"];

fn plain(_: &str) -> bool {
    false
}

/// A stone floor at y = 0 and a stone wall at z = 2 from y = 1 to 14; the column at z = 1 holds `block` (with its properties) when one is given.
fn wall(h: &mut Herd, block: Option<(&'static str, &'static [(&'static str, &'static str)])>) {
    for x in 0..=4 {
        for z in 0..=4 {
            h.set_block(x, 0, z, "minecraft:stone");
        }
        for y in 1..=14 {
            h.set_block(x, y, 2, "minecraft:stone");
            if let Some((name, props)) = block {
                h.set_block_state(x, y, 1, name, props);
            }
        }
    }
}

/// A body of the species `name` at (x, y, z), its activity kept on.
fn spawn(h: &mut Herd, name: &str, x: f64, y: f64, z: f64) -> Entity {
    let sp = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == name).unwrap();
    let e = spawn_species(&mut h.world, &sp, x, z, 1);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.get_mut::<Pos>(e).unwrap().0.y = y;
    e
}

/// `Spider.onClimbable` is its own flag, whatever the block; an equine and a flying monster are never on a climbable, a cow in the same vines is.
#[test]
fn an_equine_and_a_flying_monster_are_never_on_a_climbable() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, true);
    wall(&mut h, Some(("minecraft:vine", &[("south", "true")])));
    let cow = spawn(&mut h, "minecraft:cow", 1.5, 2.0, 1.5);
    assert!(on_climbable_entity(&h.world, cow), "a cow in vines is on a climbable");
    for name in EQUINES.iter().chain(FLIERS.iter()).copied() {
        let e = spawn(&mut h, name, 1.5, 2.0, 1.5);
        assert!(!on_climbable_entity(&h.world, e), "{name} is on a climbable");
    }
    let spider = spawn(&mut h, "minecraft:spider", 1.5, 2.0, 1.5);
    assert!(!on_climbable_entity(&h.world, spider), "a spider at rest is not climbing");
    h.world.get_mut::<Drive>(spider).unwrap().climbing = true;
    assert!(on_climbable_entity(&h.world, spider), "a climbing spider is climbable");
}

/// `Wolf.tame` under Health Boost II: +8 on top of the tamed 40 (48), and when the effect ends the maximum is 40 again.
#[test]
fn a_wolf_tamed_under_health_boost_loses_the_bonus_when_it_ends() {
    let mut a = Herd::new(0, 1.0, true, false, 1);
    let sp = species_table().0.into_iter().find(|s| s.name == "minecraft:wolf").expect("a known species");
    let e = spawn_species(&mut a.world, &sp, 0.5, 0.5, 1);
    effects::add(&mut a.world, e, Kind::HealthBoost, 2, 600);
    tame::tame(&mut a.world, e, Who::Player(1), 40.0);
    assert_eq!(max_health(&a.world, e), 48.0);
    effects::remove(&mut a.world, e, Kind::HealthBoost);
    assert_eq!(max_health(&a.world, e), 40.0);
    assert_eq!(a.world.get::<Health>(e).map(|h| h.0), Some(40.0));
    touched(&mut a.world, e, "max_health");
    assert_eq!(max_health(&a.world, e), 40.0);
}

/// A goat with the panic goal of `animal_panic` at its default parameters.
fn goat_world() -> Option<(Herd, Entity)> {
    if !have_data() {
        return None;
    }
    let mut h = Herd::with_slab_awake(60, false, false);
    set_lazy_look(&mut h.world, false);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let mut sp = *table.iter().find(|s| s.name == "minecraft:goat").unwrap();
    let entries: &'static [Entry] = Box::leak(Box::new([Entry::new(&animal_panic::MODULE, 1, &animal_panic::Params::DEFAULT)]));
    sp.goals = Config { entries };
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 11);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    arm_timers(&mut h.world, e);
    h.tick();
    Some((h, e))
}

/// A hit as `apply_damage` leaves it: its `LastDamage` and the `hurt` event.
fn hurt(h: &mut Herd, e: Entity) {
    let now = now_of(&h.world, e);
    h.world.entity_mut(e).insert(LastDamage { at: now, panic: true, by: None });
    h.world.resource_mut::<Events>().hurt.push(e);
}

/// A panicking goat that reaches its spot before its panic ends picks another one (`path_run` asks `next_spot` on each tick
/// its path is over), so its walk target changes while it still runs.
#[test]
fn a_panicking_goat_flees() {
    let Some((mut h, e)) = goat_world() else {
        return;
    };
    hurt(&mut h, e);
    h.tick();
    assert!(h.world.get::<AnimalPanicking>(e).is_some(), "the goat runs");
    let mut targets = std::collections::BTreeSet::new();
    for _ in 0..20 {
        h.world.get_mut::<Nav>(e).unwrap().path = None;
        h.tick();
        let s = h.world.get::<Stroll>(e).unwrap();
        targets.insert((s.tx.to_bits(), s.tz.to_bits()));
    }
    assert!(targets.len() > 1, "a panicking goat that reaches its spot picks another one");
}

/// `Axolotl.finalizeSpawn` (`Util.getRandom` over the four common variants, then `nextInt(2)`): a lone axolotl takes the level's first draws.
#[test]
fn a_lone_axolotl_takes_the_level_randoms_first_draws() {
    let d = axolotl::DEF.draws.unwrap();
    let c = Ctx { biome: "", biome_has: &plain, special: 0.0, baby: false };
    for seed in 0..64i64 {
        let mut r = LegacyRandom::new(seed);
        let pair = [r.next_int_bound(4).unwrap(), r.next_int_bound(4).unwrap()];
        let picked = pair[r.next_int_bound(2).unwrap() as usize];
        let mut w = World::new();
        let e = w.spawn_empty().id();
        run(&mut w, e, d, When::Before, &mut None, &c, &mut LegacyRandom::new(seed));
        assert_eq!(w.get::<Variant>(e).map(|v| v.0), Some(picked as u8), "seed {seed}");
    }
}

/// `RabbitGroupData` and `AxolotlGroupData`: a pack of rabbits shares the first one's variant; a pack of axolotls picks from the first
/// one's two variants, and a member after two others is a baby (`GrowAt`).
#[test]
fn a_pack_shares_its_first_members_group_data() {
    let c = Ctx { biome: "", biome_has: &plain, special: 0.0, baby: false };
    let d = rabbit::DEF.draws.unwrap();
    for seed in 0..32i64 {
        let mut w = World::new();
        let mut r = LegacyRandom::new(seed);
        let mut group = None;
        let mut variants = Vec::new();
        for _ in 0..4 {
            let e = w.spawn_empty().id();
            for when in [When::Before, When::After] {
                run(&mut w, e, d, when, &mut group, &c, &mut r);
            }
            variants.push(w.get::<Variant>(e).map(|v| v.0));
        }
        assert!(variants.iter().all(|v| *v == variants[0]), "rabbit pack {seed}: {variants:?}");
    }

    let d = axolotl::DEF.draws.unwrap();
    for seed in 0..32i64 {
        let mut w = World::new();
        let mut draws = LegacyRandom::new(seed);
        let mut r = LegacyRandom::new(seed);
        let pair = [r.next_int_bound(4).unwrap(), r.next_int_bound(4).unwrap()];
        let mut group = None;
        for i in 0..4 {
            let e = w.spawn_empty().id();
            for when in [When::Before, When::After] {
                run(&mut w, e, d, when, &mut group, &c, &mut draws);
            }
            let want = pair[r.next_int_bound(2).unwrap() as usize];
            assert_eq!(w.get::<Variant>(e).map(|v| v.0), Some(want as u8), "axolotl {i} of pack {seed}");
            assert_eq!(w.get::<GrowAt>(e).is_some(), i >= 2, "axolotl {i} of pack {seed}");
        }
    }
}

/// `Entity.baseTick` on fire: 1 damage on the first tick and every 20 ticks after it while the fire lasts; rain puts it out on the
/// next tick, as water does, and a dry fire lasts its 40 ticks (the pig keeps its health when the fire is out).
#[test]
fn wet_fire_damage_and_extinguishing_follow_the_full_mob_tick() {
    if !have_data() {
        return;
    }
    let mut dry = Herd::with_slab_awake(8, true, false);
    let pig = spawn(&mut dry, "minecraft:pig", 1.5, 1.0, 1.5);
    set_on_fire(&mut dry.world, pig, 40);
    for _ in 0..50 {
        dry.tick();
    }
    assert!(dry.world.get::<Burning>(pig).is_none(), "the fire is out after its 40 ticks");
    assert_eq!(health(&dry, pig), 8.0, "two burns of 1 at the first tick and at the 20th");

    let mut wet = Herd::with_slab_awake(8, true, false);
    wet.set_raining(true);
    let pig = spawn(&mut wet, "minecraft:pig", 1.5, 1.0, 1.5);
    set_on_fire(&mut wet.world, pig, 40);
    wet.tick();
    assert!(wet.world.get::<Burning>(pig).is_none(), "rain puts the fire out on the next tick");
    assert_eq!(health(&wet, pig), 9.0, "only the first burn comes before it");
}
