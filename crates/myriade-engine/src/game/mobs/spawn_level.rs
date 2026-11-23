//! The game's side of the natural spawning: the level
//! it reads, the spawning chunks and caps of `ServerChunkCache.tickChunks`,
//! and the mobs it adds (finalized with the game's `finalizeSpawn`).
use super::{Dim, Game, structures::reaches_in};
use myriade_engine::vanillagen::VanillaTerrain;
use myriade_engine::{
    biome::{BiomeInfo, SpawnEntry},
    blocks::BlockFacts,
    collisions::{SceneCollisions, no_collision_empty},
    mobs::{
        MobWorld, Species,
        data::{self, MobData},
        seen_grid::SeenGrid,
        finalize,
        spawn_rules::{Reason, SpawnLevel},
        spawn_place::{
            self, Category, Heightmap, NaturalLevel, PlayerSquares, SpawnCost, StructureAt,
            StructureSpawns,
        },
    },
};
use horizon_core::{
    biome::BiomeZoom,
    geometry::{Aabb, Axis, Vec3},
    position::BlockPos,
};
use horizon_world::dimension_type::DimensionFacts;
use horizon_ecs_proto::bridge::{host::Select, view};
use myriade_types::SpawnCheck;
use myriade_types::SpawnFlags;
use myriade_types::SpawnLimits;
use std::collections::HashMap;
use std::sync::Arc;
use horizon_world::registry::BlockStateId;

/// A level's natural spawning policy, as mods set it through its `LevelRef`
/// (`Game::spawn_policies`): the categories' caps and which may spawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct SpawnPolicy {
    pub(super) limits: SpawnLimits,
    pub(super) flags: SpawnFlags,
}

impl Game<'_> {
    /// The mods' policy of the current level as the ECS spawner reads it (`spawning::Policy`).
    pub(in crate::game) fn ecs_spawn_policy(&self) -> horizon_ecs_proto::spawning::Policy {
        let p = self.spawn_policy(self.dim.key());
        let l = p.limits;
        let caps = [l.monster, l.creature, l.ambient, l.axolotls, l.underground_water_creature, l.water_creature, l.water_ambient];
        let mut out = [-1; 8];
        for (o, c) in out.iter_mut().zip(caps) {
            *o = c as i32;
        }
        horizon_ecs_proto::spawning::Policy { caps: out, monsters: p.flags.monsters, friendlies: p.flags.friendlies }
    }
}
/// `ServerLevel.canSpawnEntitiesInChunk` of `world`: the chunk is loaded.
pub(super) fn can_spawn_entities_in_chunk(
    world: &myriade_engine::world::World,
    cx: i32,
    cz: i32,
) -> bool {
    world.chunks.contains_key(&(cx, cz))
}

/// Natural spawning's candidate chunks (`ChunkMap.collectSpawningChunks`):
/// the union of every player's 17×17 chunk square, in player then dx/dz
/// order, each chunk once. `candidates` is one pass's working list (spawning
/// shuffles and drains it); the rest is the union for the players' last chunk
/// positions, kept between ticks. A chunk crossing rebuilds only the squares
/// it can change, and proximity to players is checked live on every pass.
#[derive(Default)]
pub(super) struct SpawnScratch {
    pub(super) candidates: Vec<(i32, i32)>,
    /// Each player's chunk at the last pass.
    chunks: Vec<(i32, i32)>,
    /// For each player, the others whose squares overlap its own, in order.
    neighbours: Vec<Vec<usize>>,
    /// The chunks each player adds to the union: those of its square, in
    /// dx/dz order, that no earlier player's square holds.
    segments: Vec<Vec<(i32, i32)>>,
    stale: Vec<bool>,
}
impl SpawnScratch {
    fn chunk(p: &LevelPlayer) -> (i32, i32) {
        (
            p.position.x.floor() as i32 >> 4,
            p.position.z.floor() as i32 >> 4,
        )
    }
    /// Whether the square around chunk `centre` holds chunk `c`.
    fn holds(centre: (i32, i32), c: (i32, i32)) -> bool {
        centre.0.abs_diff(c.0) <= 8 && centre.1.abs_diff(c.1) <= 8
    }
    fn overlap(a: (i32, i32), b: (i32, i32)) -> bool {
        a.0.abs_diff(b.0) <= 16 && a.1.abs_diff(b.1) <= 16
    }
    /// Bring the segments up to date with the players' chunks. A segment
    /// depends only on its player's chunk and those of the earlier players
    /// whose squares overlap it, so a move rebuilds the mover's segment and
    /// the later ones its old or new square overlaps.
    fn prepare(&mut self, players: &[LevelPlayer]) {
        let n = players.len();
        let all = n != self.chunks.len();
        self.stale.clear();
        self.stale.resize(n, all);
        if !all {
            let mut moved = false;
            for m in 0..n {
                let (old, new) = (self.chunks[m], Self::chunk(&players[m]));
                if old == new {
                    continue;
                }
                moved = true;
                self.stale[m] = true;
                for (i, p) in players.iter().enumerate().skip(m + 1) {
                    let c = Self::chunk(p);
                    self.stale[i] |= Self::overlap(old, c) || Self::overlap(new, c);
                }
            }
            if !moved {
                return;
            }
        }
        self.chunks.clear();
        self.chunks.extend(players.iter().map(Self::chunk));
        self.neighbours.resize_with(n, Vec::new);
        self.neighbours.truncate(n);
        self.segments.resize_with(n, Vec::new);
        self.segments.truncate(n);
        let chunks = &self.chunks;
        for (i, near) in self.neighbours.iter_mut().enumerate() {
            near.clear();
            near.extend((0..n).filter(|&j| j != i && Self::overlap(chunks[i], chunks[j])));
        }
        for (i, segment) in self.segments.iter_mut().enumerate() {
            if !self.stale[i] {
                continue;
            }
            let near = &self.neighbours[i];
            let earlier = &near[..near.partition_point(|&j| j < i)];
            let (pcx, pcz) = chunks[i];
            segment.clear();
            for dx in -8..=8 {
                for dz in -8..=8 {
                    let c = (pcx + dx, pcz + dz);
                    if !earlier.iter().any(|&j| Self::holds(chunks[j], c)) {
                        segment.push(c);
                    }
                }
            }
        }
    }
    /// Fill `candidates` with the union's chunks, in order, that are within
    /// 128 blocks of a non-spectator holding them in its square and that
    /// `keep` accepts. Returns the union's size.
    fn collect(
        &mut self,
        players: &[LevelPlayer],
        mut forced: impl FnMut((i32, i32)) -> bool,
        mut keep: impl FnMut((i32, i32)) -> bool,
    ) -> i32 {
        self.prepare(players);
        self.candidates.clear();
        let mut count = 0;
        for (i, segment) in self.segments.iter().enumerate() {
            count += segment.len();
            let near = &self.neighbours[i];
            for &c in segment {
                let close = close_enough_for_spawning(&players[i], c)
                    || near.iter().any(|&j| {
                        Self::holds(self.chunks[j], c) && close_enough_for_spawning(&players[j], c)
                    })
                    || forced(c);
                if close && keep(c) {
                    self.candidates.push(c);
                }
            }
        }
        count as i32
    }
}

/// A player in the level in place, as spawning and despawning see it.
#[derive(Clone)]
pub(super) struct LevelPlayer {
    pub(super) uuid: u128,
    pub(super) position: Vec3,
    pub(super) bounds: Aabb,
    pub(super) alive: bool,
    pub(super) creative: bool,
    pub(super) spectator: bool,
}
impl LevelPlayer {
    fn distance_sqr(&self, x: f64, y: f64, z: f64) -> f64 {
        (self.position.x - x).powi(2)
            + (self.position.y - y).powi(2)
            + (self.position.z - z).powi(2)
    }
    /// `Level.hasNearbyAlivePlayer`'s test of one player (any distance for a
    /// negative range).
    fn alive_near(&self, x: f64, y: f64, z: f64, range: f64) -> bool {
        self.alive && !self.spectator && (range < 0.0 || self.distance_sqr(x, y, z) < range * range)
    }
}
/// `ChunkMap.euclideanDistanceSquared` below 128 blocks
/// (`playerIsCloseEnoughForSpawning`).
fn close_enough_for_spawning(p: &LevelPlayer, (cx, cz): (i32, i32)) -> bool {
    let (x, z) = (
        f64::from(cx * 16 + 8) - p.position.x,
        f64::from(cz * 16 + 8) - p.position.z,
    );
    !p.spectator && x * x + z * z < 16384.0
}
/// Up to this many players, a spawn start measures to every one.
const PLAYERS_SCANNED: usize = 8;
/// What a level reads of the game beyond its world view, for the serial pass (`&Game`, which also
/// answers the server mods).
pub(super) trait LevelHost: Sized {
    /// The game's entities as box queries read them, listed before the grid is built.
    fn mob_boxes(&self) -> Vec<MobBox>;
    fn potential_spawns(&self, category: Category, pos: [i32; 3], entries: &mut Vec<SpawnEntry>);
}

impl LevelHost for &Game<'_> {
    fn mob_boxes(&self) -> Vec<MobBox> {
        self.ecs.mob_boxes()
    }
    fn potential_spawns(&self, _category: Category, _pos: [i32; 3], _entries: &mut Vec<SpawnEntry>) {
    }
}

/// The parts of the game a level reads, as plain shared borrows (all `Sync`).
#[derive(Clone)]
pub(super) struct SpawnHost<'g> {
    pub(super) biome_zoom: BiomeZoom,
    pub(super) border: &'g horizon_core::geometry::border::BorderGeometry,
    pub(super) mob_types: &'g HashMap<String, Arc<myriade_engine::mobs::MobType>>,
    pub(super) structure_spawns: &'g StructureSpawns,
    /// The terrain that locates the structures (the end state's).
    pub(super) locator: Option<Arc<VanillaTerrain>>,
    pub(super) respawn: Option<[i32; 3]>,
    pub(super) spawn_mobs: bool,
    /// `ServerLevel.isSpawningMonsters`, which finalizeSpawn reads.
    pub(super) spawning_monsters: bool,
    /// The tables a finalizeSpawn reads besides the level ([`Game::finalize_spawn_data`]).
    pub(super) registries: &'g myriade_engine::mobs::finalize::SpawnRegistries,
    pub(super) enchantments: &'g myriade_engine::enchant::Enchantments,
    pub(super) items: &'g myriade_engine::items::Items,
    pub(super) camel_husk_size: (f32, f32),
}
/// The game level as natural spawning reads it.
pub(super) type GameLevel<'g, 'a> = Level<'g, &'g Game<'a>>;
pub(super) struct Level<'g, H> {
    pub(super) game: H,
    pub(super) host: SpawnHost<'g>,
    /// The grids a pass builds once for all its lanes' levels.
    pub(super) shared_boxes: Option<&'g MobBoxes>,
    pub(super) shared_squares: Option<&'g PlayerSquares>,
    pub(super) w: MobWorld<'g>,
    /// Every player in the level ([`Game::level_players`]).
    pub(super) players: Vec<LevelPlayer>,
    /// Where the non-spectators among them are (`getNearestPlayer`).
    pub(super) nearest: Vec<[f64; 3]>,
    /// `nearest` by square, built when a spawn start first asks with more
    /// than [`PLAYERS_SCANNED`] players.
    pub(super) player_squares: std::cell::OnceCell<PlayerSquares>,
    /// Boxes of the mobs this pass created, not yet in the game's list.
    pub(super) pending: std::cell::RefCell<Vec<Aabb>>,
    /// The types and boxes of what a spawner created this tick.
    pub(super) pending_ids: std::cell::RefCell<Vec<(String, Aabb)>>,
    pub(super) rides: std::cell::RefCell<Vec<(u32, u32)>>,
    /// How many provisional ids the pass has handed out (`u32::MAX - n`) to
    /// the lent handles; a refused check gives them back.
    pub(super) provisional: std::cell::Cell<u32>,
    /// The game's entities by box, built at the first box query (they do not change while
    /// the pass holds the game).
    pub(super) boxes: std::cell::OnceCell<MobBoxes>,
    /// The entity types some biome of the level gives a spawn cost (the
    /// nether's few): for any other the cost is none wherever it stands, and
    /// spawning asks for one for every loaded mob each tick.
    pub(super) costed: Vec<&'g str>,
    /// The spawner block whose tick this pass is, told to the spawn checks;
    /// None for natural spawning and the other passes.
    pub(super) spawner: Option<myriade_types::Spawner>,
}
/// An entity: its id, box, alive, and whether it is a living shulker.
type MobBox = (u32, Aabb, bool, bool);
/// The boxes of the game's entities, and a grid over them.
pub(super) struct MobBoxes {
    entries: Vec<MobBox>,
    grid: SeenGrid,
}
impl MobBoxes {
    fn new(entries: Vec<MobBox>) -> Self {
        let grid = SeenGrid::new(entries.iter().map(|e| e.1));
        Self { entries, grid }
    }
    /// The entries whose boxes meet `area` (and possibly a few more).
    fn near(&self, area: Aabb) -> impl Iterator<Item = MobBox> + '_ {
        let mut near = Vec::new();
        let indexed = self.grid.candidates(area, &mut near);
        let all = (!indexed).then(|| self.entries.iter());
        near.into_iter()
            .map(|i| self.entries[i as usize])
            .chain(all.into_iter().flatten().copied())
    }
}
impl<H: LevelHost> Level<'_, H> {
    /// The entities whose boxes may meet `area` (callers test them).
    fn mobs_near(&self, area: Aabb) -> Box<dyn Iterator<Item = MobBox> + '_> {
        if let Some(shared) = self.shared_boxes {
            return Box::new(shared.near(area));
        }
        Box::new(
            self.boxes
                .get_or_init(|| MobBoxes::new(self.game.mob_boxes()))
                .near(area),
        )
    }
    fn biome(&self, x: i32, y: i32, z: i32) -> Option<&BiomeInfo> {
        Some(self.w.biomes.get(self.w.world.noise_biome(x, y, z)?))
    }
    /// The block whose noise cell `BiomeManager.getBiome` reads.
    fn zoomed(&self, x: i32, y: i32, z: i32) -> [i32; 3] {
        myriade_engine::mobs::zoomed_block(self.host.biome_zoom, x, y, z)
    }
    fn zoomed_biome(&self, x: i32, y: i32, z: i32) -> Option<&BiomeInfo> {
        Some(self.w.biomes.get(myriade_engine::mobs::zoomed_biome_index(&self.w, self.host.biome_zoom, x, y, z)?))
    }
    /// A provisional id, distinct within the pass, for a lent handle
    pub(super) fn provisional_id(&self) -> u32 {
        let n = self.provisional.get();
        self.provisional.set(n + 1);
        u32::MAX - n
    }
}

impl Game<'_> {
    /// The tables a finalizeSpawn `Ctx` borrows: the variant registries, the
    /// enchantments and items its gear rolls need, and `EntityTypes.CAMEL_HUSK`'s
    /// dimensions if the table has the type.
    pub(super) fn finalize_spawn_data(&self) -> finalize::SpawnData<'_> {
        let game = self;
        finalize::SpawnData {
            registries: game.res.spawn_registries(),
            enchantments: &world_stations!(game).enchantments,
            items: world_items!(game),
            camel_husk_size: game
                .res
                .mob_types()
                .get("minecraft:camel_husk")
                .map(|t| (t.width as f32, t.height as f32))
                .unwrap_or((1.7, 2.375)),
        }
    }
}
impl<H: LevelHost> SpawnLevel for Level<'_, H> {
    fn placement_check(&self, _: &str, _: Reason, _: [i32; 3], vanilla: bool) -> bool {
        vanilla
    }
    fn within_world_border(&self, x: i32, z: i32) -> bool {
        self.host
            .border
            .is_within_bounds(f64::from(x), f64::from(z), 0.0)
    }
    fn state(&self, x: i32, y: i32, z: i32) -> BlockStateId {
        self.w.state(x, y, z)
    }
    fn facts(&self) -> &BlockFacts {
        self.w.facts
    }
    fn mob_data(&self) -> &MobData {
        self.w.data
    }
    fn block_tag(&self, tag: &str, block: &str) -> bool {
        self.w.block_tag(tag, block)
    }
    // Level.getBiome: the zoomed biome.
    fn biome_tag(&self, tag: &str, x: i32, y: i32, z: i32) -> bool {
        let [x, y, z] = self.zoomed(x, y, z);
        self.w.biome_tag(tag, x, y, z)
    }
    fn sky_light(&self, x: i32, y: i32, z: i32) -> i32 {
        self.w.sky_light(x, y, z)
    }
    fn block_light(&self, x: i32, y: i32, z: i32) -> i32 {
        self.w.block_light(x, y, z)
    }
    fn sky_darken(&self) -> i32 {
        SpawnLevel::sky_darken(&self.w)
    }
    fn thundering(&self) -> bool {
        self.w.thundering
    }
    fn sea_level(&self) -> i32 {
        self.w.sea_level()
    }
    fn world_surface(&self, x: i32, z: i32) -> i32 {
        self.w.world_surface(x, z)
    }
    fn peaceful(&self) -> bool {
        self.w.peaceful()
    }
    fn dimension_type(&self) -> &DimensionFacts {
        self.w.dimension_type
    }
    fn surface_slime_chance(&self, x: i32, y: i32, z: i32) -> f32 {
        self.w.surface_slime_chance(x, y, z)
    }
    fn world_seed(&self) -> i64 {
        self.w.world_seed
    }
    /// `getNearestPlayer(x, y, z, distance, true)`, over every player.
    fn player_near(&self, x: f64, y: f64, z: f64, distance: f64) -> bool {
        self.players.iter().any(|p| {
            p.alive && !p.creative && !p.spectator && p.distance_sqr(x, y, z) < distance * distance
        })
    }
}
impl<H: LevelHost> NaturalLevel for Level<'_, H> {
    fn position_check(&self, _: &str, _: Reason, _: [f64; 3], _: spawn_place::Pending) -> SpawnCheck {
        SpawnCheck::Default
    }
    fn potential_spawns(&self, category: Category, pos: [i32; 3], entries: &mut Vec<SpawnEntry>) {
        self.game.potential_spawns(category, pos, entries);
    }
    // `Level.getMinY` and `getMaxY`: the level's own range (0..255 in vanilla's
    // Nether and End), not the widened column its chunks hold.
    fn min_y(&self) -> i32 {
        self.w.world.shape.min_y
    }
    fn max_y(&self) -> i32 {
        self.w.world.shape.max_y() - 1
    }
    fn ambient_light(&self) -> f32 {
        self.w.dimension_type.ambient_light
    }
    fn has_ceiling(&self) -> bool {
        self.w.world.shape.has_ceiling
    }
    fn mob_type(&self, entity: &str) -> Option<(Species, f32, f32, Category)> {
        let t = self.host.mob_types.get(entity)?;
        Some((
            t.species,
            t.width as f32,
            t.height as f32,
            Category::from_name(&t.category),
        ))
    }
    // The `natural_mob_spawns` attribute reads the zoomed biome
    // (fullResolutionBiomes), the spawn probability the noise cell.
    fn biome_spawns(&self, x: i32, y: i32, z: i32) -> Option<&HashMap<String, Vec<SpawnEntry>>> {
        self.zoomed_biome(x, y, z).map(|b| &b.spawns)
    }
    fn spawn_cost(&self, entity: &str, x: i32, y: i32, z: i32) -> Option<SpawnCost> {
        if !self.costed.contains(&entity) {
            return None;
        }
        self.zoomed_biome(x, y, z)?.spawn_costs.get(entity).copied()
    }
    fn creature_probability(&self, x: i32, y: i32, z: i32) -> f32 {
        self.biome(x, y, z).map_or(0.1, |b| b.creature_probability)
    }
    fn structures_at(
        &self,
        x: i32,
        y: i32,
        z: i32,
        wanted: &dyn Fn(&str) -> bool,
    ) -> Vec<StructureAt> {
        reaches_in(
            self.w.world,
            &|| self.host.locator.clone(),
            [x, y, z],
            wanted,
        )
        .into_iter()
        .map(|reach| StructureAt {
            id: reach.name,
            in_piece: reach.in_piece,
            in_bounds: reach.in_box,
        })
        .collect()
    }
    fn structure_spawns(&self) -> &StructureSpawns {
        self.host.structure_spawns
    }
    fn nearest_player_positions(&self) -> &[[f64; 3]] {
        &self.nearest
    }
    fn player_positions_near(&self, x: f64, z: f64, extent: f64, out: &mut Vec<[f64; 3]>) {
        if self.nearest.len() <= PLAYERS_SCANNED {
            out.extend_from_slice(&self.nearest);
            return;
        }
        let squares = self.shared_squares.unwrap_or_else(|| {
            self.player_squares
                .get_or_init(|| PlayerSquares::new(&self.nearest))
        });
        squares.near(x, z, extent, out);
    }
    fn respawn(&self) -> Option<[i32; 3]> {
        self.host.respawn
    }
    /// `ServerLevel.canSpawnEntitiesInChunk`: the whole chunk is inside the border.
    fn can_spawn_in_chunk(&self, cx: i32, cz: i32) -> bool {
        self.w.world.chunks.contains_key(&(cx, cz))
            && self.within_world_border(cx << 4, cz << 4)
            && self.within_world_border((cx << 4).wrapping_add(15), (cz << 4).wrapping_add(15))
    }
    fn no_collision(&self, bounds: Aabb) -> bool {
        let mut scene = SceneCollisions {
            world: self.w.world,
            binding: self.w.collision,
            subject: None,
        };
        // getEntityCollisions(null, box): living shulkers are solid.
        let grown = bounds.inflate(Vec3::new(1e-7, 1e-7, 1e-7));
        no_collision_empty(&mut scene, bounds)
            && !self
                .mobs_near(grown)
                .any(|(_, b, _, shulker)| shulker && b.intersects(grown))
            && !self
                .pending_ids
                .borrow()
                .iter()
                .any(|(id, b)| id == "minecraft:shulker" && b.intersects(grown))
    }
    fn unobstructed(&self, bounds: Aabb) -> bool {
        let player = self
            .players
            .iter()
            .any(|p| p.alive && !p.spectator && p.bounds.intersects(bounds));
        !player
            && !self.pending.borrow().iter().any(|b| b.intersects(bounds))
            && !self
                .mobs_near(bounds)
                .any(|(_, b, alive, _)| alive && b.intersects(bounds))
    }
    fn spawn_mobs(&self) -> bool {
        self.host.spawn_mobs
    }
    /// WORLD_SURFACE (every spawn attempt's) is the chunk's kept heightmap.
    /// The others are scanned down the column: sections of air alone are
    /// skipped (air counts for no heightmap). An unloaded column reads as air.
    fn heightmap(&self, kind: Heightmap, x: i32, z: i32) -> i32 {
        let world = self.w.world;
        // A heightmap reads at least the level's minimum Y (an empty column of a level
        // narrower than its chunks' column would read the column's bottom).
        let min_y = world.shape.min_y;
        if kind == Heightmap::WorldSurface {
            return self.w.world_surface(x, z).max(min_y);
        }
        world
            .chunks
            .get(&(x >> 4, z >> 4))
            .and_then(|chunk| {
                chunk.highest(world.air.0, |y| {
                    spawn_place::heightmap_blocks(self, kind, x, y, z)
                })
            })
            .map_or(min_y, |y| (y + 1).max(min_y))
    }
    fn pathfindable_land(&self, x: i32, y: i32, z: i32) -> bool {
        self.w
            .data
            .pathfindable(self.state(x, y, z), data::PathComputationType::Land)
    }
}
impl myriade_engine::mobs::spawner::SpawnerLevel for GameLevel<'_, '_> {
    /// `Level.hasNearbyAlivePlayer`, over every player.
    fn alive_player_near(&self, x: f64, y: f64, z: f64, range: f64) -> bool {
        self.players.iter().any(|p| p.alive_near(x, y, z, range))
    }
    fn spawner_blocks_work(&self) -> bool {
        self.game
            .session
            .rules
            .bool_at(myriade_engine::gamerules::vanilla::SPAWNER_BLOCKS_WORK)
    }
    fn entity_exists(&self, id: &str) -> bool {
        self.game.res.mob_types().contains_key(id) || self.game.entity_type_exists(id)
    }
    fn count_entities(&self, id: &str, bounds: Aabb) -> usize {
        // The ECS index over the box's circumscribed sphere, then the boxes' own test on each position.
        let ecs = &self.game.ecs;
        let (Some(w), c) = (ecs.world(), bounds.center()) else {
            return 0;
        };
        let half = |a: Axis| (bounds.max(a) - bounds.min(a)) / 2.0;
        let r = (half(Axis::X).powi(2) + half(Axis::Y).powi(2) + half(Axis::Z).powi(2)).sqrt();
        let mobs = ecs
            .select(&Select { kind: Some(id.to_owned()), tag: None, near: Some(([c.x, c.y, c.z], r)), limit: usize::MAX })
            .into_iter()
            .filter(|&e| view::position(w, e).is_some_and(|p| bounds.contains(Vec3::new(p[0], p[1], p[2]))))
            .count();
        let pending = self
            .pending_ids
            .borrow()
            .iter()
            .filter(|(k, b)| k == id && b.intersects(bounds))
            .count();
        mobs + pending
    }
}
/// Every entity type with a spawn cost in some biome, each once.
fn spawn_cost_types(biomes: &myriade_engine::biome::Biomes) -> Vec<&str> {
    let mut types: Vec<&str> = Vec::new();
    for t in biomes.list.iter().flat_map(|b| b.spawn_costs.keys()) {
        if !types.contains(&t.as_str()) {
            types.push(t);
        }
    }
    types
}
impl<'a> Game<'a> {
    /// The natural spawning policy of the level `dimension`: vanilla's until a
    /// mod sets one.
    pub(super) fn spawn_policy(&self, dimension: myriade_types::DimensionKey) -> SpawnPolicy {
        self.spawn_policies
            .get(&dimension)
            .copied()
            .unwrap_or_default()
    }
    /// What a level reads of the game besides its world view.
    pub(super) fn spawn_host(&self) -> SpawnHost<'_> {
        let data = self.finalize_spawn_data();
        SpawnHost {
            biome_zoom: BiomeZoom::new(horizon_core::biome::obfuscate_seed(self.level.seed)),
            border: self.border.geometry(),
            mob_types: self.res.mob_types(),
            structure_spawns: &self.structure_spawns,
            locator: self.end.locator(),
            // The world spawn point lives in the overworld.
            respawn: (self.dim == Dim::Overworld)
                .then_some(self.level.spawn)
                .flatten()
                .map(|s| {
                    [
                        s[0].floor() as i32,
                        s[1].floor() as i32,
                        s[2].floor() as i32,
                    ]
                }),
            spawn_mobs: self
                .session
                .rules
                .bool_at(myriade_engine::gamerules::vanilla::SPAWN_MOBS),
            spawning_monsters: self.spawning_monsters(),
            registries: data.registries,
            enchantments: data.enchantments,
            items: data.items,
            camel_husk_size: data.camel_husk_size,
        }
    }
    pub(in crate::game) fn natural_level(&self) -> GameLevel<'_, 'a> {
        let players = self.level_players();
        let nearest = players
            .iter()
            .filter(|p| !p.spectator)
            .map(|p| [p.position.x, p.position.y, p.position.z])
            .collect();
        GameLevel {
            game: self,
            host: self.spawn_host(),
            shared_boxes: None,
            shared_squares: None,
            w: self.mob_world_base(&[]),
            players,
            nearest,
            player_squares: Default::default(),
            pending: Default::default(),
            pending_ids: Default::default(),
            rides: Default::default(),
            provisional: Default::default(),
            boxes: Default::default(),
            costed: spawn_cost_types(world_biomes!(self)),
            spawner: None,
        }
    }
    /// Every player in the level in place: the local player, and the LAN
    /// guests or server players in it.
    pub(super) fn level_players(&self) -> Vec<LevelPlayer> {
        self.level_players_iter().collect()
    }
    /// `level_players` one at a time, without collecting them.
    pub(super) fn level_players_iter(&self) -> impl Iterator<Item = LevelPlayer> + '_ {
        self.local_level_player()
            .into_iter()
            .chain(self.level_guests().map(|g| LevelPlayer {
                uuid: g.uuid,
                position: Vec3::new(g.position[0], g.position[1], g.position[2]),
                bounds: super::lan_mobs::guest_bounds(g),
                alive: !g.dead,
                creative: g.creative(),
                spectator: g.spectator(),
            }))
    }
    fn local_level_player(&self) -> Option<LevelPlayer> {
        self.player_view().map(|p| LevelPlayer {
            uuid: myriade_engine::lan::playerdata::ints_uuid(p.uuid),
            position: p.position,
            bounds: p.bounds,
            alive: p.alive,
            creative: p.creative,
            spectator: p.spectator,
        })
    }
    /// The UUIDs of the players in level `dimension`, for a query that only
    /// names them: no `PlayerView`, so nothing is allocated per player. For the
    /// level in place they are `level_players`', in its order; a parked level
    /// has only its guests.
    pub(super) fn level_player_uuids<'s>(
        &'s self,
        dimension: &'s str,
    ) -> impl Iterator<Item = u128> + 's {
        let local = (dimension == self.dim.id() && self.player.is_some() && !self.lan_acting_fake)
            .then(|| myriade_engine::lan::playerdata::ints_uuid(self.acting_uuid()));
        local
            .into_iter()
            .chain(self.guests_placed_in(dimension).map(|g| g.uuid))
    }
    /// The guests in the level in place, past the acting one: `player_view`
    /// already reads that guest's live body, and its record lags behind it.
    fn level_guests(&self) -> impl Iterator<Item = &myriade_engine::lan::guest::Guest> + '_ {
        self.lan
            .iter()
            .flat_map(|h| self.guests_placed_in(&h.hub.config.dimension))
    }
    /// The guests placed in level `dimension`, past the acting one.
    fn guests_placed_in<'s>(
        &'s self,
        dimension: &'s str,
    ) -> impl Iterator<Item = &'s myriade_engine::lan::guest::Guest> + 's {
        self.lan.iter().flat_map(move |h| {
            h.hub
                .guests
                .iter()
                .filter(move |g| g.placed_in(dimension) && self.lan_acting != Some(g.conn))
        })
    }
    /// `Level.hasNearbyAlivePlayer` before a level is built for the spawn
    /// code: what a spawner asks first, every tick.
    pub(super) fn alive_player_near(&self, x: f64, y: f64, z: f64, range: f64) -> bool {
        if self
            .local_level_player()
            .is_some_and(|p| p.alive_near(x, y, z, range))
        {
            return true;
        }
        self.level_guests().any(|g| {
            let [px, py, pz] = g.position;
            !g.dead
                && !g.spectator()
                && (range < 0.0
                    || (px - x).powi(2) + (py - y).powi(2) + (pz - z).powi(2) < range * range)
        })
    }
    /// `ChunkMap.collectSpawningChunks`: the natural spawn chunk tracker's
    /// chunks (within 8 chunks of a player, `getSpawnCandidateChunks`) that
    /// are ticking and within 128 blocks of a non-spectator. Also returns how many chunks the tracker
    /// holds (`getNaturalSpawnChunkCount`).
    pub(super) fn spawning_chunks(
        &self,
        players: &[LevelPlayer],
        scratch: &mut SpawnScratch,
    ) -> i32 {
        let levels = self.tickets.levels();
        scratch.collect(
            players,
            |_| false,
            |(cx, cz)| levels.loaded_ticking(cx, cz) && super::ticking_chunk(&self.world, cx, cz),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_core::{nbt::Tag, random::RandomSource};

    /// `Level.hasNearbyAlivePlayer` as spawners ask it, against the same
    /// test on every player's full record (`level_players`).
    #[test]
    #[ignore = "daily"]
    fn alive_player_near_tests_guests_where_they_stand() {
        use myriade_engine::lan::{Joined, NetConfig, PlayerSender, guest::Guest, hub::HubConfig};
        use horizon_protocol::packets::configuration::ClientInformation;
        use std::sync::{Arc, atomic::AtomicI32, mpsc};
        let mut resources = None;
        let mut game = crate::game::game_tests::tests::server_game(&mut resources);
        let net = NetConfig {
            announce: false,
            ..NetConfig::lan(0, "spawner-players-test".into(), 8)
        };
        game.lan_host(
            super::super::lan::LanSettings {
                port: 0,
                guest_commands: true,
                force_game_mode: false,
            },
            net,
            HubConfig {
                default_game_mode: 0,
                guest_commands: true,
                force_game_mode: false,
                hardcore: false,
                view_distance: 2,
                simulation_distance: 2,
                dimension: Dim::Overworld.id().into(),
                allow_flight: false,
                seed: 0,
                max_players: 8,
                levels: Default::default(),
                level_seeds: Default::default(),
            },
        )
        .unwrap();
        // (x, dead, spectator): alive at 20, dead at 5, a spectator at 2.
        for (conn, (x, dead, spectator)) in
            [(20.5, false, false), (5.5, true, false), (2.5, false, true)]
                .into_iter()
                .enumerate()
        {
            let conn = conn as u32 + 2;
            let (tx, _rx) = mpsc::channel();
            let mut guest = Guest::new(
                Joined {
                    conn,
                    uuid: u128::from(conn),
                    name: format!("Near{conn}"),
                    properties: Vec::new(),
                    info: ClientInformation::new(2),
                    brand: "test".into(),
                    sender: PlayerSender::new(tx, Arc::default()),
                    latency: Arc::new(AtomicI32::new(0)),
                    address: "127.0.0.1:25565".parse().unwrap(),
                    expected: false,
                },
                conn as i32,
                [x, 64.0, 0.5],
                1,
            );
            guest.placed = true;
            guest.loaded = true;
            guest.dead = dead;
            if spectator {
                guest.game_mode = 3;
            }
            game.lan.as_mut().unwrap().hub.guests.push(guest);
        }
        let full = |game: &Game, x: f64, range: f64| {
            game.level_players()
                .iter()
                .any(|p| p.alive_near(x, 64.0, 0.5, range))
        };
        for (x, range, near) in [
            (0.5, 16.0, false),   // only the dead and the spectator in range
            (5.5, 16.0, true),    // the live one 15 away
            (4.4, 16.0, false),   // 16.1 away
            (4.5, 16.0, false),   // 16 away: not below the range
            (-500.0, -1.0, true), // any distance
            (36.4, 16.0, true),
        ] {
            assert_eq!(
                game.alive_player_near(x, 64.0, 0.5, range),
                near,
                "{x} {range}"
            );
            assert_eq!(full(&game, x, range), near, "{x} {range}");
        }
    }

    /// An End chunk read back from a save holds the table's first biome (badlands) in the
    /// sections its widened column adds below Y 0 and above 255. Natural spawning reads the
    /// level's own range (`getMinY` 0, `getMaxY` 255) and biomes clamped to it
    /// (`getNoiseBiome`): next to a floor at Y 0 every monster list is the End's, and an
    /// empty column's surface is the floor (#15880).
    #[test]
    #[ignore = "daily"]
    fn end_spawning_reads_the_level_range_not_the_widened_column() {
        use myriade_engine::chunk::{Chunk, LevelShape};
        use std::sync::Arc;
        let mut resources = None;
        let mut game = crate::game::game_tests::tests::server_game(&mut resources);
        game.dim = Dim::End;
        game.world.shape = LevelShape::END;
        let end = world_biomes!(game).id("minecraft:the_end");
        // The column runs -64..320: quart rows 16..80 are the level's 0..256. The zoom
        // reaches into the neighbours.
        for (cx, cz) in (-1..=1).flat_map(|x| (-1..=1).map(move |z| (x, z))) {
            let mut chunk = Chunk::with_layout(cx, cz, LevelShape::END.column());
            for qy in 16..80 {
                for qz in 0..4 {
                    for qx in 0..4 {
                        chunk.set_biome_cell(qx, qy, qz, end);
                    }
                }
            }
            game.world.chunks.insert((cx, cz), Arc::new(chunk));
        }
        let level = game.natural_level();
        assert_eq!((level.min_y(), level.max_y()), (0, 255));
        assert_eq!(level.heightmap(Heightmap::WorldSurface, 3, 3), 0);
        assert_eq!(level.heightmap(Heightmap::MotionBlocking, 3, 3), 0);
    }

    #[test]
    #[ignore = "daily"]
    fn authoritative_world_border_gates_positions_and_whole_chunks() {
        use horizon_core::position::BlockPos;
        use horizon_world::level::LevelAccess;
        crate::game::native_entity_refs::tests::with_game(|game| {
            game.world.chunks.insert(
                (0, 0),
                std::sync::Arc::new(myriade_engine::chunk::Chunk::empty(0, 0)),
            );
            game.border.set_center([8.0, 8.0]);
            game.border.set_size(16.0);
            assert!(game.natural_level().can_spawn_in_chunk(0, 0));
            assert!(!game.natural_level().can_spawn_in_chunk(1, 0)); // unloaded
            game.border.set_size(15.5);
            assert!(!game.natural_level().can_spawn_in_chunk(0, 0)); // partial
            assert!(game.natural_level().within_world_border(1, 1));
            assert!(!game.natural_level().within_world_border(0, 0));
            game.with_level(|_, level, _| {
                assert!(level.within_world_border(BlockPos::new(1, 64, 1)));
                assert!(!level.within_world_border(BlockPos::new(0, 64, 0)));
            });
            game.border.set_size(16.5);
            assert!(game.natural_level().can_spawn_in_chunk(0, 0));
            assert!(game.mob_world_base(&[]).within_world_border(0, 0));
            assert!(!game.mob_world_base(&[]).within_world_border(17, 0));
        });
    }

    fn player(x: f64, y: f64, z: f64, spectator: bool) -> LevelPlayer {
        LevelPlayer {
            uuid: 0,
            position: Vec3::new(x, y, z),
            bounds: Aabb::new(Vec3::new(x, y, z), Vec3::new(x, y, z)),
            alive: true,
            creative: false,
            spectator,
        }
    }

    /// A chunk in a player's spawning area but past 128 blocks is a spawning
    /// chunk only while a ticket forces it; a chunk outside the area never is.
    #[test]
    fn a_forcing_ticket_makes_a_far_chunk_in_the_area_a_spawning_chunk() {
        let players = [player(8.0, 64.0, 8.0, false)];
        let mut scratch = SpawnScratch::default();
        let corner = (8, 8);
        let count = scratch.collect(&players, |_| false, |_| true);
        assert_eq!(count, 289);
        assert!(
            !scratch.candidates.contains(&corner),
            "no player within 128 blocks"
        );
        assert!(scratch.candidates.contains(&(1, 1)));
        scratch.collect(&players, |c| c == corner || c == (30, 30), |_| true);
        assert!(scratch.candidates.contains(&corner));
        assert!(
            !scratch.candidates.contains(&(-8, -8)),
            "only the held chunk"
        );
        assert!(!scratch.candidates.contains(&(30, 30)), "outside the area");
    }

    /// The candidates as the uncached port collected them every tick: each
    /// player's square in dx/dz order into one ordered set, close to any
    /// player that holds the chunk. Returns the set's size too.
    fn uncached_candidates(players: &[LevelPlayer]) -> (i32, Vec<(i32, i32)>) {
        let mut order = Vec::new();
        let mut close = Vec::new();
        let mut seen = HashMap::new();
        for p in players {
            let (pcx, pcz) = SpawnScratch::chunk(p);
            for dx in -8..=8 {
                for dz in -8..=8 {
                    let key = (pcx + dx, pcz + dz);
                    let i = *seen.entry(key).or_insert_with(|| {
                        order.push(key);
                        close.push(false);
                        order.len() - 1
                    });
                    close[i] |= close_enough_for_spawning(p, key);
                }
            }
        }
        let kept = order.iter().zip(&close).filter(|c| *c.1).map(|c| *c.0);
        (order.len() as i32, kept.collect())
    }

    #[test]
    fn spawn_candidates_keep_order_and_live_proximity_across_cache_hits() {
        let mut players = vec![
            player(0.0, 64.0, 0.0, false),
            player(16.0, 64.0, 0.0, false),
        ];
        let mut scratch = SpawnScratch::default();
        // Two adjacent 17x17 squares: the first square, then the new east edge.
        assert_eq!(scratch.collect(&players, |_| false, |_| true), 306);
        assert!(scratch.candidates.contains(&(-8, 0)));
        // Spawning consumes the working list; the next pass must not care.
        scratch.candidates.reverse();
        scratch.candidates.truncate(3);
        // Moving inside its chunk takes player 0 out of range of the west edge.
        players[0].position.x = 15.5;
        assert_eq!(scratch.collect(&players, |_| false, |_| true), 306);
        assert!(!scratch.candidates.contains(&(-8, 0)));
        assert_eq!(scratch.candidates, uncached_candidates(&players).1);
        // Nothing is close to spectators, though the squares still count.
        for p in &mut players {
            p.spectator = true;
        }
        assert_eq!(scratch.collect(&players, |_| false, |_| true), 306);
        assert!(scratch.candidates.is_empty());
        for p in &mut players {
            p.spectator = false;
        }
        // Player 1 near its chunk's east side reaches the middle of chunk 9.
        players[1].position.x = 31.9;
        assert_eq!(scratch.collect(&players, |_| false, |_| true), 306);
        assert!(scratch.candidates.contains(&(9, 0)));
        assert_eq!(scratch.candidates, uncached_candidates(&players).1);
        // `keep` filters without reordering.
        scratch.collect(&players, |_| false, |(x, z)| x == z);
        let diagonal: Vec<_> = uncached_candidates(&players)
            .1
            .into_iter()
            .filter(|&(x, z)| x == z)
            .collect();
        assert!(diagonal.len() > 1);
        assert_eq!(scratch.candidates, diagonal);
    }

    /// Players walking, crossing borders, meeting, splitting, joining, leaving
    /// and swapping order: every pass matches the uncached collection.
    #[test]
    #[ignore = "daily"]
    fn spawn_candidates_match_the_uncached_union_while_players_move() {
        let mut seed = 0x5DEECE66Du64;
        let mut next = move |bound: u64| {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 33) % bound
        };
        let mut players: Vec<LevelPlayer> = [0.0, 40.0, 300.0, 1200.0, -500.0]
            .iter()
            .map(|&x| player(x, 64.0, x / 2.0, false))
            .collect();
        let mut scratch = SpawnScratch::default();
        for step in 0..3000 {
            match next(40) {
                0 if players.len() < 8 => {
                    let x = next(1200) as f64 - 600.0;
                    let at = next(players.len() as u64 + 1) as usize;
                    players.insert(at, player(x, 64.0, -x, false));
                }
                1 if players.len() > 1 => {
                    players.remove(next(players.len() as u64) as usize);
                }
                2 if players.len() > 1 => {
                    let i = next(players.len() as u64 - 1) as usize;
                    players.swap(i, i + 1);
                }
                3 => {
                    let i = next(players.len() as u64) as usize;
                    players[i].spectator = !players[i].spectator;
                }
                4 => {
                    // Teleport onto another player: identical squares.
                    let (i, j) = (next(players.len() as u64), next(players.len() as u64));
                    players[i as usize].position = players[j as usize].position;
                }
                _ => {
                    for p in &mut players {
                        p.position.x += next(13) as f64 - 6.0 + 0.37;
                        p.position.z += next(13) as f64 - 6.0 - 0.21;
                    }
                }
            }
            let (count, expected) = uncached_candidates(&players);
            assert_eq!(
                scratch.collect(&players, |_| false, |_| true),
                count,
                "step {step}"
            );
            assert_eq!(scratch.candidates, expected, "step {step}");
        }
    }
}
