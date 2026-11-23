//! The natural spawning reads the biome of each position: on a world whose chunks at x < 0 are plains and the others desert
//! (`Kind::TwoBiomes`), the biome of a block and the creature list of its biome follow the chunk. The expected names are the game's:
//! desert's creatures (`BiomeDefaultFeatures.desertSpawns`, by javap) hold the rabbit and the camel; plains' hold no camel.
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::spawn_biome::biome_at;
use horizon_ecs_proto::spawning::{Category, Entry, Kind, Spawner};

fn have() -> bool {
    horizon_ecs_proto::species::inscribe_every_module();
    std::env::var_os("HORIZON_TEST_RESOURCES").is_some()
}

#[test]
fn the_biome_of_a_position_is_read_where_it_is() {
    if !have() {
        return;
    }
    let mut h = Herd::with_spawn_world_kind_awake(48, true, Kind::TwoBiomes);
    h.enable_spawning("minecraft:plains", 7);
    let mw = h.world.resource::<Level>().mw.0;
    let spawner = h.world.resource::<Spawner>();
    let zoom = spawner.zoom;
    for x in [-40, -24] {
        assert_eq!(mw.biomes.get(biome_at(mw, zoom, x, 1, 0)).name, "minecraft:plains");
    }
    for x in [24, 40] {
        assert_eq!(mw.biomes.get(biome_at(mw, zoom, x, 1, 0)).name, "minecraft:desert");
    }
    let table = species_table();
    let creatures = |biome: &str| -> Vec<String> {
        let list: &[Entry] = &spawner.tables[mw.biomes.index[biome] as usize].lists[Category::Creature as usize];
        list.iter().map(|e| table.0[e.species as usize].name.to_string()).collect()
    };
    let desert = creatures("minecraft:desert");
    assert!(desert.iter().any(|n| n == "minecraft:rabbit"));
    assert!(desert.iter().any(|n| n == "minecraft:camel"));
    let plains = creatures("minecraft:plains");
    assert!(!plains.iter().any(|n| n == "minecraft:camel"));
}
