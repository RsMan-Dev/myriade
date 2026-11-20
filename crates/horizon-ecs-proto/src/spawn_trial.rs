//! The trial spawner (`trial_spawner.rs`) by event: waiting for players, its waves while they fight, the omen (`ominous`: the
//! ominous waves, their item spawner), the reward ejection, the cooldown. One record per spawner (block data), never per mob: the
//! mobs it made are kept as a list of its own (vanilla's `currentMobs`), dropped when they leave its 47-block reach or die (`forget`).
//! The players come from `Online`; the reach box and the sight line are written here (`visual_hit` walks the cells of the segment,
//! `Env.solid` says which block stops it). The detection runs at the ticks where `(pack(pos) + time) % 20 == 0`, so the spawner
//! wakes at those ticks and at the instants its state changes. Called every tick by the caller (`run_wakes`), which also drops the
//! ominous items when their delay ends.
use crate::cow::{species_table, Body, Clock, Pos};
use crate::despawn;
use crate::goals::{Events, Online};
use crate::item::spawn_at_location;
use crate::spawn_spawner::{nearest_sqr, weighted, weighted_index, Block3, SpawnerRng, Valid};
use bevy_ecs::prelude::*;
use horizon_client_lib::items::ItemStack;
use horizon_core::geometry::Vec3;
use horizon_core::position::BlockPos;
use horizon_core::random::{LegacyRandom, RandomSource};
use std::collections::{BTreeMap, HashMap};

/// `MAX_MOB_TRACKING_DISTANCE`: a mob further than this (in blocks) is no longer one of the spawner's.
const TRACK: i64 = 47;
/// `DELAY_BEFORE_EJECT_AFTER_KILLING_LAST_MOB` and `TIME_BETWEEN_EACH_EJECTION`, in ticks.
const DELAY_BEFORE_EJECT: u64 = 40;
const EJECT_EVERY: u64 = 30;
/// The delay before a wave once a player is detected (`try_detect_players`).
const FIRST_WAVE_DELAY: u64 = 40;
/// `TICKS_BETWEEN_ITEM_SPAWNERS`: also the gap an ominous item spawner waits after each item, and the cooldown an omen sets.
const ITEM_GAP: u64 = 160;
/// The height of a player (standing) for the ominous item's position: the eye is at 1.62.
const PLAYER_HEIGHT: f64 = 1.8;

/// The numbers of one wave: the normal one or the ominous one (`TrialConfig`).
#[derive(Clone, Debug, PartialEq)]
pub struct Wave {
    pub total: f32,
    pub simultaneous: f32,
    pub total_per_player: f32,
    pub simultaneous_per_player: f32,
    pub ticks_between_spawn: u64,
    /// The species the waves draw from, weighted; empty or all zero: the spawner does nothing.
    pub potentials: Vec<(u8, i32)>,
    /// `loot_tables_to_eject`, weighted.
    pub eject: Vec<(String, i32)>,
    /// `items_to_drop_when_ominous`: the table of the ominous item spawners.
    pub ominous_items: String,
}
impl Default for Wave {
    fn default() -> Self {
        Self {
            total: 6.0,
            simultaneous: 2.0,
            total_per_player: 2.0,
            simultaneous_per_player: 1.0,
            ticks_between_spawn: 40,
            potentials: Vec::new(),
            eject: Vec::new(),
            ominous_items: String::new(),
        }
    }
}
impl Wave {
    fn has_potentials(&self) -> bool {
        self.potentials.iter().any(|p| p.1 > 0)
    }
    fn total_target(&self, additional: i32) -> i32 {
        (self.total + self.total_per_player * additional as f32).floor() as i32
    }
    fn simultaneous_target(&self, additional: i32) -> i32 {
        (self.simultaneous + self.simultaneous_per_player * additional as f32).floor() as i32
    }
}

/// `TrialConfig`'s shared numbers. The ominous wave's defaults equal the normal ones here: vanilla's are not read yet (doubt).
#[derive(Clone, Debug, PartialEq)]
pub struct Cfg {
    pub spawn_range: i32,
    /// `target_cooldown_length` (36000 by default).
    pub cooldown: u64,
    /// `required_player_range` (14 by default).
    pub range: f64,
    pub normal: Wave,
    pub ominous: Wave,
}
impl Default for Cfg {
    fn default() -> Self {
        Self { spawn_range: 4, cooldown: 36_000, range: 14.0, normal: Wave::default(), ominous: Wave::default() }
    }
}

/// `TrialSpawnerState`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Inactive,
    WaitingForPlayers,
    Active,
    WaitingForRewardEjection,
    EjectingReward,
    Cooldown,
}

/// The omen a player carries: `Bad` turns into `Trial` when it makes the spawner ominous, `Trial` makes it so.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Omen {
    Bad,
    Trial,
}

/// What the level knows of a player beyond its position (creative and spectator players are never detected).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlayerState {
    pub creative: bool,
    pub spectator: bool,
    pub alive: bool,
    pub omen: Option<Omen>,
}

/// What the level answers: whether mob spawning is on, the seed, the player state, the block shapes that stop a sight line (a
/// full cube here: vanilla's visual shape is not read yet), the spawn rules of a species, the loot of a table, and the change of
/// a `Bad` omen into `Trial` (the effect data is the bridge's).
pub struct Env {
    pub enabled: bool,
    pub seed: i64,
    pub player: fn(&World, u32) -> PlayerState,
    pub solid: fn(&World, Block3) -> bool,
    pub valid: Valid,
    pub loot: fn(&mut World, &str, i64) -> Vec<ItemStack>,
    pub trial_omen: fn(&mut World, u32),
}

struct Trial {
    cfg: Cfg,
    state: State,
    ominous: bool,
    players: Vec<u32>,
    mobs: Vec<Entity>,
    total: i32,
    next_spawn: u64,
    cooldown_ends: u64,
    next_species: Option<u8>,
    eject_table: Option<String>,
    dispensing: Option<Vec<ItemStack>>,
    /// The tick of the wake-up scheduled for this spawner (`None`: none).
    wake: Option<u64>,
}
impl Trial {
    fn wave(&self) -> &Wave {
        if self.ominous {
            &self.cfg.ominous
        } else {
            &self.cfg.normal
        }
    }
}

/// The trial spawners of the world, their wake-ups, the mobs' owners and the ominous items on their way down.
#[derive(Resource, Default)]
pub struct Trials {
    at: HashMap<Block3, Trial>,
    wakes: BTreeMap<u64, Vec<Block3>>,
    owner: HashMap<Entity, Block3>,
    drops: BTreeMap<u64, Vec<(ItemStack, Vec3)>>,
}

/// What a test or a tool reads of one spawner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Info {
    pub state: State,
    pub ominous: bool,
    pub total: i32,
    pub mobs: usize,
    pub players: usize,
    pub cooldown_ends: u64,
}

/// Places a trial spawner at `pos`, inactive; its first wake-up is the next tick.
pub fn place(w: &mut World, pos: Block3, cfg: Cfg) {
    let now = w.resource::<Clock>().0;
    let t = Trial {
        cfg,
        state: State::Inactive,
        ominous: false,
        players: Vec::new(),
        mobs: Vec::new(),
        total: 0,
        next_spawn: 0,
        cooldown_ends: 0,
        next_species: None,
        eject_table: None,
        dispensing: None,
        wake: None,
    };
    let mut s = w.get_resource_or_insert_with(Trials::default);
    s.at.insert(pos, t);
    reschedule(&mut s, pos, Some(now + 1));
}

/// The state of the spawner at `pos`, if one is placed.
pub fn info(w: &World, pos: Block3) -> Option<Info> {
    let s = w.get_resource::<Trials>()?;
    let t = s.at.get(&pos)?;
    Some(Info {
        state: t.state,
        ominous: t.ominous,
        total: t.total,
        mobs: t.mobs.len(),
        players: t.players.len(),
        cooldown_ends: t.cooldown_ends,
    })
}

/// The ominous items on their way down (not yet dropped).
pub fn pending_drops(w: &World) -> usize {
    w.get_resource::<Trials>().map_or(0, |s| s.drops.values().map(Vec::len).sum())
}

/// The death of a mob the spawner made: it leaves the spawner's list at this event, not at its next wake-up.
pub fn forget(w: &mut World, e: Entity) {
    let Some(mut s) = w.get_resource_mut::<Trials>() else { return };
    if let Some(pos) = s.owner.remove(&e) {
        if let Some(t) = s.at.get_mut(&pos) {
            t.mobs.retain(|&m| m != e);
        }
    }
}

/// Runs, at this tick, the ominous items whose delay ended and the wake-ups due. Each spawner is taken out of the schedule while
/// it runs. Called every tick.
pub fn run_wakes(w: &mut World, env: &Env) {
    let now = w.resource::<Clock>().0;
    drop_due(w, now);
    let ready: Vec<Block3> = {
        let mut s = w.resource_mut::<Trials>();
        let later = s.wakes.split_off(&(now + 1));
        std::mem::replace(&mut s.wakes, later).remove(&now).unwrap_or_default()
    };
    for pos in ready {
        let taken = w.resource_mut::<Trials>().at.remove(&pos);
        let Some(mut t) = taken else { continue };
        if t.wake != Some(now) {
            w.resource_mut::<Trials>().at.insert(pos, t);
            continue;
        }
        t.wake = None;
        w.resource_scope(|w, mut rng: Mut<SpawnerRng>| step(w, &mut rng.0, env, pos, now, &mut t));
        let next = next_wake(&t, pos, now);
        let mut s = w.resource_mut::<Trials>();
        t.wake = next;
        if let Some(due) = next {
            s.wakes.entry(due).or_default().push(pos);
        }
        s.at.insert(pos, t);
    }
}

fn drop_due(w: &mut World, now: u64) {
    let due: Vec<(ItemStack, Vec3)> = {
        let mut s = w.resource_mut::<Trials>();
        let later = s.drops.split_off(&(now + 1));
        std::mem::replace(&mut s.drops, later).into_values().flatten().collect()
    };
    if due.is_empty() {
        return;
    }
    w.resource_scope(|w, mut rng: Mut<SpawnerRng>| {
        for (stack, at) in due {
            spawn_at_location(w, stack, at, &mut rng.0);
        }
    });
}

fn reschedule(s: &mut Trials, pos: Block3, due: Option<u64>) {
    if let Some(t) = s.at.get_mut(&pos) {
        t.wake = due;
    }
    if let Some(due) = due {
        s.wakes.entry(due).or_default().push(pos);
    }
}

fn aligned(pos: Block3, now: u64) -> bool {
    BlockPos::new(pos[0], pos[1], pos[2]).pack().wrapping_add(now as i64) % 20 == 0
}

/// The first aligned tick after `now` (the detection's ticks).
fn next_aligned(pos: Block3, now: u64) -> u64 {
    (now + 1..).find(|&t| aligned(pos, t)).unwrap_or(now + 1)
}

fn center(pos: Block3) -> [f64; 3] {
    pos.map(|v| f64::from(v) + 0.5)
}

fn disown(w: &mut World, e: Entity) {
    if let Some(mut s) = w.get_resource_mut::<Trials>() {
        s.owner.remove(&e);
    }
}

/// `Online` players in the reach box of the spawner (squared block distance under `range`²), not creative nor spectator, and,
/// with `sight`, those whose eye sees the centre.
fn detect(w: &World, env: &Env, pos: Block3, range: f64, sight: bool) -> Vec<u32> {
    let r2 = range * range;
    let mut found = Vec::new();
    for p in w.resource::<Online>().players() {
        let block = [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32];
        let d = [0, 1, 2].map(|i| f64::from(block[i] - pos[i]));
        if d[0] * d[0] + d[1] * d[1] + d[2] * d[2] >= r2 {
            continue;
        }
        let st = (env.player)(w, p.id);
        if st.creative || st.spectator {
            continue;
        }
        if sight && !in_line_of_sight(w, env, center(pos), [p.x, p.eye(), p.z]) {
            continue;
        }
        found.push(p.id);
    }
    found
}

/// `TrialSpawner.inLineOfSight`: the sight line from `to` back to the spawner meets nothing, or the spawner's own block.
fn in_line_of_sight(w: &World, env: &Env, origin: [f64; 3], to: [f64; 3]) -> bool {
    match visual_hit(w, env, to, origin) {
        None => true,
        Some(b) => b == origin.map(|v| v.floor() as i32),
    }
}

/// The first solid block the segment `from`..`to` meets, the end block included, or `None`: the cells it crosses, one by one
/// (Amanatides and Woo's walk over the unit cubes), in the order of the ray.
fn visual_hit(w: &World, env: &Env, from: [f64; 3], to: [f64; 3]) -> Option<Block3> {
    let mut cell = from.map(|v| v.floor() as i32);
    let goal = to.map(|v| v.floor() as i32);
    let d = [0, 1, 2].map(|i| to[i] - from[i]);
    let step = d.map(|v| if v < 0.0 { -1 } else { 1 });
    let mut t_max = [f64::INFINITY; 3];
    let mut t_delta = [f64::INFINITY; 3];
    for i in 0..3 {
        if d[i] != 0.0 {
            let edge = if step[i] > 0 { f64::from(cell[i] + 1) } else { f64::from(cell[i]) };
            t_max[i] = (edge - from[i]) / d[i];
            t_delta[i] = 1.0 / d[i].abs();
        }
    }
    for _ in 0..512 {
        if (env.solid)(w, cell) {
            return Some(cell);
        }
        if cell == goal {
            return None;
        }
        let axis = (0..3).min_by(|&a, &b| t_max[a].total_cmp(&t_max[b]))?;
        cell[axis] += step[axis];
        t_max[axis] += t_delta[axis];
    }
    None
}

/// `try_detect_players`: on an aligned tick, the players in reach join the list; a Bad omen becomes Trial and the spawner turns
/// ominous when a player in sight carries one; a new player brings the first wave in 40 ticks.
fn try_detect(w: &mut World, env: &Env, pos: Block3, now: u64, t: &mut Trial) {
    if !aligned(pos, now) {
        return;
    }
    let cooldown = t.state == State::Cooldown;
    if cooldown && t.ominous {
        return;
    }
    let range = t.cfg.range;
    let in_sight = detect(w, env, pos, range, true);
    let mut became = false;
    if !t.ominous && !in_sight.is_empty() {
        let mut found = None;
        for &id in &in_sight {
            match (env.player)(w, id).omen {
                Some(Omen::Trial) => {
                    found = Some((id, Omen::Trial));
                    break;
                }
                Some(Omen::Bad) => found = Some((id, Omen::Bad)),
                None => {}
            }
        }
        if let Some((id, omen)) = found {
            if omen == Omen::Bad {
                (env.trial_omen)(w, id);
            }
            apply_ominous(w, now, t);
            became = true;
        }
    }
    if cooldown && !became {
        return;
    }
    let found = if t.players.is_empty() { in_sight } else { detect(w, env, pos, range, false) };
    let before = t.players.len();
    for id in found {
        if !t.players.contains(&id) {
            t.players.push(id);
        }
    }
    if t.players.len() != before {
        t.next_spawn = t.next_spawn.max(now + FIRST_WAVE_DELAY);
    }
}

/// `applyOminous` then `resetAfterBecomingOminous`: the spawner's mobs vanish (flames at their place), the ominous wave starts.
fn apply_ominous(w: &mut World, now: u64, t: &mut Trial) {
    t.ominous = true;
    for e in std::mem::take(&mut t.mobs) {
        if let Some(p) = w.get::<Pos>(e).map(|p| p.0) {
            w.resource_mut::<Events>().host.particles("minecraft:flame", [p.x, p.y, p.z], [0.0; 3], 0.0, 20);
        }
        despawn::remove(w, e);
        disown(w, e);
    }
    if t.cfg.ominous.has_potentials() {
        t.next_species = None;
    }
    t.total = 0;
    t.next_spawn = now + t.cfg.ominous.ticks_between_spawn;
    t.cooldown_ends = now + ITEM_GAP;
}

/// `spawnOminousItemSpawner`: every tick, one item of the table is drawn (the table rolled once per 30x20x30 cell); once the gap
/// is over, its position is found and it drops after 60 to 120 ticks.
fn spawn_ominous_item(w: &mut World, env: &Env, pos: Block3, now: u64, rng: &mut LegacyRandom, t: &mut Trial) {
    if t.dispensing.is_none() {
        let cell = BlockPos::new(
            (pos[0] as f32 / 30.0).floor() as i32,
            (pos[1] as f32 / 20.0).floor() as i32,
            (pos[2] as f32 / 30.0).floor() as i32,
        );
        let drops = (env.loot)(w, &t.cfg.ominous.ominous_items, env.seed.wrapping_add(cell.pack()));
        if !drops.is_empty() {
            t.dispensing = Some(drops);
        }
    }
    let Some(items) = t.dispensing.as_ref() else { return };
    let counts: Vec<i32> = items.iter().map(|s| s.count as i32).collect();
    let Some(index) = weighted_index(&counts, rng) else { return };
    let mut item = items[index].clone();
    item.count = 1;
    if now < t.cooldown_ends {
        return;
    }
    let Some(at) = item_spawner_position(w, env, pos, t, rng) else { return };
    let delay = rng.next_int_between_inclusive(60, 120).unwrap_or(60) as u64;
    let pitch = (rng.next_float() - rng.next_float()) * 0.2 + 1.0;
    w.resource_mut::<Trials>().drops.entry(now + delay).or_default().push((item, Vec3::new(at[0], at[1], at[2])));
    w.resource_mut::<Events>().host.sound("block.trial_spawner.spawn_item_begin", at.map(f64::floor), 1.0, pitch);
    t.cooldown_ends = now + ITEM_GAP;
}

/// `calculatePositionAbove`: a player or a mob in reach, chosen by a coin; a visual clip 2 to 5 blocks above its head; the item
/// goes where the clip ends, when that block is free.
fn item_spawner_position(w: &World, env: &Env, pos: Block3, t: &Trial, rng: &mut LegacyRandom) -> Option<[f64; 3]> {
    let c = center(pos);
    let range2 = t.cfg.range * t.cfg.range;
    let near = |p: [f64; 3]| (p[0] - c[0]).powi(2) + (p[1] - c[1]).powi(2) + (p[2] - c[2]).powi(2) <= range2;
    let online = w.resource::<Online>();
    let players: Vec<([f64; 3], f64)> = t
        .players
        .iter()
        .filter_map(|&id| online.get(id).map(|p| (id, p)))
        .filter(|(id, p)| {
            let s = (env.player)(w, *id);
            !s.creative && !s.spectator && s.alive && near([p.x, p.y, p.z])
        })
        .map(|(_, p)| ([p.x, p.y, p.z], PLAYER_HEIGHT))
        .collect();
    if players.is_empty() {
        return None;
    }
    let mobs: Vec<([f64; 3], f64)> = t
        .mobs
        .iter()
        .filter_map(|&e| {
            let p = w.get::<Pos>(e)?.0;
            let h = w.get::<Body>(e)?.height;
            Some(([p.x, p.y, p.z], f64::from(h)))
        })
        .filter(|(p, _)| near(*p))
        .collect();
    let eligible = if rng.next_boolean() { mobs } else { players };
    let (at, height) = match eligible.len() {
        0 => return None,
        1 => eligible[0],
        n => eligible[rng.next_int_bound(n as i32).unwrap_or(0) as usize],
    };
    let up = height + 2.0 + f64::from(rng.next_int_bound(4).unwrap_or(0));
    let to = [at[0], at[1] + up, at[2]];
    let hit = visual_hit(w, env, at, to).unwrap_or_else(|| to.map(|v| v.floor() as i32));
    let down = [f64::from(hit[0]) + 0.5, f64::from(hit[1]) - 0.5, f64::from(hit[2]) + 0.5];
    (!(env.solid)(w, down.map(|v| v.floor() as i32))).then_some(down)
}

/// One tick of the spawner at `now`, as `tick_and_get_next`: the mobs out of reach leave the list, then the state moves.
fn step(w: &mut World, rng: &mut LegacyRandom, env: &Env, pos: Block3, now: u64, t: &mut Trial) {
    let before = t.mobs.len();
    let mut gone = Vec::new();
    t.mobs.retain(|&e| {
        let inside = w.get::<Pos>(e).is_some_and(|p| {
            let d = [
                p.0.x.floor() as i64 - i64::from(pos[0]),
                p.0.y.floor() as i64 - i64::from(pos[1]),
                p.0.z.floor() as i64 - i64::from(pos[2]),
            ];
            d.iter().map(|v| v * v).sum::<i64>() <= TRACK * TRACK
        });
        if !inside {
            gone.push(e);
        }
        inside
    });
    for e in gone {
        disown(w, e);
    }
    if t.mobs.len() != before {
        t.next_spawn = now + t.wave().ticks_between_spawn;
    }
    match t.state {
        State::Inactive => {
            if t.wave().has_potentials() {
                t.state = State::WaitingForPlayers;
            }
        }
        State::WaitingForPlayers => {
            if !env.enabled {
                reset_statistics(t);
            } else if !t.wave().has_potentials() {
                t.state = State::Inactive;
            } else {
                try_detect(w, env, pos, now, t);
                if !t.players.is_empty() {
                    t.state = State::Active;
                }
            }
        }
        State::Active => {
            if !env.enabled {
                reset_statistics(t);
                t.state = State::WaitingForPlayers;
                return;
            }
            if !t.wave().has_potentials() {
                t.state = State::Inactive;
                return;
            }
            // The wave's numbers are read before the detection, as vanilla's config copy does.
            let wave = t.wave().clone();
            let additional = (t.players.len() as i32 - 1).max(0);
            try_detect(w, env, pos, now, t);
            if t.ominous {
                spawn_ominous_item(w, env, pos, now, rng, t);
            }
            if t.total >= wave.total_target(additional) {
                if t.mobs.is_empty() {
                    t.cooldown_ends = now + t.cfg.cooldown;
                    t.total = 0;
                    t.next_spawn = 0;
                    t.state = State::WaitingForRewardEjection;
                }
            } else if now >= t.next_spawn && (t.mobs.len() as i32) < wave.simultaneous_target(additional) {
                if let Some(e) = spawn_mob(w, rng, env, pos, t, &wave) {
                    t.mobs.push(e);
                    w.resource_mut::<Trials>().owner.insert(e, pos);
                    t.total += 1;
                    t.next_spawn = now + wave.ticks_between_spawn;
                    t.next_species = weighted(&wave.potentials, rng);
                }
            }
        }
        State::WaitingForRewardEjection => {
            if now >= started(t) + DELAY_BEFORE_EJECT {
                w.resource_mut::<Events>().host.sound("block.trial_spawner.open_shutter", center(pos), 1.0, 1.0);
                t.state = State::EjectingReward;
            }
        }
        State::EjectingReward => {
            let s = started(t);
            if now < s || (now - s) % EJECT_EVERY != 0 {
                return;
            }
            if t.players.is_empty() {
                w.resource_mut::<Events>().host.sound("block.trial_spawner.close_shutter", center(pos), 1.0, 1.0);
                t.eject_table = None;
                t.state = State::Cooldown;
                return;
            }
            if t.eject_table.is_none() {
                let tables = if t.ominous { &t.cfg.ominous.eject } else { &t.cfg.normal.eject };
                let weights: Vec<i32> = tables.iter().map(|x| x.1).collect();
                t.eject_table = weighted_index(&weights, rng).map(|i| tables[i].0.clone());
            }
            if let Some(table) = t.eject_table.clone() {
                let seed = rng.next_long();
                let at = center(pos);
                for stack in (env.loot)(w, &table, seed) {
                    spawn_at_location(w, stack, Vec3::new(at[0], at[1], at[2]), rng);
                }
            }
            // The first detected player leaves the wave (vanilla: the first of the set's order).
            t.players.remove(0);
        }
        State::Cooldown => {
            try_detect(w, env, pos, now, t);
            if !t.players.is_empty() {
                t.total = 0;
                t.next_spawn = 0;
                t.state = State::Active;
            } else if now >= t.cooldown_ends {
                t.ominous = false;
                reset(t);
                t.state = State::WaitingForPlayers;
            }
        }
    }
}

fn started(t: &Trial) -> u64 {
    t.cooldown_ends.saturating_sub(t.cfg.cooldown)
}

fn reset_statistics(t: &mut Trial) {
    t.players.clear();
    t.total = 0;
    t.next_spawn = 0;
    t.cooldown_ends = 0;
}

fn reset(t: &mut Trial) {
    t.mobs.clear();
    reset_statistics(t);
    t.next_species = None;
}

/// `spawn_mob`: a position in the spawn range (the spawner's draws, in its order), the rules, the sight line; the mob is born there.
fn spawn_mob(w: &mut World, rng: &mut LegacyRandom, env: &Env, pos: Block3, t: &mut Trial, wave: &Wave) -> Option<Entity> {
    if t.next_species.is_none() {
        t.next_species = weighted(&wave.potentials, rng);
    }
    let id = t.next_species?;
    let sp = species_table().0.into_iter().find(|s| s.id == id)?;
    let r = f64::from(t.cfg.spawn_range);
    let at = [
        f64::from(pos[0]) + (rng.next_double() - rng.next_double()) * r + 0.5,
        f64::from(pos[1] + rng.next_int_bound(3).unwrap_or(0) - 1),
        f64::from(pos[2]) + (rng.next_double() - rng.next_double()) * r + 0.5,
    ];
    if !(env.valid)(w, &sp, at) || !in_line_of_sight(w, env, center(pos), at) {
        return None;
    }
    let _yaw = rng.next_float() * 360.0;
    let seed = rng.next_long() as u64;
    let nearest = nearest_sqr(w, at);
    let e = crate::spawning::born(w, &sp, at, seed, nearest);
    w.resource_mut::<Events>().host.particles("minecraft:flame", at, [0.0; 3], 0.0, 20);
    Some(e)
}

/// The tick of the next wake-up of a spawner after `now`, by its state (`None`: nothing changes without an outside event). An
/// ominous spawner wakes at every tick: its item spawner waits for the gap, which no other event ends.
fn next_wake(t: &Trial, pos: Block3, now: u64) -> Option<u64> {
    let later = |due: u64| due.max(now + 1);
    match t.state {
        State::Inactive => t.wave().has_potentials().then_some(now + 1),
        State::WaitingForPlayers => Some(next_aligned(pos, now)),
        State::Active => {
            if t.ominous {
                return Some(now + 1);
            }
            let detect = next_aligned(pos, now);
            let spawn = if t.next_spawn > now { t.next_spawn } else { now + 1 };
            Some(detect.min(spawn))
        }
        State::WaitingForRewardEjection => Some(later(started(t) + DELAY_BEFORE_EJECT)),
        State::EjectingReward => {
            let s = started(t);
            let k = if now < s { 0 } else { (now - s) / EJECT_EVERY + 1 };
            Some(s + k * EJECT_EVERY)
        }
        State::Cooldown => Some(later(t.cooldown_ends)),
    }
}

