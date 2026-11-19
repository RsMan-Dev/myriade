//! The block spawner of the ECS (`spawn_spawner`) against the spawner of the client `Mob` (`SpawnerExt::server_tick`), same scene: one
//! cow spawner, one player near, potentials of one species. Checks on both sides: the first attempt after the drawn delay, the mobs of
//! a wave (`count`), the gap between waves (`1 + d`), the stop at `max_nearby`. Needs HORIZON_TEST_RESOURCES.
//!
//! The client's `SpawnerLevel` cannot be implemented for `RefLevel` from here (orphan rule), so `Lvl` wraps it and forwards.
#![allow(dead_code)]
use bevy_ecs::prelude::*;
use horizon_client_lib::biome::SpawnEntry;
use horizon_client_lib::blocks::BlockFacts;
use horizon_client_lib::mobs::data::MobData;
use horizon_client_lib::mobs::spawn_rules::SpawnLevel;
use horizon_client_lib::mobs::spawn_selection::{self as sel, Heightmap, NaturalLevel, SpawnCost, StructureAt, StructureSpawns};
use horizon_client_lib::mobs::spawner::{Spawner, SpawnerAdded, SpawnerEvent, SpawnerExt, SpawnerLevel, SpawnerSpawn};
use horizon_core::geometry::{Aabb, Vec3};
use horizon_core::nbt::parse_snbt;
use horizon_core::random::LegacyRandom;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::goals::*;
use horizon_ecs_proto::spawn_reference::*;
use horizon_ecs_proto::spawn_spawner::{ensure, near_changed, place, run_wakes, Config, Valid};
use horizon_world::dimension_type::DimensionFacts;
use horizon_world::registry::BlockStateId;
use std::cell::RefCell;
use std::collections::HashMap;

const COW: &str = "minecraft:cow";
const POS: [i32; 3] = [0, 1, 0];
const T0: u64 = 1000;
const TICKS: u64 = 3000;
/// The spawner's data as the client loads it: `Delay:-1` makes the first attempt draw its delay, as `place` does on the ECS.
const SNBT: &str = r#"{Delay:-1s,SpawnData:{entity:{id:"minecraft:cow"}},MinSpawnDelay:200s,MaxSpawnDelay:800s,SpawnCount:4s,MaxNearbyEntities:6s,SpawnRange:4s,RequiredPlayerRange:16s}"#;

fn have() -> bool {
    horizon_ecs_proto::species::inscribe_every_module();
    std::env::var_os("HORIZON_TEST_RESOURCES").is_some()
}

/// The reference level, with the spawner's queries on top: its players and the cows the client spawner has added.
struct Lvl<'a> {
    inner: RefLevel<'a>,
    spawned: RefCell<Vec<(String, Aabb)>>,
}

impl SpawnLevel for Lvl<'_> {
    fn state(&self, x: i32, y: i32, z: i32) -> BlockStateId {
        self.inner.state(x, y, z)
    }
    fn facts(&self) -> &BlockFacts {
        self.inner.facts()
    }
    fn mob_data(&self) -> &MobData {
        self.inner.mob_data()
    }
    fn block_tag(&self, tag: &str, block: &str) -> bool {
        self.inner.block_tag(tag, block)
    }
    fn biome_tag(&self, tag: &str, x: i32, y: i32, z: i32) -> bool {
        self.inner.biome_tag(tag, x, y, z)
    }
    fn sky_light(&self, x: i32, y: i32, z: i32) -> i32 {
        self.inner.sky_light(x, y, z)
    }
    fn block_light(&self, x: i32, y: i32, z: i32) -> i32 {
        self.inner.block_light(x, y, z)
    }
    fn sky_darken(&self) -> i32 {
        self.inner.sky_darken()
    }
    fn thundering(&self) -> bool {
        self.inner.thundering()
    }
    fn sea_level(&self) -> i32 {
        self.inner.sea_level()
    }
    fn world_surface(&self, x: i32, z: i32) -> i32 {
        self.inner.world_surface(x, z)
    }
    fn peaceful(&self) -> bool {
        self.inner.peaceful()
    }
    fn dimension_type(&self) -> &DimensionFacts {
        self.inner.dimension_type()
    }
    fn surface_slime_chance(&self, x: i32, y: i32, z: i32) -> f32 {
        self.inner.surface_slime_chance(x, y, z)
    }
    fn world_seed(&self) -> i64 {
        self.inner.world_seed()
    }
    fn player_near(&self, x: f64, y: f64, z: f64, distance: f64) -> bool {
        self.inner.player_near(x, y, z, distance)
    }
}

impl NaturalLevel for Lvl<'_> {
    fn min_y(&self) -> i32 {
        self.inner.min_y()
    }
    fn max_y(&self) -> i32 {
        self.inner.max_y()
    }
    fn ambient_light(&self) -> f32 {
        self.inner.ambient_light()
    }
    fn has_ceiling(&self) -> bool {
        self.inner.has_ceiling()
    }
    fn mob_type(&self, entity: &str) -> Option<sel::MobType> {
        self.inner.mob_type(entity)
    }
    fn biome_spawns(&self, x: i32, y: i32, z: i32) -> Option<&HashMap<String, Vec<SpawnEntry>>> {
        self.inner.biome_spawns(x, y, z)
    }
    fn spawn_cost(&self, entity: &str, x: i32, y: i32, z: i32) -> Option<SpawnCost> {
        self.inner.spawn_cost(entity, x, y, z)
    }
    fn creature_probability(&self, x: i32, y: i32, z: i32) -> f32 {
        self.inner.creature_probability(x, y, z)
    }
    fn structures_at(&self, x: i32, y: i32, z: i32, wanted: &dyn Fn(&str) -> bool) -> Vec<StructureAt> {
        self.inner.structures_at(x, y, z, wanted)
    }
    fn structure_spawns(&self) -> &StructureSpawns {
        self.inner.structure_spawns()
    }
    fn nearest_player_positions(&self) -> &[[f64; 3]] {
        self.inner.nearest_player_positions()
    }
    fn respawn(&self) -> Option<[i32; 3]> {
        self.inner.respawn()
    }
    fn can_spawn_in_chunk(&self, cx: i32, cz: i32) -> bool {
        self.inner.can_spawn_in_chunk(cx, cz)
    }
    fn no_collision(&self, bounds: Aabb) -> bool {
        self.inner.no_collision(bounds)
    }
    fn unobstructed(&self, bounds: Aabb) -> bool {
        self.inner.unobstructed(bounds)
    }
    fn spawn_mobs(&self) -> bool {
        self.inner.spawn_mobs()
    }
    fn heightmap(&self, kind: Heightmap, x: i32, z: i32) -> i32 {
        self.inner.heightmap(kind, x, z)
    }
    fn pathfindable_land(&self, x: i32, y: i32, z: i32) -> bool {
        self.inner.pathfindable_land(x, y, z)
    }
}

impl SpawnerLevel for Lvl<'_> {
    fn alive_player_near(&self, x: f64, y: f64, z: f64, range: f64) -> bool {
        self.inner.players.iter().any(|p| range < 0.0 || (p[0] - x).powi(2) + (p[1] - y).powi(2) + (p[2] - z).powi(2) < range * range)
    }
    fn spawner_blocks_work(&self) -> bool {
        true
    }
    fn entity_exists(&self, id: &str) -> bool {
        self.inner.mob_type(id).is_some()
    }
    fn count_entities(&self, id: &str, b: Aabb) -> usize {
        let (lo, hi) = (b.min_position(), b.max_position());
        self.spawned
            .borrow()
            .iter()
            .filter(|(n, e)| {
                let (emin, emax) = (e.min_position(), e.max_position());
                n.as_str() == id && emin.x < hi.x && emax.x > lo.x && emin.y < hi.y && emax.y > lo.y && emin.z < hi.z && emax.z > lo.z
            })
            .count()
    }
}

/// The ECS's valid callback: always true, so the only refusal left is the nearby limit (the client side keeps its own rules).
fn valid(_: &World, _: &horizon_ecs_proto::cow::Species, _: [f64; 3]) -> bool {
    true
}

fn cow_id() -> u8 {
    species_table().0.iter().find(|s| s.name == COW).expect("minecraft:cow in the species table").id
}

fn cows(h: &mut Herd) -> usize {
    let id = cow_id();
    h.world.query::<&SpeciesId>().iter(&h.world).filter(|s| s.0 == id).count()
}

/// The ECS spawner: (tick, mobs added by that tick) for each wave, over `TICKS` ticks from `T0`.
fn ecs_waves(h: &mut Herd) -> Vec<(u64, usize)> {
    h.world.resource_mut::<Clock>().0 = T0;
    ensure(&mut h.world, 7);
    place(&mut h.world, POS, Config::default(), vec![(cow_id(), 1)]);
    near_changed(&mut h.world, POS, true);
    let mut waves = Vec::new();
    let mut last = cows(h);
    for t in 0..TICKS {
        h.world.resource_mut::<Clock>().0 = T0 + t;
        run_wakes(&mut h.world, valid as Valid);
        let c = cows(h);
        if c > last {
            waves.push((t, c - last));
            // The limit counts the box, not the world: the ECS cows walk off between two waves, so the total may pass it.
            let id = cow_id();
            let boxed = h
                .world
                .query::<(&SpeciesId, &Pos)>()
                .iter(&h.world)
                .filter(|(s, p)| s.0 == id && (-4.0..=5.0).contains(&p.0.x) && (-4.0..=5.0).contains(&p.0.z) && (-3.0..=6.0).contains(&p.0.y))
                .count();
            assert!(boxed <= 6, "ecs: {boxed} cows in the box after the wave at {t}, the limit is 6");
            last = c;
        }
    }
    waves
}

/// The client spawner on the same scene: (call index, mobs added by that call) for each wave.
fn client_waves(l: &Lvl) -> Vec<(u64, usize)> {
    let tag = parse_snbt(SNBT).expect("snbt").0;
    let mut sp = <Spawner as SpawnerExt>::load(&tag);
    let mut rng = LegacyRandom::new(7);
    let mut events: Vec<SpawnerEvent> = Vec::new();
    let mut spawn = |l: &Lvl, _: &mut LegacyRandom, s: &SpawnerSpawn| -> Option<SpawnerAdded> {
        let p = s.position;
        let b = Aabb::new(Vec3::new(p[0] - 0.3, p[1], p[2] - 0.3), Vec3::new(p[0] + 0.3, p[1] + 1.8, p[2] + 0.3));
        l.spawned.borrow_mut().push((s.id.to_string(), b));
        Some(SpawnerAdded { id: 0, bounds: b, mob: true })
    };
    let mut waves = Vec::new();
    let mut last = 0usize;
    for t in 0..TICKS {
        sp.server_tick(l, &mut rng, POS, &mut spawn, &mut events);
        let c = l.spawned.borrow().len();
        if c > last {
            waves.push((t, c - last));
            last = c;
        }
    }
    waves
}

#[test]
fn spawner_block_vs_client_mob() {
    if !have() {
        return;
    }
    let mut h = Herd::with_spawn_world(176, true);
    h.set_daylight(false);
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5, y: 1.0, z: 0.5, main: 0, off: 0, ..Default::default() });
    let ecs = ecs_waves(&mut h);

    let mw = h.world.resource::<Level>().mw.0;
    let mut inner = RefLevel::new(mw, "minecraft:plains");
    inner.players.push([0.5, 1.0, 0.5]);
    let lvl = Lvl { inner, spawned: RefCell::new(Vec::new()) };
    let client = client_waves(&lvl);

    eprintln!("ecs waves (tick, mobs): {ecs:?}");
    eprintln!("client waves (tick, mobs): {client:?}");

    // First attempt: nothing before the minimum delay, and within the maximum delay on the ECS (every attempt is valid there).
    let (ecs_first, client_first) = (ecs[0].0, client[0].0);
    assert!((200..800).contains(&ecs_first), "ecs first wave at {ecs_first}");
    assert!((200..800).contains(&client_first), "client first wave at {client_first}");

    // A wave: `count` mobs on the ECS; the client may refuse some with its own rules, never more than `count`.
    assert_eq!(ecs[0].1, 4, "ecs wave size");
    assert!(client[0].1 <= 4, "client wave size {}", client[0].1);

    // Gap between waves: `1 + d` with d in [200, 800), on both sides.
    for w in [&ecs, &client] {
        for pair in w.windows(2) {
            let gap = pair[1].0 - pair[0].0;
            assert!((201..=800).contains(&gap), "gap {gap} between {pair:?}");
        }
    }

    // Nearby limit: 6 of the species in the box, then no more spawns: checked in the box after each ECS wave (`ecs_waves`); the
    // client's mobs do not move, so its total is bounded.
    let ecs_total: usize = ecs.iter().map(|w| w.1).sum();
    let client_total: usize = client.iter().map(|w| w.1).sum();
    assert_eq!(ecs_total, 6, "ecs stops at max_nearby");
    assert!(client_total <= 6, "client exceeds max_nearby: {client_total}");
}
