//! `NaturalSpawner` by event. Horizon runs, every tick, a pass over the chunks of the players: it counts every mob again
//! (`createState`: the cost follows the creatures), takes the categories under their cap and tries each chunk once per category (a random
//! column, a random height, groups of up to four around it). Here:
//!  - the counts are kept by EVENT: a `Counted` component (category, chunk) whose hooks add and remove the creature from the count of its
//!    category, of its chunk and of the players near it, so birth, death, going away (`despawn.rs`), persistence (`despawn::keep`) and a
//!    move to another chunk (read from the index's column changes) keep them with nothing recounted;
//!  - the pass is ONE system: with every category at its cap (or the creatures' turn not come: every 400 ticks) it does nothing; else it
//!    tries the chunks within 128 blocks of a player, the cost following their number and not the creatures';
//!  - what a species needs to be born is its configuration (`SpawnDef` in `species/<name>.rs`): the category, where it stands (placement),
//!    the predicate (dark, bright above a block tag, a bat's cave...), the group limit. The spawn lists by biome (weights, group sizes) are
//!    the data of the biome (`Table`, loaded from the game's biomes), not of the species. No species name is in this file.
//! What depends on the bridge: the biome at a position (one biome here), the structures that override the lists (fortress...), the spawn
//! charge (soul sand valley...), the sky light and the block flags (read from the real world of the prototype, the bridge supplies the game's).
use crate::cow::{box_at, Clock, Index, Level, SpeciesId, SpeciesTable};
use crate::despawn::DespawnDef;
use bevy_ecs::prelude::*;
use bevy_ecs::{lifecycle::HookContext, world::DeferredWorld};
use horizon_client_lib::mobs::spawn_rules::{is_dark_enough, SpawnLevel};
use horizon_core::geometry::Vec3;
use horizon_core::random::{LegacyRandom, RandomSource};
use std::collections::HashMap;

/// `MobCategory`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    Monster = 0,
    Creature,
    Ambient,
    Axolotls,
    UndergroundWaterCreature,
    WaterCreature,
    WaterAmbient,
    Misc,
}
impl Category {
    pub const SPAWNING: [Category; 7] = [Self::Monster, Self::Creature, Self::Ambient, Self::Axolotls, Self::UndergroundWaterCreature, Self::WaterCreature, Self::WaterAmbient];
    pub fn from_name(n: &str) -> Option<Self> {
        Some(match n {
            "monster" => Self::Monster,
            "creature" => Self::Creature,
            "ambient" => Self::Ambient,
            "axolotls" => Self::Axolotls,
            "underground_water_creature" => Self::UndergroundWaterCreature,
            "water_creature" => Self::WaterCreature,
            "water_ambient" => Self::WaterAmbient,
            "misc" => Self::Misc,
            _ => return None,
        })
    }
    pub fn name(self) -> &'static str {
        ["monster", "creature", "ambient", "axolotls", "underground_water_creature", "water_creature", "water_ambient", "misc"][self as usize]
    }
    /// `getMaxInstancesPerChunk` (the vanilla limits).
    pub fn max(self) -> i32 {
        [70, 10, 15, 5, 5, 5, 20, -1][self as usize]
    }
    /// Spawned only every 400 ticks.
    pub fn persistent(self) -> bool {
        matches!(self, Self::Creature | Self::Misc)
    }
}
/// Where the creature stands (`SpawnPlacementTypes`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    /// A block valid for spawning below, two free blocks.
    OnGround,
    InWater,
    InLava,
    Anywhere,
}
/// The predicate of the type (`SpawnPlacements.Data.predicate`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rule {
    Any,
    /// `Mob.checkMobSpawnRules`.
    Ground,
    /// `Monster.checkMonsterSpawnRules`: dark enough, then `Ground`.
    Dark,
    /// `Animal.checkAnimalSpawnRules`: the block below has the tag, and the light is above 8.
    Bright { on: &'static str },
    /// `Slime.checkSlimeSpawnRules` (the slime chunks, the swamp).
    Slime,
    /// `WaterAnimal.checkSurfaceWaterAnimalSpawnRules`: within 13 blocks under the sea level, water below and above.
    SurfaceWater,
    /// `GlowSquid.checkSpawnRules`: deep in the water, in the dark.
    GlowSquid,
    /// `Drowned.checkDrownedSpawnRules`.
    Drowned,
    /// `Bat.checkBatSpawnRules`: under the surface, in the dark, on a block of the tag.
    Cave { on: &'static str },
}
#[derive(Clone, Copy, Debug)]
pub struct SpawnDef {
    pub category: Category,
    pub place: Place,
    pub rule: Rule,
    /// `Mob.getMaxSpawnClusterSize`.
    pub cluster: u8,
    /// `EntityType.canSpawnFarFromPlayer`.
    pub far: bool,
    /// `EntityType.isAllowedInPeaceful`.
    pub peaceful: bool,
    /// Its box may hold liquid (`checkSpawnObstruction` of the water animals, the squids, the drowned).
    pub liquid: bool,
    /// Its despawn rule (the same as the `Def`'s, here to refuse a spawn the creature would leave at once).
    pub despawn: DespawnDef,
}
impl SpawnDef {
    pub const MONSTER: SpawnDef = SpawnDef { category: Category::Monster, place: Place::OnGround, rule: Rule::Dark, cluster: 4, far: false, peaceful: false, liquid: false, despawn: DespawnDef::MOB };
    pub const ANIMAL: SpawnDef = SpawnDef { category: Category::Creature, place: Place::OnGround, rule: Rule::Bright { on: "minecraft:animals_spawnable_on" }, cluster: 4, far: true, peaceful: true, liquid: false, despawn: DespawnDef::KEPT };
}

/// One line of a biome's list of a category.
#[derive(Clone, Copy, Debug)]
pub struct Entry {
    pub species: u8,
    pub weight: i32,
    pub min: u32,
    pub max: u32,
    pub constant: bool,
}
/// The lists of the biome of the world, by category, resolved to species. The lines whose species the prototype does not have (or that
/// have no `SpawnDef`) are counted in `unported`, never silently dropped.
#[derive(Default, Clone, Debug)]
pub struct Table {
    pub lists: [Vec<Entry>; 8],
    pub unported: Vec<String>,
}
impl Table {
    pub fn from_biome(info: &horizon_client_lib::biome::BiomeInfo, species: &SpeciesTable) -> Self {
        let mut t = Table::default();
        for (cat, list) in &info.spawns {
            let Some(c) = Category::from_name(cat) else { continue };
            for e in list {
                match species.0.iter().find(|s| s.name == e.entity && spawn_def(s).is_some()) {
                    Some(s) => t.lists[c as usize].push(Entry { species: s.id, weight: e.weight as i32, min: e.min, max: e.max, constant: e.constant }),
                    None => t.unported.push(e.entity.clone()),
                }
            }
        }
        t
    }
}

/// The resources of the service, made when the spawning is turned on or a counted creature is born.
pub fn ensure(w: &mut World) {
    if !w.contains_resource::<SpawnCounts>() {
        w.insert_resource(SpawnCounts::default());
        w.insert_resource(DirtyColumns::default());
    }
}
/// The counts, by event.
#[derive(Resource, Default)]
pub struct SpawnCounts {
    pub global: [i32; 8],
    chunks: HashMap<(i32, i32), [u16; 8]>,
    /// By player id: where it stands (its chunk) and what is counted near it.
    players: Vec<Option<PlayerSlot>>,
}
#[derive(Clone, Copy)]
struct PlayerSlot {
    /// Where it stood when its counts were made (`ChunkMap.getPlayersCloseForSpawning`: the centre of the chunk within 128 blocks of it).
    pos: (f64, f64),
    counts: [i32; 8],
}
fn near(p: (f64, f64), c: (i32, i32)) -> bool {
    let (x, z) = (f64::from(c.0 * 16 + 8) - p.0, f64::from(c.1 * 16 + 8) - p.1);
    x * x + z * z < 16384.0
}
impl SpawnCounts {
    /// What is counted near a player (the creatures in the chunks whose centre is within 128 blocks of it).
    pub fn near_player(&self, id: u32) -> Option<[i32; 8]> {
        self.players.get(id as usize).copied().flatten().map(|s| s.counts)
    }
    fn add(&mut self, cat: Category, chunk: (i32, i32), delta: i32) {
        let k = cat as usize;
        self.global[k] += delta;
        let e = self.chunks.entry(chunk).or_insert([0; 8]);
        e[k] = (i32::from(e[k]) + delta) as u16;
        for s in self.players.iter_mut().flatten() {
            if near(s.pos, chunk) {
                s.counts[k] += delta;
            }
        }
    }
    fn recount_player(&mut self, id: usize, pos: (f64, f64)) {
        let mut counts = [0; 8];
        let chunk = chunk_of(pos.0, pos.1);
        for dx in -8..=8 {
            for dz in -8..=8 {
                if near(pos, (chunk.0 + dx, chunk.1 + dz))
                    && let Some(c) = self.chunks.get(&(chunk.0 + dx, chunk.1 + dz))
                {
                    for k in 0..8 {
                        counts[k] += i32::from(c[k]);
                    }
                }
            }
        }
        if self.players.len() <= id {
            self.players.resize(id + 1, None);
        }
        self.players[id] = Some(PlayerSlot { pos, counts });
    }
}
/// A creature the caps count (`NaturalSpawner.createState` skips the persistent ones and the other entities).
#[derive(Component, Clone, Copy, Debug)]
#[component(on_add = counted_added, on_remove = counted_removed)]
pub struct Counted {
    pub category: Category,
    pub chunk: (i32, i32),
}
fn counted_added(mut w: DeferredWorld, ctx: HookContext) {
    let c = *w.get::<Counted>(ctx.entity).unwrap();
    w.resource_mut::<SpawnCounts>().add(c.category, c.chunk, 1);
}
fn counted_removed(mut w: DeferredWorld, ctx: HookContext) {
    let c = *w.get::<Counted>(ctx.entity).unwrap();
    w.resource_mut::<SpawnCounts>().add(c.category, c.chunk, -1);
}
pub fn chunk_of(x: f64, z: f64) -> (i32, i32) {
    ((x.floor() as i32) >> 4, (z.floor() as i32) >> 4)
}
/// The caps no longer count it (it became persistent).
pub fn uncount(w: &mut World, e: Entity) {
    w.entity_mut(e).remove::<Counted>();
}
/// The caps count it again (the species decides the category).
pub fn recount(w: &mut World, e: Entity) {
    let Some(id) = w.get::<SpeciesId>(e).map(|s| s.0) else { return };
    let Some(def) = spawn_def(&w.resource::<SpeciesTable>().0[id as usize]) else { return };
    let Some(p) = w.get::<crate::cow::Pos>(e).map(|p| p.0) else { return };
    ensure(w);
    w.entity_mut(e).insert(Counted { category: def.category, chunk: chunk_of(p.x, p.z) });
}

/// The spawn rules of a species (its configuration; the species table does not carry them: it is copied by the hot paths).
pub fn spawn_def(sp: &crate::cow::Species) -> Option<&'static SpawnDef> {
    crate::species::registry().by_name(sp.name).ok()?.spawn
}
/// What the pass keeps between ticks.
#[derive(Resource)]
pub struct Spawner {
    pub table: Table,
    pub rng: LegacyRandom,
    pub on: bool,
    /// The union of the 17 x 17 squares of the players (each chunk once) and its size, rebuilt when a player changes chunk.
    chunks: Vec<(i32, i32)>,
    union: i32,
    /// Each player's block when the candidates were made.
    keys: Vec<(u32, i32, i32)>,
    /// The candidates (the chunks within 128 blocks of a player, in the order of the union) and the largest chance of a start reaching a
    /// position that is not a block of redstone conduction, among them.
    cand: Vec<(i32, i32)>,
    pmax: f64,
    reach: horizon_core::fast_hash::FxHashMap<(i32, i32), Reach>,
    /// The chunks whose `Reach` is being made, a few columns a tick.
    pending: std::collections::VecDeque<(i32, i32)>,
    /// Columns scanned per tick once some chunk is ready (a chunk is 256), and before any is (a player just came).
    pub budget: u16,
    pub first_budget: u16,
    /// Instructions of the last cycles: the candidates made again, the chunks scanned, the rest (set `profile` to read them).
    pub profile: Option<crate::instr::Counter>,
    pub laps: [u64; 3],
    /// Scans the chunks but spawns nothing (the tests that warm the chances up).
    pub hold: bool,
    /// With `profile`: the sum of the chances of the chunks within 128 blocks, per cycle.
    pub expected: f64,
    shared: std::sync::Arc<(SpeciesTable, [Vec<Entry>; 8], Vec<Option<&'static SpawnDef>>)>,
    /// The biome of the world (one here; the bridge answers by position).
    pub biome: String,
    pub stats: SpawnStats,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct SpawnStats {
    /// Ticks the pass found nothing to do, ticks it ran, chunks tried, creatures born.
    pub idle: u64,
    pub cycles: u64,
    pub chunks: u64,
    pub starts: u64,
    pub attempts: u64,
    pub born: u64,
    pub rebuilds: u64,
    /// By category: the local cap refused the chunk, starts, creatures born.
    pub by_category: [[u64; 3]; 8],
}
impl Spawner {
    /// The chance that a start in this chunk reaches a position that is not a block of redstone conduction (once the chunk was a candidate).
    pub fn reach_chance(&self, chunk: (i32, i32)) -> Option<f64> {
        self.reach.get(&chunk).filter(|r| r.ready()).map(|r| r.p)
    }
    /// A new random stream and new statistics; the chances of the chunks stay.
    pub fn reseed(&mut self, seed: i64) {
        self.rng = LegacyRandom::new(seed);
        self.stats = SpawnStats::default();
    }
    pub fn new(table: Table, seed: i64) -> Self {
        let st = crate::cow::species_table();
        let defs = st.0.iter().map(spawn_def).collect();
        let shared = std::sync::Arc::new((st, table.lists.clone(), defs));
        Self { table, rng: LegacyRandom::new(seed), on: true, chunks: Vec::new(), union: 0, keys: Vec::new(), cand: Vec::new(), pmax: 0.0, reach: Default::default(), pending: Default::default(), budget: 64, first_budget: 512, biome: String::new(), hold: false, expected: 0.0, profile: None, laps: [0; 3], shared, stats: SpawnStats::default() }
    }
}

/// What a chunk offers to a start: for each of its 256 columns the world surface and how many heights from just above the bottom of the
/// world up to the surface are not a block of redstone conduction (a start in one of those ends at once, as `spawnCategoryForPosition`
/// does). A start draws a column, then a height of `min_y..=top`: it reaches a free position with the chance `p` (the mean over the columns of
/// free / (top - min_y + 1)), and, given that it does, in a column with the weight free / height at a free height of it.
pub struct Reach {
    top: [i16; 256],
    open: [u16; 256],
    pub p: f64,
    /// The columns scanned so far (256: ready). A chunk that is not ready is not drawn from.
    done: u16,
    /// The sections that hold only one block of redstone conduction: nothing free in them, not read.
    solid: u32,
    /// `solid` was read (the first scan does it: it costs as much as a few columns).
    started: bool,
}
const MIN_Y: i32 = -64;
fn conductor(mw: &horizon_client_lib::mobs::MobWorld, id: u32) -> bool {
    mw.mob_data().state(horizon_world::registry::BlockStateId(id)).flags & horizon_client_lib::mobs::data::REDSTONE_CONDUCTOR != 0
}
/// The sections of the chunk that are one conducting block through and through.
fn solid_sections(mw: &horizon_client_lib::mobs::MobWorld, chunk: &horizon_client_render::chunk::Chunk) -> u32 {
    let base = chunk.min_y();
    (0..chunk.section_count().min(32)).filter(|&s| {
        let v = chunk.get(0, base + 16 * s as i32, 0);
        chunk.section_is_uniform(s, v) && conductor(mw, v)
    }).fold(0, |m, s| m | 1 << s)
}
/// One column: its surface and its free heights.
fn scan_column(mw: &horizon_client_lib::mobs::MobWorld, chunk: &horizon_client_render::chunk::Chunk, solid: u32, cx: i32, cz: i32, lx: i32, lz: i32) -> (i16, u16) {
    let top = SpawnLevel::world_surface(mw, cx * 16 + lx, cz * 16 + lz);
    let base = chunk.min_y();
    let mut open = 0;
    for s in 0..chunk.section_count().min(32) {
        let lo = (base + 16 * s as i32).max(MIN_Y + 1);
        let hi = (base + 16 * s as i32 + 15).min(top);
        if solid >> s & 1 == 1 || lo > hi {
            continue;
        }
        open += (lo..=hi).filter(|&y| !conductor(mw, chunk.get(lx, y, lz))).count();
    }
    (top as i16, open as u16)
}
impl Reach {
    fn new(mw: &horizon_client_lib::mobs::MobWorld, cx: i32, cz: i32) -> Option<Self> {
        mw.world.chunks.get(&(cx, cz))?;
        Some(Reach { top: [0; 256], open: [0; 256], p: 0.0, done: 0, solid: u32::MAX, started: false })
    }
    pub fn ready(&self) -> bool {
        self.done >= 256
    }
    /// Scans up to `budget` more columns; true once the chunk is ready.
    fn advance(&mut self, mw: &horizon_client_lib::mobs::MobWorld, cx: i32, cz: i32, budget: u16) -> bool {
        let Some(chunk) = mw.world.chunks.get(&(cx, cz)).cloned() else { return false };
        if !self.started {
            // The sections read first, against the whole budget of the tick.
            self.solid = solid_sections(mw, &chunk);
            self.started = true;
            return false;
        }
        let end = (self.done + budget).min(256);
        for i in self.done..end {
            let (t, o) = scan_column(mw, &chunk, self.solid, cx, cz, i as i32 / 16, i as i32 % 16);
            self.top[i as usize] = t;
            self.open[i as usize] = o;
        }
        self.done = end;
        if self.ready() {
            self.refresh();
        }
        self.ready()
    }
    fn refresh(&mut self) {
        self.p = (0..256).map(|i| f64::from(self.open[i]) / f64::from(i32::from(self.top[i]) - MIN_Y + 1)).sum::<f64>() / 256.0;
    }
    /// A start that reached a free position: its column and height.
    fn sample(&self, mw: &horizon_client_lib::mobs::MobWorld, cx: i32, cz: i32, r: &mut LegacyRandom) -> [i32; 3] {
        let mut t = r.next_double() * self.p * 256.0;
        let mut col = 255;
        for i in 0..256 {
            let w = f64::from(self.open[i]) / f64::from(i32::from(self.top[i]) - MIN_Y + 1);
            if t < w {
                col = i;
                break;
            }
            t -= w;
        }
        let (lx, lz) = ((col / 16) as i32, (col % 16) as i32);
        let mut k = r.next_int_bound(i32::from(self.open[col]).max(1)).unwrap_or(0);
        let chunk = &mw.world.chunks[&(cx, cz)];
        let mut y = i32::from(self.top[col]);
        for yy in MIN_Y + 1..=i32::from(self.top[col]) {
            if !conductor(mw, chunk.get(lx, yy, lz)) {
                if k == 0 {
                    y = yy;
                    break;
                }
                k -= 1;
            }
        }
        [cx * 16 + lx, y, cz * 16 + lz]
    }
}
/// The columns whose blocks changed since the last pass.
#[derive(Resource, Default)]
pub struct DirtyColumns(pub Vec<(i32, i32)>);

fn pick<'a>(list: &'a [Entry], r: &mut LegacyRandom) -> Option<&'a Entry> {
    let total: i64 = list.iter().map(|e| i64::from(e.weight)).sum();
    if total <= 0 {
        return None;
    }
    let mut s = i64::from(r.next_int_bound(total as i32).ok()?);
    for e in list {
        s -= i64::from(e.weight);
        if s < 0 {
            return Some(e);
        }
    }
    None
}
fn flags(mw: &horizon_client_lib::mobs::MobWorld, x: i32, y: i32, z: i32) -> u16 {
    mw.mob_data().state(SpawnLevel::state(mw, x, y, z)).flags
}
fn raw_brightness(mw: &horizon_client_lib::mobs::MobWorld, x: i32, y: i32, z: i32, dark: i32) -> i32 {
    mw.block_light(x, y, z).max(mw.sky_light(x, y, z) - dark)
}
fn tag(mw: &horizon_client_lib::mobs::MobWorld, t: &str, x: i32, y: i32, z: i32) -> bool {
    mw.block_tag(t, &SpawnLevel::facts(mw).get(SpawnLevel::state(mw, x, y, z)).name)
}
fn valid_spawn(mw: &horizon_client_lib::mobs::MobWorld, x: i32, y: i32, z: i32) -> bool {
    flags(mw, x, y, z) & horizon_client_lib::mobs::data::VALID_SPAWN != 0
}
fn valid_empty(mw: &horizon_client_lib::mobs::MobWorld, x: i32, y: i32, z: i32) -> bool {
    flags(mw, x, y, z) & horizon_client_lib::mobs::data::VALID_EMPTY_SPAWN != 0
}
/// `SpawnPlacements.isSpawnPositionOk`.
fn place_ok(mw: &horizon_client_lib::mobs::MobWorld, d: &SpawnDef, x: i32, y: i32, z: i32) -> bool {
    if d.place != Place::Anywhere && !mw.within_world_border(x, z) {
        return false;
    }
    let fluid = |yy| SpawnLevel::facts(mw).get(SpawnLevel::state(mw, x, yy, z)).fluid;
    match d.place {
        Place::OnGround => valid_spawn(mw, x, y - 1, z) && valid_empty(mw, x, y, z) && valid_empty(mw, x, y + 1, z),
        Place::InWater => fluid(y) == horizon_client_lib::blocks::Fluid::Water && flags(mw, x, y + 1, z) & horizon_client_lib::mobs::data::REDSTONE_CONDUCTOR == 0,
        Place::InLava => fluid(y) == horizon_client_lib::blocks::Fluid::Lava,
        Place::Anywhere => true,
    }
}
/// The predicate of the type.
fn biome_has(mw: &horizon_client_lib::mobs::MobWorld, biome: &str, tag: &str) -> bool {
    mw.data.biome_tags.get(tag.trim_start_matches('#')).is_some_and(|m| m.contains(biome))
}
fn is_water(mw: &horizon_client_lib::mobs::MobWorld, x: i32, y: i32, z: i32) -> bool {
    SpawnLevel::facts(mw).get(SpawnLevel::state(mw, x, y, z)).name.as_ref() == "minecraft:water"
}
fn fluid_is(mw: &horizon_client_lib::mobs::MobWorld, x: i32, y: i32, z: i32, f: horizon_client_lib::blocks::Fluid) -> bool {
    SpawnLevel::facts(mw).get(SpawnLevel::state(mw, x, y, z)).fluid == f
}
fn rule_ok(mw: &horizon_client_lib::mobs::MobWorld, biome: &str, d: &SpawnDef, r: &mut LegacyRandom, x: i32, y: i32, z: i32) -> bool {
    if !d.peaceful && mw.peaceful() {
        return false;
    }
    match d.rule {
        Rule::Any => true,
        Rule::Ground => valid_spawn(mw, x, y - 1, z),
        Rule::Dark => is_dark_enough(mw, r, x, y, z) && valid_spawn(mw, x, y - 1, z),
        Rule::Bright { on } => tag(mw, on, x, y - 1, z) && raw_brightness(mw, x, y, z, 0) > 8,
        Rule::Slime => {
            let slime_chunk = horizon_core::random::slime_chunk_random(x >> 4, z >> 4, mw.world_seed(), 987_234_911).next_int_bound(10).unwrap_or(1) == 0;
            let surface = biome_has(mw, biome, "minecraft:allows_surface_slime_spawns") && y > 50 && y < 70 && {
                let chance = mw.surface_slime_chance(x, y, z);
                r.next_float() < chance && raw_brightness(mw, x, y, z, mw.sky_darken()) <= r.next_int_bound(8).unwrap_or(0)
            };
            surface && valid_spawn(mw, x, y - 1, z) || (r.next_int_bound(10).unwrap_or(1) == 0 && slime_chunk && y < 40 && valid_spawn(mw, x, y - 1, z))
        }
        Rule::SurfaceWater => {
            let sea = mw.sea_level();
            y >= sea - 13 && y <= sea && fluid_is(mw, x, y - 1, z, horizon_client_lib::blocks::Fluid::Water) && is_water(mw, x, y + 1, z)
        }
        Rule::GlowSquid => y <= mw.sea_level() - 33 && raw_brightness(mw, x, y, z, 0) == 0 && is_water(mw, x, y, z),
        Rule::Drowned => {
            if !fluid_is(mw, x, y - 1, z, horizon_client_lib::blocks::Fluid::Water) {
                return false;
            }
            let can_spawn = !mw.peaceful() && is_dark_enough(mw, r, x, y, z) && fluid_is(mw, x, y, z, horizon_client_lib::blocks::Fluid::Water);
            if biome_has(mw, biome, "minecraft:more_frequent_drowned_spawns") {
                r.next_int_bound(15).unwrap_or(1) == 0 && can_spawn
            } else {
                r.next_int_bound(40).unwrap_or(1) == 0 && y < mw.sea_level() - 5 && can_spawn
            }
        }
        Rule::Cave { on } => {
            if y >= SpawnLevel::world_surface(mw, x, z) || r.next_boolean() {
                return false;
            }
            if raw_brightness(mw, x, y, z, mw.sky_darken()) > r.next_int_bound(4).unwrap_or(0) {
                return false;
            }
            tag(mw, on, x, y - 1, z) && valid_spawn(mw, x, y - 1, z)
        }
    }
}

/// The players as the pass sees them: positions.
struct Ctx<'a> {
    mw: &'static horizon_client_lib::mobs::MobWorld<'static>,
    table: &'a [Vec<Entry>; 8],
    species: &'a SpeciesTable,
    defs: &'a [Option<&'static SpawnDef>],
    biome: &'a str,
    players: &'a [[f64; 3]],
    time: u64,
}
fn nearest_sqr(players: &[[f64; 3]], x: f64, y: f64, z: f64) -> Option<f64> {
    players.iter().map(|p| (p[0] - x).powi(2) + (p[1] - y).powi(2) + (p[2] - z).powi(2)).reduce(f64::min)
}
/// Whether a creature's box meets another's (`isUnobstructed`): the index lists the creatures by column.
fn obstructed(w: &World, bounds: horizon_core::geometry::Aabb) -> bool {
    let idx = w.resource::<Index>();
    let c = bounds.min_position();
    let mut hit = false;
    idx.for_radius(c.x, c.z, 3.0, |e| {
        if !hit && let Some(b) = w.get::<crate::cow::Bounds>(e) {
            hit = b.0.intersects(bounds);
        }
    });
    hit
}
/// `spawnCategoryForPosition`: up to three groups around the start; returns the creatures born.
fn category_for_position(w: &mut World, sp: &mut Spawner, cx: &Ctx, cat: Category, chunk: (i32, i32), start: [i32; 3]) {
    let [x0, y, z0] = start;
    let mw = cx.mw;
    if flags(mw, x0, y, z0) & horizon_client_lib::mobs::data::REDSTONE_CONDUCTOR != 0 {
        return;
    }
    let mut cluster = 0u32;
    'groups: for _ in 0..3 {
        let (mut x, mut z) = (x0, z0);
        let mut current: Option<(Entry, [i32; 3])> = None;
        let mut max = (sp.rng.next_float() * 4.0).ceil() as i32;
        let mut ll = 0;
        while ll < max {
            ll += 1;
            x += sp.rng.next_int_bound(6).unwrap_or(0) - sp.rng.next_int_bound(6).unwrap_or(0);
            z += sp.rng.next_int_bound(6).unwrap_or(0) - sp.rng.next_int_bound(6).unwrap_or(0);
            let (xx, zz) = (f64::from(x) + 0.5, f64::from(z) + 0.5);
            sp.stats.attempts += 1;
            let Some(dist) = nearest_sqr(cx.players, xx, f64::from(y), zz) else { continue };
            // `isRightDistanceToPlayerAndSpawnPoint`.
            let c2 = (x >> 4, z >> 4);
            if dist <= 576.0 || !(c2 == chunk || mw.world.chunks.contains_key(&c2)) {
                continue;
            }
            if current.is_none() {
                let Some(e) = pick(&cx.table[cat as usize], &mut sp.rng).copied() else { break };
                max = if e.constant { e.min as i32 } else { (e.min + sp.rng.next_int_bound((e.max.saturating_sub(e.min) + 1) as i32).unwrap_or(0) as u32) as i32 };
                current = Some((e, [x, y, z]));
            }
            let (entry, _) = current.unwrap();
            let species = cx.species.0[entry.species as usize];
            let def = cx.defs[entry.species as usize].unwrap();
            // `isValidSpawnPostitionForType`.
            let despawn2 = f64::from(def.despawn.distance).powi(2);
            if !def.far && dist > despawn2 {
                continue;
            }
            if !place_ok(mw, &def, x, y, z) || !rule_ok(mw, cx.biome, &def, &mut sp.rng, x, y, z) {
                continue;
            }
            let half = f64::from(species.width) / 2.0;
            let probe = horizon_core::geometry::Aabb::new(Vec3::new(xx - half, f64::from(y), zz - half), Vec3::new(xx + half, f64::from(y) + f64::from(species.height), zz + half));
            if !mw.no_collision(probe) {
                continue;
            }
            let _yaw = sp.rng.next_float() * 360.0;
            // `Mob.checkSpawnRules` (the walk value is not negative) and `checkSpawnObstruction`; a creature that would go at once is refused.
            let far_gone = dist > despawn2 && def.despawn.removable == crate::despawn::Removable::Always;
            if far_gone || crate::spots::walk_target_value(mw, species.walk, [x, y, z]) < 0.0 || (!def.liquid && mw.contains_any_liquid(probe)) || obstructed(w, probe) {
                continue;
            }
            let seed = (u64::from(sp.rng.next_int() as u32) << 32) | u64::from(sp.rng.next_int() as u32);
            let t = sp.profile.as_ref().map_or(0, crate::instr::Counter::read);
            let e = born(w, &species, [xx, f64::from(y), zz], seed, dist);
            let _ = e;
            sp.laps[2] += sp.profile.as_ref().map_or(0, crate::instr::Counter::read) - t;
            sp.stats.born += 1;
            sp.stats.by_category[cat as usize][2] += 1;
            cluster += 1;
            if cluster >= u32::from(def.cluster) {
                break 'groups;
            }
        }
    }
    let _ = cx.time;
}
/// A creature of the wild is born: in the world, in the index, its timers armed, counted, and given its rule of going away.
pub fn born(w: &mut World, sp: &crate::cow::Species, at: [f64; 3], seed: u64, nearest_sqr: f64) -> Entity {
    ensure(w);
    let e = crate::cow::spawn_species(w, sp, at[0], at[2], seed);
    w.get_mut::<crate::cow::Pos>(e).unwrap().0 = Vec3::new(at[0], at[1], at[2]);
    if let Some(mut b) = w.get_mut::<crate::cow::Bounds>(e) {
        b.0 = box_at(Vec3::new(at[0], at[1], at[2]), sp.width, sp.height);
    }
    crate::goals::arm_timers(w, e);
    crate::zone::admit(w, &[e]);
    if let Some(def) = spawn_def(sp) {
        w.entity_mut(e).insert(Counted { category: def.category, chunk: chunk_of(at[0], at[2]) });
        crate::despawn::admit(w, e, def.despawn, nearest_sqr);
    }
    e
}
/// One tick: the counts follow the creatures that changed chunk and the players that did; then the pass, if any category is open.
pub fn step(world: &mut World) {
    if !world.contains_resource::<Spawner>() {
        return;
    }
    // The players that changed chunk this tick (`Online` events): their square of chunks and the counts near them.
    let changed: Vec<(u32, Option<(f64, f64)>)> = world.resource::<crate::goals::Online>().seen.iter().filter_map(|(o, n)| n.or(*o).map(|p| (p.id, n.map(|n| (n.x, n.z))))).collect();
    if !changed.is_empty() {
        world.resource_scope(|w, mut sp: Mut<Spawner>| {
            let mut c = w.resource_mut::<SpawnCounts>();
            for (id, at) in changed {
                match at {
                    Some((x, z)) => {
                        // Counted again when it crossed a block (the set of chunks within 128 blocks changes only then, to a block).
                        let moved = c.players.get(id as usize).copied().flatten().is_none_or(|s| s.pos.0.floor() != x.floor() || s.pos.1.floor() != z.floor());
                        if moved {
                            c.recount_player(id as usize, (x, z));
                        }
                    }
                    None => {
                        if let Some(s) = c.players.get_mut(id as usize) {
                            *s = None;
                        }
                    }
                }
            }
        });
    }
    // A creature that crossed into another chunk.
    let crossed: Vec<Entity> = world.resource::<Index>().crossed.iter().map(|c| c.0).collect();
    for e in crossed {
        let (Some(c), Some(p)) = (world.get::<Counted>(e).copied(), world.get::<crate::cow::Pos>(e).map(|p| p.0)) else { continue };
        let now = chunk_of(p.x, p.z);
        if now != c.chunk {
            world.resource_mut::<SpawnCounts>().add(c.category, c.chunk, -1);
            world.resource_mut::<SpawnCounts>().add(c.category, now, 1);
            world.get_mut::<Counted>(e).unwrap().chunk = now;
        }
    }
    // A player the counts do not know yet (the spawning was turned on after it came): counted near it now.
    let unknown: Vec<(u32, f64, f64)> = {
        let c = world.resource::<SpawnCounts>();
        world.resource::<crate::goals::Online>().players().filter(|p| c.near_player(p.id).is_none()).map(|p| (p.id, p.x, p.z)).collect()
    };
    for (id, x, z) in unknown {
        world.resource_mut::<SpawnCounts>().recount_player(id as usize, (x, z));
    }
    let time = world.resource::<Clock>().0;
    world.resource_scope(|w, mut sp: Mut<Spawner>| cycle(w, &mut sp, time));
}
fn cycle(w: &mut World, sp: &mut Spawner, time: u64) {
    if !sp.on {
        return;
    }
    let online: Vec<(u32, f64, f64, f64)> = w.resource::<crate::goals::Online>().players().map(|p| (p.id, p.x, p.y, p.z)).collect();
    if online.is_empty() {
        sp.stats.idle += 1;
        return;
    }
    let mw = w.resource::<Level>().mw.0;
    // The blocks that changed: their column is looked at again.
    let dirty = std::mem::take(&mut w.resource_mut::<DirtyColumns>().0);
    for (x, z) in dirty {
        let (c, lx, lz) = ((x >> 4, z >> 4), x & 15, z & 15);
        if let (Some(r), Some(chunk)) = (sp.reach.get_mut(&c), mw.world.chunks.get(&c).cloned()) {
            r.solid = solid_sections(mw, &chunk);
            let i = (lx * 16 + lz) as usize;
            // A column not scanned yet is scanned with the rest, on the new blocks.
            if (i as u16) < r.done {
                let (t, o) = scan_column(mw, &chunk, r.solid, c.0, c.1, lx, lz);
                r.top[i] = t;
                r.open[i] = o;
                if r.ready() {
                    r.refresh();
                    sp.pmax = sp.pmax.max(r.p);
                }
            }
        }
    }
    let lap = |sp: &Spawner| sp.profile.as_ref().map_or(0, crate::instr::Counter::read);
    let t0 = lap(sp);
    // The candidates: made again when a player changed chunk.
    let keys: Vec<(u32, i32, i32)> = online.iter().map(|p| (p.0, (p.1.floor() as i32) >> 4, (p.3.floor() as i32) >> 4)).collect();
    if keys != sp.keys {
        let mut seen = horizon_core::fast_hash::FxHashSet::default();
        sp.chunks.clear();
        sp.cand.clear();
        for &(_, x, z) in &keys {
            for dx in -8..=8 {
                for dz in -8..=8 {
                    let c = (x + dx, z + dz);
                    if seen.insert(c) {
                        sp.chunks.push(c);
                    }
                }
            }
        }
        sp.union = sp.chunks.len() as i32;
        sp.stats.rebuilds += 1;
        let mut pmax = 0.0f64;
        let mut fresh: Vec<(i32, i32)> = Vec::new();
        for &c in &sp.chunks.clone() {
            let (x, z) = (f64::from(c.0 * 16 + 8), f64::from(c.1 * 16 + 8));
            // A margin of 24 blocks: a player moves inside its chunk without a new list; the exact 128 blocks are asked of each pick.
            if !online.iter().any(|p| (x - p.1).powi(2) + (z - p.3).powi(2) < 152.0 * 152.0) {
                continue;
            }
            if !sp.reach.contains_key(&c) {
                match Reach::new(mw, c.0, c.1) {
                    Some(r) => {
                        sp.reach.insert(c, r);
                        fresh.push(c);
                    }
                    None => continue,
                }
            }
            if sp.reach[&c].ready() {
                pmax = pmax.max(sp.reach[&c].p);
                sp.cand.push(c);
            }
        }
        // The new chunks nearest to a player are scanned first.
        let near = |c: &(i32, i32)| online.iter().map(|p| (f64::from(c.0 * 16 + 8) - p.1).powi(2) + (f64::from(c.1 * 16 + 8) - p.3).powi(2)).fold(f64::MAX, f64::min);
        fresh.sort_by(|a, b| near(a).total_cmp(&near(b)));
        sp.pending.extend(fresh);
        sp.pmax = pmax;
        // What no candidate near any player needs is dropped.
        if sp.reach.len() > 4 * sp.chunks.len() + 64 {
            let keep: horizon_core::fast_hash::FxHashSet<_> = sp.chunks.iter().copied().collect();
            sp.reach.retain(|c, _| keep.contains(c));
            sp.pending.retain(|c| keep.contains(c));
        }
        sp.keys = keys;
    }
    let t1 = lap(sp);
    // The chunks that were not scanned: a few columns a tick, each becomes a candidate when it is ready.
    // A player who just came (a long queue, or nothing ready yet) is served fast; one who walks, steadily.
    let mut budget = if sp.cand.is_empty() { sp.first_budget } else { (sp.budget + 2 * sp.pending.len() as u16).min(5 * sp.budget / 2) };
    while budget > 0 {
        let Some(&c) = sp.pending.front() else { break };
        let Some(r) = sp.reach.get_mut(&c) else {
            sp.pending.pop_front();
            continue;
        };
        let step = budget.min(256 - r.done.min(256));
        if step == 0 {
            sp.pending.pop_front();
            continue;
        }
        budget -= step;
        if r.advance(mw, c.0, c.1, step) {
            sp.pending.pop_front();
            sp.pmax = sp.pmax.max(sp.reach[&c].p);
            sp.cand.push(c);
        } else if !mw.world.chunks.contains_key(&c) {
            sp.pending.pop_front();
        }
    }
    let t2 = lap(sp);
    sp.laps = [t1 - t0, t2 - t1, 0];
    if sp.hold {
        return;
    }
    // The categories under their cap (the counts are kept, not recounted).
    let open: Vec<Category> = {
        let c = w.resource::<SpawnCounts>();
        Category::SPAWNING.into_iter().filter(|k| !sp.table.lists[*k as usize].is_empty() && (!k.persistent() || time % 400 == 0) && i64::from(c.global[*k as usize]) < i64::from(k.max()) * i64::from(sp.union) / 289).collect()
    };
    if open.is_empty() || sp.cand.is_empty() || sp.pmax <= 0.0 {
        sp.stats.idle += 1;
        return;
    }
    sp.stats.cycles += 1;
    // The starts that reach a free position, without a draw for the others: over the candidates, in order, each category draws its
    // successes by jumps of the geometric law of the largest chance, keeping a candidate with the ratio of its own chance (thinning).
    let n = sp.cand.len() as i64;
    if sp.profile.is_some() {
        sp.expected += sp.cand.iter().filter(|c| online.iter().any(|p| (f64::from(c.0 * 16 + 8) - p.1).powi(2) + (f64::from(c.1 * 16 + 8) - p.3).powi(2) < 16384.0)).map(|c| sp.reach[c].p).sum::<f64>();
    }
    let q = sp.pmax.min(1.0 - 1e-9);
    let mut picked: Vec<(usize, Category)> = Vec::new();
    for &cat in &open {
        let mut i: i64 = -1;
        loop {
            let u = sp.rng.next_double().max(1e-12);
            i += 1 + if q > 0.0 { (u.ln() / (1.0 - q).ln()).floor() as i64 } else { n };
            if i >= n {
                break;
            }
            let p = sp.reach[&sp.cand[i as usize]].p;
            if sp.rng.next_double() * sp.pmax < p {
                picked.push((i as usize, cat));
            }
        }
    }
    if picked.is_empty() {
        return;
    }
    // Visited chunk by chunk in a random order, the categories in theirs.
    let salt = sp.rng.next_long() as u64;
    picked.sort_by_key(|&(i, cat)| ((i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ salt, cat as usize));
    let shared = sp.shared.clone();
    let (species, table, defs) = (&shared.0, &shared.1, &shared.2[..]);
    let players: Vec<[f64; 3]> = online.iter().map(|p| [p.1, p.2, p.3]).collect();
    let biome = sp.biome.clone();
    let cx = Ctx { mw, table, species, defs, biome: &biome, players: &players, time };
    for (i, cat) in picked {
        let chunk = sp.cand[i];
        {
            let (x, z) = (f64::from(chunk.0 * 16 + 8), f64::from(chunk.1 * 16 + 8));
            if !online.iter().any(|p| (x - p.1).powi(2) + (z - p.3).powi(2) < 16384.0) {
                continue;
            }
        }
        sp.stats.chunks += 1;
        // `canSpawnForCategoryLocal`: a player near the chunk whose count is under the cap.
        let ok = {
            let c = w.resource::<SpawnCounts>();
            let (x, z) = (f64::from(chunk.0 * 16 + 8), f64::from(chunk.1 * 16 + 8));
            online.iter().any(|&(id, px, _, pz)| (x - px).powi(2) + (z - pz).powi(2) < 16384.0 && c.players.get(id as usize).copied().flatten().is_some_and(|s| s.counts[cat as usize] < cat.max()))
        };
        if !ok {
            sp.stats.by_category[cat as usize][0] += 1;
            continue;
        }
        sp.stats.starts += 1;
        sp.stats.by_category[cat as usize][1] += 1;
        let start = sp.reach[&chunk].sample(mw, chunk.0, chunk.1, &mut sp.rng);
        category_for_position(w, sp, &cx, cat, chunk, start);
    }
}
/// The worlds of the natural spawning tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Grass on dirt on stone to the bottom of the world.
    Plains = 0,
    /// The same, with a dark room in the stone (x, z within 60 blocks, y from -30 to -27) and a pond in it.
    Caves,
    /// The same under the sea (water from y = 1 to the sea level, 63).
    Ocean,
}
static KIND: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
pub fn set_kind(k: Kind) {
    KIND.store(k as u8, std::sync::atomic::Ordering::Relaxed);
}
/// The world of the natural spawning tests (`set_kind`): `half` blocks each way.
pub fn world_builder(half: i32) -> impl FnOnce(&mut horizon_client_render::world::World, &horizon_world::registry::BlockRegistry) {
    move |blocks, registry| {
        use horizon_client_render::chunk::Chunk;
        let kind = KIND.load(std::sync::atomic::Ordering::Relaxed);
        let (stone, dirt, grass) = (registry.default_state("minecraft:stone").unwrap(), registry.default_state("minecraft:dirt").unwrap(), registry.default_state("minecraft:grass_block").unwrap());
        let (air, water) = (registry.default_state("minecraft:air").unwrap(), registry.default_state("minecraft:water").unwrap());
        let chunks = (half + 15) / 16 + 1;
        for cx in -chunks..chunks {
            for cz in -chunks..chunks {
                let mut c = Chunk::empty(cx, cz);
                for lx in 0..16 {
                    for lz in 0..16 {
                        for y in -64..=-2 {
                            c.set(lx, y, lz, stone.0);
                        }
                        c.set(lx, -1, lz, dirt.0);
                        c.set(lx, 0, lz, grass.0);
                        let (x, z) = (cx * 16 + lx, cz * 16 + lz);
                        if kind == Kind::Caves as u8 && x.abs() <= 60 && z.abs() <= 60 {
                            for y in -30..=-27 {
                                let pond = (10..=40).contains(&x) && (10..=40).contains(&z) && y <= -28;
                                c.set(lx, y, lz, if pond { water.0 } else { air.0 });
                            }
                        }
                        if kind == Kind::Ocean as u8 {
                            for y in 1..=63 {
                                c.set(lx, y, lz, water.0);
                            }
                        }
                    }
                }
                blocks.chunks.insert((cx, cz), std::sync::Arc::new(c));
            }
        }
    }
}
