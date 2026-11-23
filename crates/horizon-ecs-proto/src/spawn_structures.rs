//! The structures that change the spawn lists, as the natural spawn lists read them. The host answers which structures cover a position
//! (`StructureSource`: the ECS does not generate the world); the lists are the game's data (`StructureSpawns`, read by the server's own
//! loader), resolved to species once when the host is made, never per tick. The mods' `potential_spawns` hook is the host's too.
use crate::cow::{species_table, SpeciesTable};
use crate::spawning::{resolve, Category, Entry};
use bevy_ecs::prelude::Resource;
use horizon_client_lib::mobs::spawn_place::{fortress_enemies, StructureAt, StructureSpawns};
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

/// The host's answer to « which structures cover this position »: those whose references reach the position's chunk and whose id `wanted`
/// takes, in the chunk's reference order (`ChunkGenerator.getMobsAt`'s structures).
pub trait StructureSource: Send + Sync {
    fn structures_at(&self, pos: [i32; 3], wanted: &dyn Fn(&str) -> bool) -> Vec<StructureAt>;
}

/// The mods' `potential_spawns`: may add or remove lines of a category's list at a position. Registered by the host; absent, the list is
/// borrowed as it is, with no copy.
pub trait SpawnHook: Send + Sync {
    fn potential_spawns(&self, cat: Category, pos: [i32; 3], entries: &mut Vec<Entry>);
}

/// The host's source, its hook and the structures' lists. Absent (no resource), the biome's lists alone are used.
#[derive(Resource, Clone)]
pub struct StructureHost {
    source: Arc<dyn StructureSource>,
    lists: Arc<StructureLists>,
    hook: Option<Arc<dyn SpawnHook>>,
}

/// Each structure's `spawn_overrides` by id then category index (the bounding box is the piece, the resolved list), and the fortress's enemies.
#[derive(Default)]
pub struct StructureLists {
    overrides: HashMap<String, [Option<(bool, Vec<Entry>)>; 8]>,
    fortress: Vec<Entry>,
    covered: [bool; 8],
    /// The lines whose species the prototype does not have, never silently dropped.
    pub unported: Vec<String>,
}
impl StructureLists {
    fn new(spawns: &StructureSpawns, species: &SpeciesTable) -> Self {
        let mut l = Self::default();
        for (id, cats) in &spawns.overrides {
            for (name, o) in cats {
                let Some(c) = Category::from_name(name) else { continue };
                let mut list = Vec::new();
                resolve(&mut list, &mut l.unported, &o.spawns, species);
                l.overrides.entry(id.clone()).or_default()[c as usize] = Some((o.piece, list));
                l.covered[c as usize] = true;
            }
        }
        resolve(&mut l.fortress, &mut l.unported, fortress_enemies(), species);
        l
    }
    fn has(&self, id: &str, cat: Category) -> bool {
        self.overrides.get(id).is_some_and(|c| c[cat as usize].is_some())
    }
}

impl StructureHost {
    pub fn new(source: Arc<dyn StructureSource>, spawns: &StructureSpawns) -> Self {
        Self { source, lists: Arc::new(StructureLists::new(spawns, &species_table())), hook: None }
    }
    /// Registers the mods' `potential_spawns` (`spawn_hooks` on the server).
    pub fn with_hook(mut self, hook: Arc<dyn SpawnHook>) -> Self {
        self.hook = Some(hook);
        self
    }
    /// Whether a piece of the structure `id` holds the position (`StructureManager.getStructureWithPieceAt`), read once at a spawn.
    pub fn has_piece_at(&self, pos: [i32; 3], id: &str) -> bool {
        self.source.structures_at(pos, &|s| s == id).iter().any(|s| s.in_piece)
    }
    /// Whether some structure overrides the category's spawns.
    pub fn covers(&self, cat: Category) -> bool {
        self.lists.covered[cat as usize]
    }
    /// The list of a category at a position, as `NaturalSpawner.mobsAt`: the structures' list (see `vanilla_list_at`), then the hook.
    /// `nether_bricks` tells whether the block under the position is nether bricks.
    pub fn list_at<'a>(&'a self, biome: &'a [Vec<Entry>; 8], cat: Category, pos: [i32; 3], nether_bricks: impl FnOnce() -> bool) -> Cow<'a, [Entry]> {
        let list = self.vanilla_list_at(biome, cat, pos, nether_bricks);
        match &self.hook {
            None => Cow::Borrowed(list),
            Some(h) => {
                let mut v = list.to_vec();
                h.potential_spawns(cat, pos, &mut v);
                Cow::Owned(v)
            }
        }
    }
    /// `vanilla_mobs_at`: the fortress's enemies for a monster on nether bricks inside a fortress's bounds, else the first structure
    /// referenced there whose override for the category holds the position (its piece when `piece`, else its bounds), else the biome's list.
    fn vanilla_list_at<'a>(&'a self, biome: &'a [Vec<Entry>; 8], cat: Category, pos: [i32; 3], nether_bricks: impl FnOnce() -> bool) -> &'a [Entry] {
        let monster = cat == Category::Monster;
        let lists = &*self.lists;
        let found = self.source.structures_at(pos, &|id| (monster && id == "minecraft:fortress") || lists.has(id, cat));
        if monster && nether_bricks() && found.iter().any(|s| s.id == "minecraft:fortress" && s.in_bounds) {
            return &lists.fortress;
        }
        for s in &found {
            if let Some((piece, list)) = lists.overrides.get(&s.id).and_then(|c| c[cat as usize].as_ref())
                && (if *piece { s.in_piece } else { s.in_bounds })
            {
                return list;
            }
        }
        biome[cat as usize].as_slice()
    }
}
