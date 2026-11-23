//! The spawn lists at a position (`StructureHost::list_at`) against the expected names, written by hand from the jar:
//! the structure's list, the piece or bounds flag, the first structure found, a category it does not cover, the fortress on
//! nether bricks, the mods' hook.
use myriade_engine_lib::biome::SpawnEntry;
use myriade_engine_lib::mobs::spawn_place::{Override, StructureAt, StructureSpawns};
use horizon_core::fast_hash::FxHashMap;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::spawn_structures::*;
use horizon_ecs_proto::spawning::{Category, Entry};
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
fn names(list: &[Entry]) -> Vec<String> {
    let table = species_table();
    list.iter().map(|e| table.0[e.species as usize].name.to_string()).collect()
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
