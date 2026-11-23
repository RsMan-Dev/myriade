//! `NaturalSpawner.spawnMobsForChunkGeneration`: a chunk of plains the generator has made gets creature packs, each creature of plains'
//! creature list (by the biome data), standing on the top of its column (the world's surface, the level's heightmap).
use horizon_client_lib::mobs::spawn_rules::SpawnLevel;
use horizon_core::random::LegacyRandom;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::spawn_generation::spawn_chunk_packs;
use horizon_ecs_proto::spawning::{Category, Entry, Kind, Spawner};

fn have() -> bool {
    horizon_ecs_proto::species::inscribe_every_module();
    std::env::var_os("HORIZON_TEST_RESOURCES").is_some()
}

#[test]
fn a_generated_plains_chunk_gets_its_packs_on_the_top_ground() {
    if !have() {
        return;
    }
    let mut h = Herd::with_spawn_world_kind_awake(48, true, Kind::TwoBiomes);
    h.enable_spawning("minecraft:plains", 7);
    let mw = h.world.resource::<Level>().mw.0;
    let table = species_table();
    let plains: Vec<String> = {
        let spawner = h.world.resource::<Spawner>();
        let list: &[Entry] = &spawner.tables[mw.biomes.index["minecraft:plains"] as usize].lists[Category::Creature as usize];
        list.iter().map(|e| table.0[e.species as usize].name.to_string()).collect()
    };
    // The chunk at x -16..-1, z 0..15: plains (`Kind::TwoBiomes`).
    let mut region = LegacyRandom::new(1234);
    spawn_chunk_packs(&mut h.world, mw, -1, 0, &mut region);
    let mut q = h.world.query::<(&Pos, &SpeciesId)>();
    let placed: Vec<(String, [f64; 3])> = q
        .iter(&h.world)
        .filter(|(p, _)| (p.0.x.floor() as i32) >> 4 == -1 && (p.0.z.floor() as i32) >> 4 == 0)
        .map(|(p, id)| (table.0[id.0 as usize].name.to_string(), [p.0.x, p.0.y, p.0.z]))
        .collect();
    assert!(!placed.is_empty(), "a plains chunk gets creatures");
    for (name, [x, y, z]) in &placed {
        assert!(plains.contains(name), "{name} is in plains' creature list");
        let top = SpawnLevel::world_surface(mw, x.floor() as i32, z.floor() as i32);
        assert_eq!(*y, f64::from(top), "{name} stands on the top ground of its column");
    }
}
