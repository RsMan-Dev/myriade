//! Health, damage, hazards and fire.
//! A cow as data components and batch systems (palier 1): the stroll goal (a geometric wait with the law of
//! `nextInt(60) == 0` each tick, then `LandRandomPos` 10 x 7), the move control, `LivingEntity.travel` on the
//! ground with Horizon's real block collision sweep (`SceneCollisions::resolve_in_border_with`), and sleeping.
//! Species data (size, step, speed, stroll range) are component values, not branches.
#[allow(unused_imports)]
use crate::IFloor as _;
#[allow(unused_imports)]
use bevy_ecs::{
    entity_disabling::Disabled,
    prelude::*,
    query::Allow,
    schedule::{Schedule, ScheduleLabel, SingleThreadedExecutor},
};
#[allow(unused_imports)]
use myriade_engine_lib::mobs::{
    MobWorld,
    nav::Navigation,
    path::{Evaluator, Finder, Path, PathMob, PATH_TYPES},
};
#[allow(unused_imports)]
use myriade_level::{chunk::Chunk, collisions::SceneCollisions};
#[allow(unused_imports)]
use horizon_core::geometry::{Aabb, Vec3, trig};
#[allow(unused_imports)]
use std::sync::Arc;
use super::*;

/// `LivingEntity.health`.
#[derive(Component, Clone, Copy, Debug)]
pub struct Health(pub f32);

/// `LivingEntity.getMaxHealth`, for the bars and the regeneration that read it.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct MaxHealth(pub f32);

/// The last hurt: the tick and the amount (`invulnerableTime`: for 10 ticks after, only a larger hit counts, by the
/// difference).
#[derive(Component, Clone, Copy, Debug)]
pub struct HurtAt {
    pub at: u64,
    pub last: f32,
}

/// `Entity.invulnerableTime` set by a mod, as the entity tick it ends at (the window counts down by the clock, nothing runs per tick).
/// Absent: 0. While it runs the entity refuses what `isInvulnerableToBase` refuses (`isTemporarilyInvulnerable`).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct InvulnerableUntil(pub u64);

/// The permanent `Invulnerable` flag (`Entity.invulnerable`): the NBT key of the entity's free state, set by `/data`, `/summon` or a save.
pub fn has_invulnerable_flag(w: &World, e: Entity) -> bool {
    w.get::<crate::invulnerable::Invulnerable>(e).is_some()
}

/// `Entity.isInvulnerableToBase` for the `Invulnerable` flag (`flag`) or the `invulnerableTime` window (`window`): refused unless the
/// source is in `#bypasses_invulnerability` (`out_of_world`, `generic_kill`) or its causing entity is a creative player.
pub fn invulnerability_refuses(flag: bool, window: bool, cause: Cause, creative_attacker: bool) -> bool {
    (flag || window) && cause != Cause::OutOfWorld && !creative_attacker
}

/// One hit of the `CombatTracker` (`CombatEntry`): what the death message reads.
#[derive(Clone, Copy, Debug)]
pub struct CombatEntry {
    pub damage_type: &'static str,
    pub by: Option<crate::targeting::Who>,
    pub amount: f32,
    pub in_water: bool,
    pub fall_distance: f32,
    /// The blow is the attacker's own (not a projectile it shot): `DamageSource.getDirectEntity` is the attacker.
    pub direct: bool,
    /// The type of the projectile that carried the blow (the direct entity of its damage source, as a loot table reads it).
    pub projectile: Option<&'static str>,
}

/// `CombatTracker`: the hits taken lately, put on the entity by its first hit (nothing before). The hits are forgotten after 100 ticks
/// without one (300 in a fight), checked when the next one lands.
#[derive(Component, Clone, Debug, Default)]
#[component(storage = "SparseSet")]
pub struct CombatLog {
    pub entries: Vec<CombatEntry>,
    pub last: u64,
    pub in_combat: bool,
}

/// `CombatTracker.recordDamage` for a hit that took `amount`.
pub fn record_combat(w: &mut World, e: Entity, damage_type: &'static str, by: Option<crate::targeting::Who>, direct: bool, projectile: Option<&'static str>, amount: f32) {
    // Queued by `apply_damage`: the hit may have removed the entity before the queue runs.
    if w.get_entity(e).is_err() {
        return;
    }
    let now = now_of(w, e);
    let in_water = w.get::<FluidState>(e).is_some_and(|f| f.in_water);
    let fall_distance = w.get::<Contact>(e).map_or(0.0, |c| c.fall_distance as f32);
    if w.get::<CombatLog>(e).is_none() {
        w.entity_mut(e).insert(CombatLog::default());
    }
    let Some(mut log) = w.get_mut::<CombatLog>(e) else { return };
    let reset = if log.in_combat { 300 } else { 100 };
    if !log.entries.is_empty() && now.wrapping_sub(log.last) > reset {
        log.entries.clear();
        log.in_combat = false;
    }
    log.entries.push(CombatEntry { damage_type, by, amount, in_water, fall_distance, direct, projectile });
    log.last = now;
    if by.is_some() {
        log.in_combat = true;
    }
}

/// On fire until this tick; its damage is in the timer wheel (`Due::Burn`, every 20 ticks).
#[derive(Component, Clone, Copy, Debug)]
#[component(on_add = crate::behaviours::drink_potion::thirst_hook, on_insert = crate::bridge::view::touched, on_remove = crate::bridge::view::touched)]
pub struct Burning {
    pub until: i64,
}

/// `Entity.igniteForSeconds`: on fire for `ticks` more (the later end wins); the first burn comes with the wheel as for a fire under it.
/// A living entity's `igniteForTicks` scales them by `Attributes.BURNING_TIME` and rounds up (`Mth.ceil`).
pub fn set_on_fire(w: &mut World, e: Entity, ticks: u64) {
    if w.get::<Health>(e).is_none() || w.get::<Immune>(e).is_some_and(|i| i.has(crate::effects::IMMUNE_FIRE)) {
        return;
    }
    let ticks = (ticks as f64 * crate::attr_mod::read(w, e, "burning_time", 1.0).clamp(0.0, 1024.0)).ceil() as u64;
    let now = now_of(w, e);
    let until = now as i64 + ticks as i64;
    match w.get::<Burning>(e).map(|b| b.until) {
        None => {
            let at = w.resource::<Clock>().at(e, now + if ticks % 20 == 0 { 1 } else { 20 });
            w.resource_mut::<Wheel>().push(at, e, Due::Burn);
            w.entity_mut(e).insert(Burning { until });
            w.resource_mut::<crate::goals::Events>().ignited.push(e);
        }
        Some(old) if old < until => {
            w.entity_mut(e).insert(Burning { until });
        }
        _ => {}
    }
}

/// The hazards under the cow's box (bits of `HAZ_*`): present only while it overlaps one.
#[derive(Component, Clone, Copy, Debug)]
pub struct Hazard(pub u8);

/// The ticks `ticks_frozen` has counted in powder snow.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Frozen(pub i32);

pub const HAZ_LAVA: u8 = 1;

pub const HAZ_FIRE: u8 = 2;

pub const HAZ_WATER: u8 = 4;

pub const HAZ_SNOW: u8 = 8;

pub const HAZ_CACTUS: u8 = 16;

/// The damage waiting to be applied (what hurts a cow only queues it: a command).
#[derive(Resource, Default)]
pub struct DamageQueue(pub Vec<Hit>);

/// One hit: who, how much, what kind, and from where when an entity did it (the knockback comes from it).
#[derive(Clone, Copy, Debug)]
pub struct Hit {
    pub target: Entity,
    pub amount: f32,
    pub cause: Cause,
    /// The vanilla damage type's name (`DamageTypes`), as the death event says it.
    pub damage: &'static str,
    /// The attacker's position (x, z); `None` for an attacker without one or an environmental cause.
    pub from: Option<(f64, f64)>,
    /// Who did it (a player of the bench's `Online` list, or a creature), for the target of a `HurtByTarget`.
    pub by: Option<crate::targeting::Who>,
    /// The blow is the attacker's own (not a projectile it shot).
    pub direct: bool,
    /// The knockback's power when the attacker sets it (a goat's ram); `None`: the default 0.4.
    pub power: Option<f64>,
    /// A second knockback after the default one (`causeExtraKnockback`): the direction (x, z) and the power; `None` elsewhere.
    pub extra: Option<(f64, f64, f64)>,
    /// The projectile that carried the blow (`DamageSource.getDirectEntity` is it): its type id; `None` for a blow with no projectile.
    pub projectile: Option<&'static str>,
}

impl DamageQueue {
    pub fn add(&mut self, target: Entity, amount: f32, cause: Cause) {
        self.0.push(Hit { target, amount, cause, damage: cause_name(cause, None), from: None, by: None, direct: false, power: None, extra: None, projectile: None });
    }
    pub fn add_by(&mut self, target: Entity, amount: f32, from: (f64, f64), player: u32) {
        self.0.push(Hit { target, amount, cause: Cause::Entity, damage: "player_attack", from: Some(from), by: Some(crate::targeting::Who::Player(player)), direct: true, power: None, extra: None, projectile: None });
    }
    /// A blow of a creature standing at `from`: the victim may take it as its target.
    pub fn add_by_entity(&mut self, target: Entity, amount: f32, from: (f64, f64), attacker: Entity) {
        self.0.push(Hit { target, amount, cause: Cause::Entity, damage: "mob_attack", from: Some(from), by: Some(crate::targeting::Who::Entity(attacker)), direct: true, power: None, extra: None, projectile: None });
    }
    /// A blow of `who` through a projectile: the victim may take `who` as its target, but the blow is not direct.
    pub fn add_via(&mut self, target: Entity, amount: f32, from: (f64, f64), who: crate::targeting::Who, projectile: &'static str) {
        self.0.push(Hit { target, amount, cause: Cause::Entity, damage: cause_name(Cause::Entity, Some(who)), from: Some(from), by: Some(who), direct: false, power: None, extra: None, projectile: Some(projectile) });
    }
    /// A blow of a creature at `from` that pushes its target with `power` (`LivingEntity.knockback`), as a ram does.
    pub fn add_ram(&mut self, target: Entity, amount: f32, from: (f64, f64), power: f64) {
        self.0.push(Hit { target, amount, cause: Cause::Entity, damage: "mob_attack", from: Some(from), by: None, direct: false, power: Some(power), extra: None, projectile: None });
    }
    pub fn add_from(&mut self, target: Entity, amount: f32, cause: Cause, from: (f64, f64)) {
        self.0.push(Hit { target, amount, cause, damage: cause_name(cause, None), from: Some(from), by: None, direct: false, power: None, extra: None, projectile: None });
    }
    /// A blast's blow: `player_explosion` when the blast has a causing entity (`DamageSources.explosion`), else `explosion`.
    pub fn add_blast(&mut self, target: Entity, amount: f32, from: (f64, f64), caused: bool) {
        let damage = if caused { "player_explosion" } else { "explosion" };
        self.0.push(Hit { target, amount, cause: Cause::Blast, damage, from: Some(from), by: None, direct: false, power: None, extra: None, projectile: None });
    }
}

/// What hurt (`DamageType`): `Panic` reads it (`#panic_environmental_causes`: cactus, freeze, hot_floor, in_fire, lava,
/// lightning_bolt, on_fire).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cause {
    Cactus,
    /// `DamageTypes.SWEET_BERRY_BUSH`.
    SweetBerry,
    /// `DamageTypes.CAMPFIRE`.
    Campfire,
    /// `DamageTypes.HOT_FLOOR`: a magma block under the feet.
    HotFloor,
    InFire,
    OnFire,
    Lava,
    Freeze,
    Fall,
    Drown,
    /// `Entity.checkBelowWorld`: 4 a tick under the world (`DamageTypes.OUT_OF_WORLD`).
    OutOfWorld,
    InWall,
    /// `cramming`: too many entities in one place (`LivingEntity.pushEntities`); not a panic cause, no knockback.
    Cramming,
    /// A hit by an entity (a player): `Panic` runs from it by the attacker rule, not the environmental list.
    Entity,
    /// Poison and instant damage (`DamageTypes.MAGIC`).
    Magic,
    /// `DamageTypes.WITHER`.
    Wither,
    /// An explosion (`explosion.rs`).
    Blast,
    /// `DamageTypes.LIGHTNING_BOLT`.
    Lightning,
    /// `DamageTypes.FALLING_ANVIL` and `FALLING_BLOCK`: a block that fell on the creature (`falling_block.rs`).
    FallingBlock,
}
impl Cause {
    /// The damage type tag `IS_FIRE` (in_fire, campfire, on_fire, lava, hot_floor) that `fireImmune` and fire resistance block.
    pub fn is_fire(self) -> bool {
        matches!(self, Cause::InFire | Cause::OnFire | Cause::Lava | Cause::HotFloor | Cause::Campfire)
    }
}

/// The causes of the hurts of this step, in the order of `Hurts` (read by the goals that depend on the damage type).
#[derive(Resource, Default)]
pub struct HurtCauses(pub Vec<(Entity, Cause)>);

/// The hazard blocks of the world by column and height: only the columns that hold one have an entry. A cow's hazard
/// bits are those of the entries its box (widened by a block each way, as a prefilter) meets; a block change at any
/// height updates its entry (`set`).
#[derive(Resource, Default)]
pub struct Hazards {
    /// The cells that hold a turtle egg (what the egg hunters look for): a search that has none in its zone does not walk it.
    pub(crate) eggs: std::collections::BTreeSet<[i32; 3]>,
    /// Bumped by each `set` that changes an entry: what an entity read of the columns is valid while it holds.
    pub(crate) version: u32,
}

pub(crate) fn hazard_bit(name: &str) -> u8 {
    match name {
        "minecraft:lava" => HAZ_LAVA,
        "minecraft:fire" => HAZ_FIRE,
        "minecraft:water" | "minecraft:bubble_column" => HAZ_WATER,
        "minecraft:powder_snow" => HAZ_SNOW,
        "minecraft:cactus" => HAZ_CACTUS,
        _ => 0,
    }
}

impl Hazards {
    /// Reads the hazard blocks of the chunk (cx, cz) that just loaded (its record is made, `Index::chunk_created`): section by section, the ones
    /// whose palette holds none are skipped, so a generated ocean, lava lake or cactus field reaches the fluid and hazard modules of the creatures.
    pub fn scan_chunk(&mut self, chunks: &mut Chunks, mw: &myriade_engine_lib::mobs::MobWorld<'_>, cx: i32, cz: i32) {
        debug_assert!(chunks.find(cx, cz) != NO_SLOT, "a chunk is scanned before its record is made");
        let layout = mw.world.layout();
        let (x0, z0) = (cx << 4, cz << 4);
        let wanted = |id| {
            let n = &*mw.facts.get(id).name;
            hazard_bit(n) != 0 || n == EGG
        };
        let mut cursor = myriade_level::world::BlockCursor::new(mw.world);
        for s in 0..layout.sections() {
            let y0 = layout.section_y(s);
            if !mw.world.may_hold([x0, y0, z0], [x0 + 15, y0 + 15, z0 + 15], wanted) {
                continue;
            }
            for y in y0..y0 + 16 {
                for z in z0..z0 + 16 {
                    for x in x0..x0 + 16 {
                        let name = &mw.facts.get(cursor.block(x, y, z)).name;
                        if &**name == EGG {
                            self.eggs.insert([x, y, z]);
                        }
                        let bit = hazard_bit(name);
                        if bit != 0 {
                            self.set(chunks, x, y, z, bit);
                        }
                    }
                }
            }
        }
    }
    /// Chunk (cx, cz) unloads: its eggs are forgotten (its hazard columns go with its record).
    pub fn forget_chunk(&mut self, chunks: &Chunks, cx: i32, cz: i32) {
        let (x0, z0) = (cx << 4, cz << 4);
        let eggs: Vec<[i32; 3]> = self.eggs.range([x0, i32::MIN, i32::MIN]..=[x0 + 15, i32::MAX, i32::MAX]).filter(|p| (z0..z0 + 16).contains(&p[2])).copied().collect();
        let held = chunks.recs.get(chunks.find(cx, cz) as usize).is_some_and(|r| r.haz_count > 0);
        if held || !eggs.is_empty() {
            self.version = self.version.wrapping_add(1);
        }
        for p in eggs {
            self.eggs.remove(&p);
        }
    }
    /// The world holds no turtle egg at all.
    pub fn eggs_empty(&self) -> bool {
        self.eggs.is_empty()
    }
    /// A turtle egg lies in the box `lo..=hi` (cells).
    pub fn egg_in(&self, lo: [i32; 3], hi: [i32; 3]) -> bool {
        self.eggs.range([lo[0], i32::MIN, i32::MIN]..=[hi[0], i32::MAX, i32::MAX]).any(|p| (lo[1]..=hi[1]).contains(&p[1]) && (lo[2]..=hi[2]).contains(&p[2]))
    }
    /// The block at (x, y, z) now has these hazard bits (0: none), at any height. A change in a chunk that is not loaded is dropped: its
    /// blocks are read again when it loads (`scan_chunk`).
    pub fn set(&mut self, chunks: &mut Chunks, x: i32, y: i32, z: i32, bit: u8) {
        let s = chunks.slot_at(x >> 4, z >> 4);
        if s == NO_SLOT {
            return;
        }
        let ChunkRec { haz, haz_count, .. } = &mut chunks.recs[s as usize];
        if bit == 0 && haz.is_none() {
            return;
        }
        let h = haz.get_or_insert_with(|| Box::new(HazCols { cols: [0; 256], pool: Vec::new() }));
        let col = (((z & 15) << 4) | (x & 15)) as usize;
        let slot = h.cols[col] as usize;
        let old = if slot == 0 { 0 } else { h.pool[slot - 1].iter().find(|e| e.0 == y).map_or(0, |e| e.1) };
        if old == bit {
            return;
        }
        if slot == 0 {
            h.pool.push(Vec::new());
            h.cols[col] = h.pool.len() as u32;
        }
        let list = &mut h.pool[h.cols[col] as usize - 1];
        match (list.iter().position(|e| e.0 == y), bit) {
            (Some(p), 0) => {
                list.swap_remove(p);
                *haz_count -= 1;
            }
            (Some(p), _) => list[p].1 = bit,
            (None, _) => {
                list.push((y, bit));
                *haz_count += 1;
            }
        }
        self.version = self.version.wrapping_add(1);
    }
    /// The hazards of the columns a box covers, at the heights it covers (a block more each way); a chunk that is not loaded holds none.
    pub fn of_box(&self, chunks: &Chunks, b: &Aabb) -> u8 {
        self.bits_of(chunks, NO_SLOT, &self.key_of(b))
    }
    /// What `of_box` reads of a box: the columns and the two height ranges.
    #[inline]
    pub(crate) fn key_of(&self, b: &Aabb) -> [i32; 8] {
        let (mn, mx) = (b.min_position(), b.max_position());
        let e = 1.0e-7;
        // Fluids have a surface below the top of their block and flow: they are looked at a block wider (the real
        // test is `fluids`); fire, cactus and snow are the blocks the box overlaps.
        [mn.x.ifloor(), (mx.x - e).ifloor(), mn.z.ifloor(), (mx.z - e).ifloor(), mn.y.ifloor() - 1, (mx.y - e).ifloor() + 1, (mn.y + 0.001).ifloor(), (mx.y - 0.001).ifloor()]
    }
    /// The bits of a key; the chunks are reached from the slot `home` by the links of the neighbours (`NO_SLOT`: from the table).
    pub(crate) fn bits_of(&self, chunks: &Chunks, home: u32, k: &[i32; 8]) -> u8 {
        let [xa, xb, za, zb, y0, y1, x0, x1] = *k;
        let mut bits = 0;
        for cz in (za >> 4)..=(zb >> 4) {
            for cx in (xa >> 4)..=(xb >> 4) {
                let s = chunks.rel(home, cx, cz);
                if s == NO_SLOT {
                    continue;
                }
                let r = &chunks.recs[s as usize];
                let Some(h) = r.haz.as_deref().filter(|_| r.haz_count > 0) else { continue };
                for z in za.max(cz << 4)..=zb.min((cz << 4) | 15) {
                    for x in xa.max(cx << 4)..=xb.min((cx << 4) | 15) {
                        let c = h.cols[(((z & 15) << 4) | (x & 15)) as usize];
                        if c != 0 {
                            for &(y, bit) in &h.pool[c as usize - 1] {
                                let fluid = bit & (HAZ_WATER | HAZ_LAVA) != 0;
                                if (fluid && (y0..=y1).contains(&y)) || (!fluid && (x0..=x1).contains(&y)) {
                                    bits |= bit;
                                }
                            }
                        }
                    }
                }
            }
        }
        bits
    }
    /// `of_box` for an entity that keeps what it read last (`key`, `version`) and its hazard bits then (`prev`): nothing is read again while the
    /// box covers the same columns and heights and no hazard block changed.
    #[inline]
    pub fn of_box_kept(&self, chunks: &Chunks, b: &Aabb, cell: &mut Cell, prev: u8) -> u8 {
        let k = self.key_of(b);
        if cell.1 == k && cell.2 == self.version {
            return prev;
        }
        cell.1 = k;
        cell.2 = self.version;
        self.bits_of(chunks, if cell.0 == u32::MAX { NO_SLOT } else { cell.0 >> 8 }, &k)
    }
}

/// What a lethal hit does: with `vetoable`, the entity stays as `Dying` until `EcsWorld::resolve_death` (a totem's rule). Absent or
/// false: the death goes on at once, as before.
#[derive(Resource, Default)]
pub struct DeathPolicy {
    pub vetoable: bool,
}

/// Set by the host while a mod hears `hurt_enemy`: a hit of an attacker is then said (`HitDealt`); else nothing is written.
#[derive(Resource, Default)]
pub struct HitWatched(pub bool);

/// A lethal hit held by `DeathPolicy`: the health it ended at (at or under 0). Only the held entities carry it.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Dying {
    pub health: f32,
}

/// The vanilla damage type's name of a hit (`DamageTypes`): a player's or a creature's blow is `player_attack` or `mob_attack`.
pub fn cause_name(cause: Cause, by: Option<crate::targeting::Who>) -> &'static str {
    match cause {
        Cause::Entity => match by {
            Some(crate::targeting::Who::Player(_)) => "player_attack",
            _ => "mob_attack",
        },
        Cause::Cactus => "cactus",
        Cause::SweetBerry => "sweet_berry_bush",
        Cause::Campfire => "campfire",
        Cause::HotFloor => "hot_floor",
        Cause::InFire => "in_fire",
        Cause::OnFire => "on_fire",
        Cause::Lava => "lava",
        Cause::Freeze => "freeze",
        Cause::Fall => "fall",
        Cause::Drown => "drown",
        Cause::OutOfWorld => "out_of_world",
        Cause::InWall => "in_wall",
        Cause::Cramming => "cramming",
        Cause::Magic => "magic",
        Cause::Wither => "wither",
        Cause::Blast => "explosion",
        Cause::Lightning => "lightning_bolt",
        Cause::FallingBlock => "falling_block",
    }
}

/// The last attacker (`LivingEntity.getLastHurtByMob`): written by `apply_damage` for every hit that has an attacker; `at` is
/// the entity's own time (a module forgets it after 100 ticks as the `Mob` does).
#[derive(Component, Clone, Copy, Debug)]
pub struct LastHurtBy {
    pub who: crate::targeting::Who,
    pub at: u64,
}

/// `Attributes.ATTACK_DAMAGE` (0: the entity does not attack).
#[derive(Component, Clone, Copy, Debug)]
pub struct AttackDamage(pub f32);

/// `Attributes.KNOCKBACK_RESISTANCE` (a living entity).
#[derive(Component, Clone, Copy, Debug)]
pub struct Resist(pub f64);

/// What the entity does not suffer, as bits (a living entity).
#[derive(Component, Clone, Copy, Debug)]
pub struct Immune(pub u32);

/// No fall damage.
pub const IMMUNE_FALL: u32 = 1;

/// No window of invulnerability after a hurt (`ItemEntity.hurtServer` takes each hit in full; a living entity ignores the weaker ones for 10 ticks).
pub const NO_HURT_COOLDOWN: u32 = 4;

impl Immune {
    pub fn has(self, bit: u32) -> bool {
        self.0 & bit != 0
    }
}

/// A receiver that takes the environment's fire by its own rule (`ArmorStand.hurtServer` for `on_fire` and `in_fire`): the hit is handed
/// to it instead of the generic health. Only the entities that carry it pay the lookup, and only for a fire hit.
#[derive(Component, Clone, Copy)]
#[component(storage = "SparseSet")]
pub struct Intake(pub fn(&mut World, Entity, Cause));

/// `LivingEntity.hurtServer` for the queued hits: within 10 ticks of a hurt only a larger hit counts, by the
/// difference; a cow at 0 health dies (leaves the index and the world). The effects `Mob` attaches to a hit that goes
/// through are applied here, in the same path (read in `mobs.rs` `hurt_server`): the knockback of an entity's hit
/// (0.4 away from the attacker, or a random direction when it has no position, scaled by the species' resistance,
/// a hop on the ground; no knockback for the environmental causes nor for a hit inside the cooldown), the end of
/// love (`Animal.actuallyHurt`), and `noActionTime` back to 0 (`ai::after_hurt`: the stroll goal is allowed again for
/// 100 ticks). The goals read the hit through `Hurts` and `HurtCauses` (Panic).
#[allow(clippy::type_complexity)]
pub(crate) fn apply_damage(
    clock: Res<Clock>,
    mut queue: ResMut<DamageQueue>,
    mut q: Query<(&mut Health, &mut HurtAt, &Cell, &Pos, &mut Vel, &Contact, Option<&mut Rng>, Option<&Resist>, Option<&Immune>, Option<&crate::equipment::Armor>, Option<&SpeciesId>, Has<crate::behaviours::guardian_thorns::Spiky>, Has<Dying>, Has<crate::item::BlastResist>, Option<&crate::effects::Resistance>)>,
    species: Res<SpeciesTable>,
    mut hurts: ResMut<crate::goals::Hurts>,
    mut ps: ResMut<crate::hostile::PlayerState>,
    mut by: ResMut<crate::hostile::HurtBy>,
    mut causes: ResMut<HurtCauses>,
    mut events: ResMut<crate::goals::Events>,
    watched: Option<Res<HitWatched>>,
    mut absorbs: Query<&mut crate::absorption::Absorption>,
    intakes: Query<&Intake>,
    flags: Query<(), With<crate::invulnerable::Invulnerable>>,
    // The limit is 16 parameters: the new read-only queries go in this group.
    (windows, mut links, netids, raiders, dead): (Query<&InvulnerableUntil>, Query<&mut crate::damage_link::DamageLink>, Query<&crate::bridge::ids::NetId>, Query<&crate::raids::Raider>, Query<(), crate::tear_down::Dead>),
    mut commands: Commands,
) {
    // The causes of the previous step were read by the goals (they run after this system in the chain).
    causes.0.clear();
    for hit in std::mem::take(&mut queue.0) {
        let e = hit.target;
        if matches!(hit.cause, Cause::OnFire | Cause::InFire) {
            if let Ok(&Intake(take)) = intakes.get(e) {
                let cause = hit.cause;
                commands.queue(move |w: &mut World| take(w, e, cause));
                continue;
            }
        }
        let Ok((mut h, mut hurt, _, pos, mut v, c, rng, resist, immune, armor, sid, spiky, dying, blast_resist, damage_resist)) = q.get_mut(e) else {
            continue;
        };
        // A stack that resists fire or explosions takes no such hit, and no event of it.
        if dying
            || dead.contains(e)
            || (hit.cause.is_fire() && immune.is_some_and(|i| i.has(crate::effects::IMMUNE_FIRE)))
            || (hit.cause == Cause::Blast && blast_resist)
        {
            continue;
        }
        // A damage-linked entity takes nothing: the blow goes to its link (once per window).
        if let Ok(mut link) = links.get_mut(e) {
            use crate::damage_link::{Verdict, handed, verdict};
            let now = clock.of(e);
            match verdict(hit.cause, link.until > now, hit.by.is_some()) {
                Verdict::Takes => {}
                Verdict::Refused => continue,
                Verdict::Handed => {
                    link.until = now + crate::damage_link::WINDOW;
                    let by = hit.by;
                    commands.queue(move |w: &mut World| handed(w, e, by));
                    continue;
                }
            }
        }
        let flag = flags.contains(e);
        let window = windows.get(e).is_ok_and(|u| u.0 > clock.of(e));
        let creative = matches!(hit.by, Some(crate::targeting::Who::Player(id)) if ps.creative(id));
        if invulnerability_refuses(flag, window, hit.cause, creative) {
            continue;
        }
        if spiky {
            let (by, direct, cause) = (hit.by, hit.direct, hit.cause);
            commands.queue(move |w: &mut World| crate::behaviours::guardian_thorns::blow(w, e, by, direct, cause));
        }
        let now = clock.of(e);
        let cooldown = now < hurt.at + 10 && hurt.at != 0 && !immune.is_some_and(|i| i.has(NO_HURT_COOLDOWN));
        let mut applied = if cooldown {
            if hit.amount <= hurt.last {
                continue;
            }
            hit.amount - hurt.last
        } else {
            hit.amount
        };
        // `LivingEntity.getDamageAfterArmorAbsorb`: what the worn armor stops, unless the hurt is in `#bypasses_armor`.
        if crate::equipment::armor_applies(hit.cause) {
            // `LivingEntity.hurtArmor` takes the hurt before the absorb: each worn piece wears by what the hit was.
            let pre = applied;
            commands.queue(move |w: &mut World| {
                crate::equipment_wear::hurt_armor(w, e, pre);
                crate::equipment_wear::hurt_body(w, e, pre);
            });
            let (a, t) = armor.map_or_else(|| sid.map_or((0.0, 0.0), |s| crate::equipment::base_armor(&species.0[s.0 as usize])), |a| (a.armor, a.toughness));
            applied = crate::equipment::absorb(applied, a, t);
        }
        // `LivingEntity.getDamageAfterMagicAbsorb`: Resistance takes `(amp + 1) * 5` of 25 off (none at level 5 or more); `starve` bypasses
        // the effects, `out_of_world` and `generic_kill` bypass Resistance (`bypasses_effects`, `bypasses_resistance` tags, jar).
        if let Some(r) = damage_resist && !matches!(hit.damage, "starve" | "out_of_world" | "generic_kill") {
            applied = (applied * (25 - 5 * r.0) as f32 / 25.0).max(0.0);
        }
        *hurt = HurtAt { at: now, last: hit.amount };
        // `Raider.hurtServer`: the raid's bar is recomputed before the damage, with the health the raider has now.
        if let Ok(r) = raiders.get(e)
            && let Some(raid) = r.raid
            && let Ok(&crate::bridge::ids::NetId(id)) = netids.get(e)
        {
            events.host.raider_hurt(raid, id, h.0);
        }
        // `LivingEntity.getDamageAfterAbsorb`: the absorption hearts take the hit after the armor and Resistance; what they took is
        // credited to a player who dealt it (`dealt_absorbed`).
        let mut absorbed = 0.0_f32;
        if let Ok(mut a) = absorbs.get_mut(e) {
            absorbed = a.0.min(applied).max(0.0);
            a.0 -= absorbed;
            if a.0 <= 0.0 {
                commands.entity(e).remove::<crate::absorption::Absorption>();
            }
        }
        applied -= absorbed;
        let health_before = h.0;
        h.0 -= applied;
        if applied != 0.0 {
            commands.queue(move |w: &mut World| crate::bridge::view::mark(w, e));
        }
        let health_after = h.0;
        commands.queue(move |w: &mut World| crate::behaviours::crack_sound::hurt(w, e, health_before, health_after));
        // `Mob.doHurtTarget`'s `hurtServer` returned true: what follows a landed melee blow (`hostile::landed`).
        if let (Some(crate::targeting::Who::Entity(attacker)), true, "mob_attack") = (hit.by, hit.direct, hit.damage) {
            commands.queue(move |w: &mut World| crate::hostile::landed(w, attacker, crate::targeting::Who::Entity(e), applied));
        }
        let (damage_type, by_log, direct_log, projectile) = (hit.damage, hit.by, hit.direct, hit.projectile);
        commands.queue(move |w: &mut World| record_combat(w, e, damage_type, by_log, direct_log, projectile, applied));
        if let Some(who) = hit.by
            && (watched.as_ref().is_some_and(|w| w.0) || (absorbed > 0.0 && matches!(who, crate::targeting::Who::Player(_))))
        {
            events.host.hit_dealt(who, e, applied, absorbed);
        }
        let by_who = hit.by;
        commands.queue(move |w: &mut World| crate::item::hurt_event(w, e, by_who));
        // `resolvePlayerResponsibleForDamage`: before a fatal hit is settled, so that the death reads it.
        commands.queue(move |w: &mut World| crate::credit::record(w, e, by_who));
        commands.queue(move |w: &mut World| crate::reinforcements::on_hurt(w, e, by_who));
        commands.queue(move |w: &mut World| crate::hurt_spawn::on_hurt(w, e));
        if h.0 <= 0.0 {
            // A totem may save it, the host may keep it `Dying`: `using::lethal` ends the hit once the commands run.
            let (damage, at, left) = (hit.damage, [pos.0.x, pos.0.y, pos.0.z], h.0);
            commands.queue(move |w: &mut World| crate::sound_slots::death(w, e));
            commands.queue(move |w: &mut World| crate::using::lethal(w, e, damage, by_who, at, left));
            continue;
        }
        hurts.0.push(e);
        commands.queue(move |w: &mut World| crate::sound_slots::hurt(w, e));
        events.hurt.push(e);
        if let Ok(&crate::bridge::ids::NetId(id)) = netids.get(e) {
            let cause = match hit.by {
                Some(crate::targeting::Who::Player(p)) => p,
                Some(crate::targeting::Who::Entity(a)) => netids.get(a).map_or(0, |n| n.0),
                None => 0,
            };
            let attacker = match hit.by {
                Some(crate::targeting::Who::Entity(a)) => Some(a),
                _ => None,
            };
            events.host.hurt(id, hit.damage, cause, hit.direct, [pos.0.x, pos.0.y, pos.0.z], attacker);
        }
        events.vibration.push(crate::behaviours::vibration_listen::Vibration { pos: [pos.0.x, pos.0.y, pos.0.z], source: hit.by, owner: None, projectile: false });
        if hit.by.is_some() {
            events.hurt_by_entity.push(e);
        }
        causes.0.push((e, hit.cause));
        commands.entity(e).insert(crate::brain::LastDamage { at: clock.of(e), panic: !matches!(hit.cause, Cause::Fall | Cause::Drown | Cause::InWall | Cause::Cramming), by: hit.by });
        if let Some(p) = hit.by {
            by.0.push((e, p));
            // `Player.setLastHurtMob`: what a player hit last, for the pets that attack it.
            if let crate::targeting::Who::Player(id) = p {
                ps.set_last_hurt(id, crate::targeting::Who::Entity(e), clock.0);
            }
        }
        // The reactions of the modules to the hurt (`Animal.actuallyHurt` ends love, `after_hurt` resets `noActionTime`...): each
        // module of the entity has its `damaged` entry point; the frame does not know what they do.
        // `getLastHurtByMob`: who hurt it last, readable by every module (`ResetUniversalAngerTarget`, revenge...), with its own time.
        if let Some(who) = hit.by {
            commands.entity(e).insert(LastHurtBy { who, at: clock.of(e) });
        }
        let (who, direct) = (hit.by, hit.direct);
        commands.queue(move |w: &mut World| {
            crate::goals::dispatch_damaged(w, e, &crate::behaviours::Damaged { amount: applied, who, direct, cause: hit.cause });
            crate::despawn::touch(w, e);
        });
        // `dealDefaultKnockback`: an entity's hit outside the cooldown (the other causes are in `#no_knockback`).
        if hit.cause == Cause::Entity && !cooldown {
            let power = hit.power.unwrap_or(0.4_f64) * (1.0 - resist.map_or(0.0, |r| r.0));
            if power > 0.0 {
                let (mut dx, mut dz) = hit.from.map_or((0.0, 0.0), |f| (f.0 - pos.0.x, f.1 - pos.0.z));
                let mut spare = Rng(clock.0 | 1);
                let mut rng = rng;
                while dx * dx + dz * dz < f64::from(1.0e-5_f32) {
                    let r: &mut Rng = match rng.as_mut() {
                        Some(r) => r,
                        None => &mut spare,
                    };
                    dx = (r.next_f64() - r.next_f64()) * 0.01;
                    dz = (r.next_f64() - r.next_f64()) * 0.01;
                }
                let l = (dx * dx + dz * dz).sqrt();
                let (kx, kz) = (dx / l * power, dz / l * power);
                let y = if c.on_ground { (v.0.y / 2.0 + power).min(0.4) } else { v.0.y };
                v.0 = Vec3::new(v.0.x / 2.0 - kx, y, v.0.z / 2.0 - kz);
                commands.entity(e).insert(Moving);
            }
            // `causeExtraKnockback`: a second push along `(x, z)`, its power cut by the same resistance, after the default one.
            if let Some((sx, sz, extra)) = hit.extra {
                let p = extra * (1.0 - resist.map_or(0.0, |r| r.0));
                let l = (sx * sx + sz * sz).sqrt();
                if p > 0.0 && l > 0.0 {
                    let y = if c.on_ground { (v.0.y / 2.0 + p).min(0.4) } else { v.0.y };
                    v.0 = Vec3::new(v.0.x / 2.0 - sx / l * p, y, v.0.z / 2.0 - sz / l * p);
                    commands.entity(e).insert(Moving);
                }
            }
        }
    }
}

/// `Entity.playEntityOnFireExtinguishedSound`: volume 0.7, pitch 1.6 and two draws of the body's generator times 0.4.
fn hiss(events: &mut crate::goals::Events, at: Vec3, rng: Option<Mut<Rng>>) {
    let spread = rng.map_or(0.0, |mut r| (r.next_float() - r.next_float()) * 0.4);
    events.host.sound("entity.generic.extinguish_fire", [at.x, at.y, at.z], 0.7, 1.6 + spread);
}

/// `Entity.isInWaterOrRain`: a burning body in rain, at its block or at the block of its top, is put out as in water. Only the
/// burning bodies are visited.
pub(crate) fn rain_out(
    clock: Res<Clock>,
    level: Res<Level>,
    mut events: ResMut<crate::goals::Events>,
    mut q: Query<(Entity, &Pos, &Bounds, &Burning, Option<&mut Rng>)>,
    mut commands: Commands,
) {
    let mw = level.mw.0;
    if !mw.raining {
        return;
    }
    for (e, pos, b, fire, rng) in &mut q {
        let (x, y, z) = (pos.0.x.floor() as i32, pos.0.y.floor() as i32, pos.0.z.floor() as i32);
        if mw.raining_at(x, y, z) || mw.raining_at(x, b.0.max_position().y.floor() as i32, z) {
            // The hiss only from a fire that was burning before this tick (more than one tick left).
            if fire.until - clock.of(e) as i64 > 1 {
                hiss(&mut events, pos.0, rng);
            }
            commands.entity(e).remove::<Burning>();
            events.extinguished.push(e);
        }
    }
}

/// The module of the cows that overlap a hazard (only they are visited): lava ignites 15 s and hurts 4, fire ignites
/// 8 s, a cactus hurts 1, powder snow counts the freezing (1 damage every 40 ticks once frozen for 140), water puts
/// the fire out. The fire's own damage (1 every 20 ticks) is in the timer wheel.
#[allow(clippy::type_complexity)]
/// `ItemTags.FREEZE_IMMUNE_WEARABLES` (`tags/item/freeze_immune_wearables.json`).
const FREEZE_IMMUNE_WEARABLES: &[&str] = &["minecraft:leather_boots", "minecraft:leather_leggings", "minecraft:leather_chestplate", "minecraft:leather_helmet", "minecraft:leather_horse_armor"];
/// `LivingEntity.canFreeze`: a stack of the tag worn in an armour slot (head to feet, and the body: slots 2 to 6) refuses the freezing.
fn wears_freeze_immune(worn: &Query<&crate::equip_stack::Equipment>, e: Entity) -> bool {
    worn.get(e).is_ok_and(|q| q.slots[2..7].iter().flatten().any(|s| FREEZE_IMMUNE_WEARABLES.contains(&&*s.item)))
}
pub(crate) fn hazard_tick(
    clock: Res<Clock>,
    level: Res<Level>,
    mut wheel: ResMut<Wheel>,
    mut queue: ResMut<DamageQueue>,
    mut events: ResMut<crate::goals::Events>,
    mut q: Query<(Entity, &Hazard, &mut Frozen, Option<&Burning>, &Pos, &Bounds, &FluidParams, Option<&Immune>, Option<&mut Rng>)>,
    worn: Query<&crate::equip_stack::Equipment>,
    mut commands: Commands,
) {
    let mw = level.mw.0;
    for (e, h, mut frozen, burning, pos, b, sid, immune, rng) in &mut q {
        // A stack that resists fire does not catch fire from the lava or the flames (its hits are refused in `apply_damage`).
        let fire_proof = immune.is_some_and(|i| i.has(crate::effects::IMMUNE_FIRE));
        let now = clock.of(e);
        let f = h.0;
        // Water and lava as the `Mob` reads them: the fluid height at the cow's box, not the column.
        let (in_water, in_lava) = if f & (HAZ_WATER | HAZ_LAVA) != 0 {
            let r = mw.fluids(b.0, pos.0.y + sid.eye_height);
            (r.in_water, r.in_lava)
        } else {
            (false, false)
        };
        let lit = f & HAZ_FIRE != 0 || in_lava;
        if in_water {
            if let Some(fire) = burning {
                // The hiss only from a fire that was burning before this tick (more than one tick left).
                if fire.until - now as i64 > 1 {
                    hiss(&mut events, pos.0, rng);
                }
                commands.entity(e).remove::<Burning>();
                events.extinguished.push(e);
            }
        } else {
            let seconds = if fire_proof { 0 } else if in_lava { 300 } else if lit { 160 } else { 0 };
            if seconds > 0 {
                let until = now as i64 + seconds as i64;
                match burning {
                    None => {
                        // `Entity.baseTick` hurts when the time left is a multiple of 20, the first tick included.
                        wheel.push(clock.at(e, now + if seconds % 20 == 0 { 1 } else { 20 }), e, Due::Burn);
                        commands.entity(e).insert(Burning { until });
                        events.ignited.push(e);
                    }
                    Some(b) if b.until < until => {
                        commands.entity(e).insert(Burning { until });
                    }
                    _ => {}
                }
            }
        }
        if in_lava {
            queue.add(e, 4.0, Cause::Lava);
        }
        if f & HAZ_FIRE != 0 {
            // `FireBlock.entityInside`: the fire hurts 1 as well as setting alight.
            queue.add(e, 1.0, Cause::InFire);
        }
        if f & HAZ_CACTUS != 0 {
            queue.add(e, 1.0, Cause::Cactus);
        }
        // `Entity.baseTick`: the powder snow counts only where `canFreeze` (no `IMMUNE_FREEZE` bit).
        if f & HAZ_SNOW != 0 && !immune.is_some_and(|i| i.has(crate::effects::IMMUNE_FREEZE)) && !wears_freeze_immune(&worn, e) {
            frozen.0 = (frozen.0 + 1).min(140);
            commands.queue(move |w: &mut World| crate::bridge::view::mark(w, e));
            if frozen.0 >= 140 && now % 40 == 0 {
                queue.add(e, 1.0, Cause::Freeze);
            }
        }
    }
}
