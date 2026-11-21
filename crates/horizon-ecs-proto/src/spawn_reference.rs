//! Horizon's natural spawning, run as the game runs it (`spawn_selection::spawn_for_chunk` of `NaturalSpawner`, one pass per tick over the
//! chunks of the players), on the prototype's real test world (`cow::Level.mw`). The control of `spawning.rs` and the cost it is measured
//! against. The level is the game's `NaturalLevel` with what a test world has: ONE biome (its real spawn lists), no structure, no spawn
//! charge, no mod hook. The mobs it spawns are not finalized (a position, a species, a category): nothing in the spawn decision reads more.
use crate::rules::spawn_reference::{chunk_near, chunk_of};
use horizon_client_lib::biome::{BiomeInfo, SpawnEntry};
use horizon_client_lib::mobs::spawn_rules::SpawnLevel;
use horizon_client_lib::mobs::spawn_selection::{self as sel, Category, Counted, Heightmap, NaturalLevel, SpawnCost, SpawnPlayer, SpawnRequest, SpawnState, Spawned, StructureAt, StructureSpawns};
use horizon_client_lib::mobs::{MobWorld, Species};
use horizon_core::geometry::Aabb;
use horizon_core::random::{LegacyRandom, RandomSource};
use std::cell::RefCell;
use std::collections::HashMap;

/// A mob the reference spawned.
#[derive(Clone, Debug)]
pub struct RefMob {
    pub entity: String,
    pub category: Category,
    pub block: [i32; 3],
    pub position: [f64; 3],
}

/// The game's `NaturalLevel` over the test world.
pub struct RefLevel<'a> {
    pub w: &'a MobWorld<'static>,
    pub biome: &'a BiomeInfo,
    kinds: HashMap<String, sel::MobType>,
    /// The non-spectator players.
    pub players: Vec<[f64; 3]>,
    boxes: RefCell<HashMap<(i32, i32), Vec<Aabb>>>,
    pub spawned: RefCell<Vec<RefMob>>,
    structures: StructureSpawns,
    /// The biome's lists (what `biome_spawns` answers): the biome's own, less the entries `drop_entities` took out.
    spawns: HashMap<String, Vec<SpawnEntry>>,
}
impl<'a> RefLevel<'a> {
    /// Takes entries out of the lists (the species the other side does not have): the comparison is then of the same lists.
    pub fn drop_entities(&mut self, names: &[String]) {
        for list in self.spawns.values_mut() {
            list.retain(|e| !names.contains(&e.entity));
        }
    }
    pub fn new(w: &'a MobWorld<'static>, biome: &str) -> Self {
        let i = *w.biomes.index.get(biome).unwrap_or_else(|| panic!("biome {biome}"));
        let info = w.biomes.get(i);
        let mut kinds = HashMap::new();
        for list in info.spawns.values() {
            for e in list {
                let k = crate::cow::mob_kind(&e.entity);
                kinds.insert(e.entity.clone(), (k.species, k.width as f32, k.height as f32, Category::from_name(&k.category)));
            }
        }
        Self { w, biome: info, kinds, players: Vec::new(), boxes: RefCell::new(HashMap::new()), spawned: RefCell::new(Vec::new()), structures: StructureSpawns::default(), spawns: info.spawns.clone() }
    }
    fn add_box(&self, b: Aabb) {
        let (x0, x1, z0, z1) = (b.min_position().x.floor() as i32, b.max_position().x.floor() as i32, b.min_position().z.floor() as i32, b.max_position().z.floor() as i32);
        let mut m = self.boxes.borrow_mut();
        for x in x0..=x1 {
            for z in z0..=z1 {
                m.entry((x, z)).or_default().push(b);
            }
        }
    }
}
impl SpawnLevel for RefLevel<'_> {
    fn within_world_border(&self, x: i32, z: i32) -> bool {
        self.w.within_world_border(x, z)
    }
    fn state(&self, x: i32, y: i32, z: i32) -> horizon_world::registry::BlockStateId {
        SpawnLevel::state(self.w, x, y, z)
    }
    fn facts(&self) -> &horizon_client_lib::blocks::BlockFacts {
        SpawnLevel::facts(self.w)
    }
    fn mob_data(&self) -> &horizon_client_lib::mobs::data::MobData {
        self.w.mob_data()
    }
    fn block_tag(&self, tag: &str, block: &str) -> bool {
        self.w.block_tag(tag, block)
    }
    fn biome_tag(&self, tag: &str, _: i32, _: i32, _: i32) -> bool {
        self.w.data.biome_tags.get(tag.trim_start_matches('#')).is_some_and(|m| m.contains(&self.biome.name))
    }
    fn sky_light(&self, x: i32, y: i32, z: i32) -> i32 {
        self.w.sky_light(x, y, z)
    }
    fn block_light(&self, x: i32, y: i32, z: i32) -> i32 {
        self.w.block_light(x, y, z)
    }
    fn sky_darken(&self) -> i32 {
        SpawnLevel::sky_darken(self.w)
    }
    fn thundering(&self) -> bool {
        self.w.thundering()
    }
    fn sea_level(&self) -> i32 {
        self.w.sea_level()
    }
    fn world_surface(&self, x: i32, z: i32) -> i32 {
        SpawnLevel::world_surface(self.w, x, z)
    }
    fn peaceful(&self) -> bool {
        self.w.peaceful()
    }
    fn dimension_type(&self) -> &horizon_world::dimension_type::DimensionFacts {
        self.w.dimension_type()
    }
    fn surface_slime_chance(&self, x: i32, y: i32, z: i32) -> f32 {
        self.w.surface_slime_chance(x, y, z)
    }
    fn world_seed(&self) -> i64 {
        self.w.world_seed()
    }
    fn player_near(&self, x: f64, y: f64, z: f64, distance: f64) -> bool {
        self.players.iter().any(|p| (p[0] - x).powi(2) + (p[1] - y).powi(2) + (p[2] - z).powi(2) < distance * distance)
    }
}
impl NaturalLevel for RefLevel<'_> {
    fn min_y(&self) -> i32 {
        -64
    }
    fn max_y(&self) -> i32 {
        319
    }
    fn ambient_light(&self) -> f32 {
        0.0
    }
    fn has_ceiling(&self) -> bool {
        false
    }
    fn mob_type(&self, entity: &str) -> Option<sel::MobType> {
        self.kinds.get(entity).copied()
    }
    fn biome_spawns(&self, _: i32, _: i32, _: i32) -> Option<&HashMap<String, Vec<SpawnEntry>>> {
        Some(&self.spawns)
    }
    fn spawn_cost(&self, entity: &str, _: i32, _: i32, _: i32) -> Option<SpawnCost> {
        self.biome.spawn_costs.get(entity).copied()
    }
    fn creature_probability(&self, _: i32, _: i32, _: i32) -> f32 {
        self.biome.creature_probability
    }
    fn structures_at(&self, _: i32, _: i32, _: i32, _: &dyn Fn(&str) -> bool) -> Vec<StructureAt> {
        Vec::new()
    }
    fn structure_spawns(&self) -> &StructureSpawns {
        &self.structures
    }
    fn nearest_player_positions(&self) -> &[[f64; 3]] {
        &self.players
    }
    fn respawn(&self) -> Option<[i32; 3]> {
        None
    }
    fn can_spawn_in_chunk(&self, cx: i32, cz: i32) -> bool {
        self.w.world.chunks.contains_key(&(cx, cz))
    }
    fn no_collision(&self, bounds: Aabb) -> bool {
        self.w.no_collision(bounds)
    }
    fn unobstructed(&self, bounds: Aabb) -> bool {
        let m = self.boxes.borrow();
        let (x0, x1, z0, z1) = (bounds.min_position().x.floor() as i32, bounds.max_position().x.floor() as i32, bounds.min_position().z.floor() as i32, bounds.max_position().z.floor() as i32);
        !(x0..=x1).any(|x| (z0..=z1).any(|z| m.get(&(x, z)).is_some_and(|v| v.iter().any(|b| b.intersects(bounds)))))
    }
    fn spawn_mobs(&self) -> bool {
        true
    }
    fn heightmap(&self, kind: Heightmap, x: i32, z: i32) -> i32 {
        match kind {
            Heightmap::WorldSurface => SpawnLevel::world_surface(self.w, x, z),
            _ => (self.min_y()..=self.max_y()).rev().find(|&y| sel::heightmap_blocks(self, kind, x, y, z)).map_or(self.min_y(), |y| y + 1),
        }
    }
    fn pathfindable_land(&self, x: i32, y: i32, z: i32) -> bool {
        self.w.data.pathfindable(SpawnLevel::state(self.w, x, y, z), horizon_client_lib::mobs::data::PathComputationType::Land)
    }
}

/// The index of a category in `Category::ALL`.
pub fn category_index(c: Category) -> usize {
    Category::ALL.iter().position(|x| *x == c).unwrap()
}

/// What a pass did, for the cost lines.
#[derive(Clone, Copy, Debug, Default)]
pub struct PassStats {
    pub candidates: usize,
    pub categories: usize,
    pub spawned: usize,
}

/// `ServerChunkCache.tickChunks` for one level: the state counted from the mobs, the categories under their caps, the shuffled chunks.
pub struct RefSpawner<'a> {
    pub level: RefLevel<'a>,
    pub mobs: Vec<RefMob>,
    pub random: LegacyRandom,
    pub game_time: i64,
    buffers: sel::SpawnBuffers,
    counter: Option<crate::instr::Counter>,
    /// Instructions of the last pass: the candidate chunks, the state counted from the mobs, the shuffle and the chunks' attempts.
    pub laps: [u64; 4],
    /// What the trace of the last pass saw, by category: local cap refused, starts, spawns; and the attempts of the pass.
    pub trace: [[u64; 3]; 8],
    pub tries: u64,
}
impl<'a> RefSpawner<'a> {
    pub fn new(level: RefLevel<'a>, seed: i64) -> Self {
        Self { level, mobs: Vec::new(), random: LegacyRandom::new(seed), game_time: 1, buffers: Default::default(), counter: crate::instr::Counter::new(), laps: [0; 4], trace: [[0; 3]; 8], tries: 0 }
    }
    /// The 17 x 17 chunk squares of the players, each chunk once, those within 128 blocks of a player kept (`collectSpawningChunks`), and
    /// the size of the union.
    pub fn candidates(&self) -> (Vec<(i32, i32)>, i32) {
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for p in &self.level.players {
            let (pcx, pcz) = chunk_of(*p);
            for dx in -8..=8 {
                for dz in -8..=8 {
                    let c = (pcx + dx, pcz + dz);
                    if !seen.insert(c) {
                        continue;
                    }
                    if self.level.players.iter().any(|q| chunk_near(c, *q)) {
                        out.push(c);
                    }
                }
            }
        }
        (out, seen.len() as i32)
    }
    /// One tick of natural spawning (the pass `spawn_natural` runs), the game time advancing by one afterwards.
    pub fn pass(&mut self) -> PassStats {
        let read = |c: &Option<crate::instr::Counter>| c.as_ref().map_or(0, crate::instr::Counter::read);
        let t0 = read(&self.counter);
        let (mut chunks, spawnable) = self.candidates();
        let t1 = read(&self.counter);
        let counted: Vec<Counted> = self.mobs.iter().map(|m| Counted { entity: &m.entity, category: m.category, block: m.block, is_mob: true, persistent: false }).collect();
        let players: Vec<SpawnPlayer> = self.level.players.iter().map(|p| SpawnPlayer { position: *p, spectator: false }).collect();
        let buffers = std::mem::take(&mut self.buffers);
        let mut state = SpawnState::new_in(&self.level, spawnable, players, buffers, &counted, |_, _| true);
        let categories = state.filtered_categories(true, self.game_time % 400 == 0);
        let t2 = read(&self.counter);
        for i in (1..chunks.len()).rev() {
            let j = self.random.next_int_bound(i as i32 + 1).unwrap_or(0) as usize;
            chunks.swap(i, j);
        }
        let n_chunks = chunks.len();
        let t3 = read(&self.counter);
        if !categories.is_empty() {
            let mut spawn = |l: &RefLevel, r: &SpawnRequest, _: &mut LegacyRandom| {
                let (_, width, height, category) = l.mob_type(r.entity)?;
                let b = sel::spawn_box(r.position[0], r.position[1], r.position[2], width, height);
                l.add_box(b);
                l.spawned.borrow_mut().push(RefMob { entity: r.entity.to_owned(), category, block: [r.position[0].floor() as i32, r.position[1].floor() as i32, r.position[2].floor() as i32], position: r.position });
                Some(Spawned { bounds: b, school: false, cluster_size: None })
            };
            let mut tr = [[0u64; 3]; 8];
            let mut tries = 0u64;
            let kinds = &self.level;
            for c in chunks {
                sel::spawn_for_chunk(&self.level, &mut self.random, &mut state, &categories, c, &mut spawn, &mut |e| match e {
                    sel::Event::Local(cat, false) => tr[category_index(cat)][0] += 1,
                    sel::Event::Start(cat, _) => tr[category_index(cat)][1] += 1,
                    sel::Event::Try(..) => tries += 1,
                    sel::Event::Spawn(name, ..) => {
                        if let Some(t) = kinds.mob_type(name) {
                            tr[category_index(t.3)][2] += 1;
                        }
                    }
                    _ => {}
                });
            }
            self.trace = tr;
            self.tries = tries;
        }
        let t4 = read(&self.counter);
        self.laps = [t1 - t0, t2 - t1, t3 - t2, t4 - t3];
        self.buffers = state.into_parts().1;
        let fresh: Vec<RefMob> = self.level.spawned.borrow_mut().drain(..).collect();
        let spawned = fresh.len();
        self.mobs.extend(fresh);
        self.game_time += 1;
        PassStats { candidates: n_chunks, categories: categories.len(), spawned }
    }
    pub fn species_of(&self, entity: &str) -> Option<Species> {
        self.level.mob_type(entity).map(|t| t.0)
    }
}
