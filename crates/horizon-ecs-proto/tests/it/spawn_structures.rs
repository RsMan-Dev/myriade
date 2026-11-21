//! The spawn lists at a position (`StructureHost::list_at`) against `spawn_selection::mobs_at`, the server's own function: the structure's
//! list, the piece or bounds flag, the first structure found, a category it does not cover, the fortress on nether bricks, the mods' hook.
//! The server side needs a world (`HORIZON_TEST_RESOURCES`), its level being `spawn_reference::RefLevel` with the structures and the hook given
//! here; the expected names are written by hand, and the pure tests run without the world.
use horizon_client_lib::biome::SpawnEntry;
use horizon_client_lib::mobs::spawn_rules::SpawnLevel;
use horizon_client_lib::mobs::spawn_selection::{self as sel, mobs_at, Heightmap, NaturalLevel, Override, StructureAt, StructureSpawns};
use horizon_core::fast_hash::FxHashMap;
use horizon_core::geometry::Aabb;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::spawn_reference::RefLevel;
use horizon_ecs_proto::spawn_structures::*;
use horizon_ecs_proto::spawning::{Category, Entry};
use std::collections::HashMap;
use std::sync::Arc;

const POS: [i32; 3] = [16, 64, 16];

struct Fixed {
    found: Vec<StructureAt>,
}
fn answer(found: &[StructureAt], wanted: &dyn Fn(&str) -> bool) -> Vec<StructureAt> {
    found.iter().filter(|s| wanted(&s.id)).cloned().collect()
}
impl StructureSource for Fixed {
    fn structures_at(&self, _: [i32; 3], wanted: &dyn Fn(&str) -> bool) -> Vec<StructureAt> {
        answer(&self.found, wanted)
    }
}
struct AddZombie(u8);
impl SpawnHook for AddZombie {
    fn potential_spawns(&self, cat: Category, _: [i32; 3], entries: &mut Vec<Entry>) {
        if cat == Category::Monster {
            entries.push(Entry { species: self.0, weight: 5, min: 1, max: 1, constant: true });
        }
    }
}

/// The server's level: the reference level's world and biome answers, with the structures, their lists and the hook given here.
struct Lvl<'a> {
    r: RefLevel<'a>,
    found: Vec<StructureAt>,
    spawns: StructureSpawns,
    biome: HashMap<String, Vec<SpawnEntry>>,
    hook: bool,
}
impl SpawnLevel for Lvl<'_> {
    fn within_world_border(&self, x: i32, z: i32) -> bool {
        SpawnLevel::within_world_border(&self.r, x, z)
    }
    fn state(&self, x: i32, y: i32, z: i32) -> horizon_world::registry::BlockStateId {
        SpawnLevel::state(&self.r, x, y, z)
    }
    fn facts(&self) -> &horizon_client_lib::blocks::BlockFacts {
        SpawnLevel::facts(&self.r)
    }
    fn mob_data(&self) -> &horizon_client_lib::mobs::data::MobData {
        SpawnLevel::mob_data(&self.r)
    }
    fn block_tag(&self, tag: &str, block: &str) -> bool {
        SpawnLevel::block_tag(&self.r, tag, block)
    }
    fn biome_tag(&self, tag: &str, x: i32, y: i32, z: i32) -> bool {
        SpawnLevel::biome_tag(&self.r, tag, x, y, z)
    }
    fn sky_light(&self, x: i32, y: i32, z: i32) -> i32 {
        SpawnLevel::sky_light(&self.r, x, y, z)
    }
    fn block_light(&self, x: i32, y: i32, z: i32) -> i32 {
        SpawnLevel::block_light(&self.r, x, y, z)
    }
    fn sky_darken(&self) -> i32 {
        SpawnLevel::sky_darken(&self.r)
    }
    fn thundering(&self) -> bool {
        SpawnLevel::thundering(&self.r)
    }
    fn sea_level(&self) -> i32 {
        SpawnLevel::sea_level(&self.r)
    }
    fn world_surface(&self, x: i32, z: i32) -> i32 {
        SpawnLevel::world_surface(&self.r, x, z)
    }
    fn peaceful(&self) -> bool {
        SpawnLevel::peaceful(&self.r)
    }
    fn dimension_type(&self) -> &horizon_world::dimension_type::DimensionFacts {
        SpawnLevel::dimension_type(&self.r)
    }
    fn surface_slime_chance(&self, x: i32, y: i32, z: i32) -> f32 {
        SpawnLevel::surface_slime_chance(&self.r, x, y, z)
    }
    fn world_seed(&self) -> i64 {
        SpawnLevel::world_seed(&self.r)
    }
    fn player_near(&self, x: f64, y: f64, z: f64, distance: f64) -> bool {
        SpawnLevel::player_near(&self.r, x, y, z, distance)
    }
}
impl NaturalLevel for Lvl<'_> {
    fn spawn_hooks(&self) -> bool {
        self.hook
    }
    fn potential_spawns(&self, category: sel::Category, _: [i32; 3], entries: &mut Vec<SpawnEntry>) {
        if category == sel::Category::Monster {
            entries.push(line("minecraft:zombie"));
        }
    }
    fn min_y(&self) -> i32 {
        NaturalLevel::min_y(&self.r)
    }
    fn max_y(&self) -> i32 {
        NaturalLevel::max_y(&self.r)
    }
    fn ambient_light(&self) -> f32 {
        NaturalLevel::ambient_light(&self.r)
    }
    fn has_ceiling(&self) -> bool {
        NaturalLevel::has_ceiling(&self.r)
    }
    fn mob_type(&self, entity: &str) -> Option<sel::MobType> {
        NaturalLevel::mob_type(&self.r, entity)
    }
    fn biome_spawns(&self, _: i32, _: i32, _: i32) -> Option<&HashMap<String, Vec<SpawnEntry>>> {
        Some(&self.biome)
    }
    fn spawn_cost(&self, entity: &str, x: i32, y: i32, z: i32) -> Option<sel::SpawnCost> {
        NaturalLevel::spawn_cost(&self.r, entity, x, y, z)
    }
    fn creature_probability(&self, x: i32, y: i32, z: i32) -> f32 {
        NaturalLevel::creature_probability(&self.r, x, y, z)
    }
    fn structures_at(&self, _: i32, _: i32, _: i32, wanted: &dyn Fn(&str) -> bool) -> Vec<StructureAt> {
        answer(&self.found, wanted)
    }
    fn structure_spawns(&self) -> &StructureSpawns {
        &self.spawns
    }
    fn nearest_player_positions(&self) -> &[[f64; 3]] {
        NaturalLevel::nearest_player_positions(&self.r)
    }
    fn respawn(&self) -> Option<[i32; 3]> {
        NaturalLevel::respawn(&self.r)
    }
    fn can_spawn_in_chunk(&self, cx: i32, cz: i32) -> bool {
        NaturalLevel::can_spawn_in_chunk(&self.r, cx, cz)
    }
    fn no_collision(&self, bounds: Aabb) -> bool {
        NaturalLevel::no_collision(&self.r, bounds)
    }
    fn unobstructed(&self, bounds: Aabb) -> bool {
        NaturalLevel::unobstructed(&self.r, bounds)
    }
    fn spawn_mobs(&self) -> bool {
        NaturalLevel::spawn_mobs(&self.r)
    }
    fn heightmap(&self, kind: Heightmap, x: i32, z: i32) -> i32 {
        NaturalLevel::heightmap(&self.r, kind, x, z)
    }
    fn pathfindable_land(&self, x: i32, y: i32, z: i32) -> bool {
        NaturalLevel::pathfindable_land(&self.r, x, y, z)
    }
}

fn have() -> bool {
    horizon_ecs_proto::species::inscribe_every_module();
    std::env::var_os("HORIZON_TEST_RESOURCES").is_some()
}
fn id(name: &str) -> u8 {
    species_table().0.iter().find(|s| s.name == name).map(|s| s.id).unwrap()
}
fn line(entity: &str) -> SpawnEntry {
    SpawnEntry { entity: entity.into(), weight: 5, min: 1, max: 1, constant: true }
}
fn at(id: &str, in_piece: bool, in_bounds: bool) -> StructureAt {
    StructureAt { id: id.into(), in_piece, in_bounds }
}
/// Monster overrides, one per structure: (structure, bounding box is the piece, species).
fn structures(list: &[(&str, bool, &str)]) -> StructureSpawns {
    let mut overrides = FxHashMap::default();
    for (sid, piece, entity) in list {
        let mut cats = FxHashMap::default();
        cats.insert("monster".to_string(), Override { piece: *piece, spawns: vec![line(entity)] });
        overrides.insert(sid.to_string(), cats);
    }
    StructureSpawns { overrides }
}
/// The biome's lists: bats, monster and creature.
fn biome() -> [Vec<Entry>; 8] {
    let bat = Entry { species: id("minecraft:bat"), weight: 1, min: 1, max: 1, constant: true };
    let mut b: [Vec<Entry>; 8] = Default::default();
    b[Category::Monster as usize].push(bat);
    b[Category::Creature as usize].push(bat);
    b
}
fn biome_map() -> HashMap<String, Vec<SpawnEntry>> {
    HashMap::from([("monster".to_string(), vec![line("minecraft:bat")]), ("creature".to_string(), vec![line("minecraft:bat")])])
}
fn names(list: &[Entry]) -> Vec<String> {
    let table = species_table();
    list.iter().map(|e| table.0[e.species as usize].name.to_string()).collect()
}
fn srv_names(list: &[SpawnEntry]) -> Vec<String> {
    list.iter().map(|e| e.entity.clone()).collect()
}
fn srv_cat(c: Category) -> sel::Category {
    match c {
        Category::Monster => sel::Category::Monster,
        _ => sel::Category::Creature,
    }
}

struct Case {
    found: Vec<StructureAt>,
    spawns: StructureSpawns,
    hook: bool,
    cat: Category,
    nether: bool,
    want: Vec<&'static str>,
}
fn cases() -> Vec<Case> {
    let pillager = || structures(&[("minecraft:pillager_outpost", false, "minecraft:zombie")]);
    let bastion = || structures(&[("minecraft:bastion_remnant", true, "minecraft:zombie")]);
    let both = || structures(&[("minecraft:pillager_outpost", false, "minecraft:zombie"), ("minecraft:bastion_remnant", false, "minecraft:spider")]);
    // The fortress's blaze, zombified piglin and magma cube have no `SpawnDef` yet: they are counted `unported` by `resolve`, not listed.
    let fortress_only = || StructureSpawns::default();
    vec![
        Case { found: vec![at("minecraft:pillager_outpost", false, true)], spawns: pillager(), hook: false, cat: Category::Monster, nether: false, want: vec!["minecraft:zombie"] },
        Case { found: vec![], spawns: pillager(), hook: false, cat: Category::Monster, nether: false, want: vec!["minecraft:bat"] },
        Case { found: vec![at("minecraft:bastion_remnant", false, true)], spawns: bastion(), hook: false, cat: Category::Monster, nether: false, want: vec!["minecraft:bat"] },
        Case { found: vec![at("minecraft:bastion_remnant", true, true)], spawns: bastion(), hook: false, cat: Category::Monster, nether: false, want: vec!["minecraft:zombie"] },
        Case { found: vec![at("minecraft:pillager_outpost", false, true), at("minecraft:bastion_remnant", false, true)], spawns: both(), hook: false, cat: Category::Monster, nether: false, want: vec!["minecraft:zombie"] },
        Case { found: vec![at("minecraft:bastion_remnant", false, true), at("minecraft:pillager_outpost", false, true)], spawns: both(), hook: false, cat: Category::Monster, nether: false, want: vec!["minecraft:spider"] },
        Case { found: vec![at("minecraft:pillager_outpost", false, true)], spawns: pillager(), hook: false, cat: Category::Creature, nether: false, want: vec!["minecraft:bat"] },
        Case { found: vec![], spawns: pillager(), hook: true, cat: Category::Monster, nether: false, want: vec!["minecraft:bat", "minecraft:zombie"] },
        Case { found: vec![at("minecraft:fortress", false, true)], spawns: fortress_only(), hook: false, cat: Category::Monster, nether: false, want: vec!["minecraft:bat"] },
        Case { found: vec![at("minecraft:fortress", false, true)], spawns: fortress_only(), hook: false, cat: Category::Monster, nether: true, want: vec!["minecraft:wither_skeleton", "minecraft:skeleton"] },
    ]
}
fn host(c: &Case) -> StructureHost {
    let h = StructureHost::new(Arc::new(Fixed { found: c.found.clone() }), &c.spawns);
    if c.hook { h.with_hook(Arc::new(AddZombie(id("minecraft:zombie")))) } else { h }
}

#[test]
fn spawn_lists_at_structures_are_the_expected_ones() {
    for c in cases() {
        let b = biome();
        let got = names(&host(&c).list_at(&b, c.cat, POS, || c.nether));
        assert_eq!(got, c.want, "found {:?}, nether {}", c.found, c.nether);
    }
}

#[test]
fn spawn_lists_agree_with_the_server_mobs_at() {
    if !have() {
        return;
    }
    let world = Herd::with_spawn_world_awake(176, true);
    let mw = world.world.resource::<Level>().mw.0;
    for c in cases().into_iter().filter(|c| !c.nether) {
        let b = biome();
        let ecs = names(&host(&c).list_at(&b, c.cat, POS, || false));
        let lvl = Lvl { r: RefLevel::new(mw, "minecraft:plains"), found: c.found.clone(), spawns: c.spawns.clone(), biome: biome_map(), hook: c.hook };
        let server = srv_names(&mobs_at(&lvl, srv_cat(c.cat), POS[0], POS[1], POS[2]));
        assert_eq!(ecs, server, "found {:?}, hook {}", c.found, c.hook);
    }
}
