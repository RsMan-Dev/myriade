//! `Cushion` (`decoration.Cushion`): a 1 x 0.25 pillow on the top face of a block, coloured by its dye, that seats one rider.
//! Survival and hazards (lava, fire) are the host's (`CushionHost`). Nothing ticks: the check (`BlockAttachedEntity.tick`, on the
//! 101st tick after the birth) is a wake armed when a block changes in a cell the cushion covers (`BlockEvents`). An intact cushion
//! costs nothing per tick, and a seat exists only while someone sits.
use super::Def;
use crate::behaviours::Cap;
use crate::block_events::BlockEvents;
use crate::cow::*;
use crate::goals::Events;
use crate::item::drop_item;
use crate::mount::Riding;
use crate::targeting::Who;
use bevy_ecs::prelude::*;
use horizon_client_lib::items::ItemStack;
use horizon_core::geometry::Vec3;
use horizon_core::nbt::Tag;
use std::collections::HashMap;

pub static DEF: Def = Def { name: "minecraft:cushion", caps: Some(&[Cap::Transform, Cap::Hanging]), ..Def::EMPTY };

/// `BlockAttachedEntity.tick`: the check falls on the 101st tick after the birth.
pub const CHECK_PERIOD: u64 = 101;
/// Ticks of the dropped cushion's pickup delay (`spawnAtLocation`).
const DROP_DELAY: i64 = 10;
/// `DyeColor` by ordinal.
pub(crate) const DYES: [&str; 16] = ["white", "orange", "magenta", "light_blue", "yellow", "lime", "pink", "gray", "light_gray", "cyan", "purple", "blue", "brown", "green", "red", "black"];
/// The wool each colour breaks into (`showBreakingParticles`).
const WOOL: [&str; 16] = [
    "minecraft:white_wool", "minecraft:orange_wool", "minecraft:magenta_wool", "minecraft:light_blue_wool", "minecraft:yellow_wool", "minecraft:lime_wool", "minecraft:pink_wool", "minecraft:gray_wool",
    "minecraft:light_gray_wool", "minecraft:cyan_wool", "minecraft:purple_wool", "minecraft:blue_wool", "minecraft:brown_wool", "minecraft:green_wool", "minecraft:red_wool", "minecraft:black_wool",
];

/// The cushion: the block it sits on, its birth in its own time, its `DyeColor` ordinal and its yaw (for the spawn).
#[derive(Component, Clone, Copy, Debug)]
#[component(on_remove = leave_index)]
pub struct Cushion {
    pub block: [i32; 3],
    pub born: u64,
    pub color: u8,
    pub yaw: f32,
}
/// The rider on the cushion (`Cushion.interact`): only while someone sits there.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Seat(pub Who);
/// The custom name (`getCustomName`), copied onto the dropped cushion: only a named cushion carries one.
#[derive(Component, Clone, Debug)]
#[component(storage = "SparseSet")]
pub struct Named(pub Box<Tag>);

/// The cushions by each cell they cover (the box, and the block it hangs on).
#[derive(Resource, Default)]
pub struct CushionIndex(HashMap<[i32; 3], Vec<Entity>>);

/// The host's answers about the world around a cushion at a point: placement (`canBePlacedAt`), survival (`wouldSurviveAt`), and
/// the hazard in its cell, lava first, then fire (`tickAtCheckInterval`).
#[derive(Resource)]
pub struct CushionHost {
    pub can_place: fn(&World, Vec3) -> bool,
    pub survives: fn(&World, Vec3) -> bool,
    pub hazard: fn(&World, Vec3) -> Hazard,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hazard {
    None,
    Lava,
    Fire,
}
/// `gameRules.ENTITY_DROPS`; absent: drops are on.
#[derive(Resource, Clone, Copy)]
pub struct EntityDrops(pub bool);

/// Who hurts the cushion: the environment (a blast with no owner, fire), a creature, or a player with what it may do there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Environment,
    Entity,
    Player { may_build: bool, may_interact: bool, creative: bool },
}

fn cell(v: f64) -> i32 {
    v.floor() as i32
}
/// The cells a cushion with its bottom centre at `at` covers: its box (x and z half a block out, 0.25 tall) and the block it
/// rests on (`Cushion.hasAnchorBelow` reaches 1/8 down).
fn covered(at: Vec3) -> Vec<[i32; 3]> {
    const EPS: f64 = 1.0e-7;
    let (x0, x1) = (cell(at.x - 0.5), cell(at.x + 0.5 - EPS));
    let (z0, z1) = (cell(at.z - 0.5), cell(at.z + 0.5 - EPS));
    let (y0, y1) = (cell(at.y - 0.140625), cell(at.y + 0.25 - EPS));
    let mut out = Vec::new();
    for x in x0..=x1 {
        for y in y0..=y1 {
            for z in z0..=z1 {
                out.push([x, y, z]);
            }
        }
    }
    out
}
fn pos_of(w: &World, e: Entity) -> Vec3 {
    w.get::<Pos>(e).map_or(Vec3::ZERO, |p| p.0)
}
fn leave_index(mut world: bevy_ecs::world::DeferredWorld, ctx: bevy_ecs::lifecycle::HookContext) {
    let Some(at) = world.get::<Pos>(ctx.entity).map(|p| p.0) else { return };
    let mut index = world.resource_mut::<CushionIndex>();
    for c in covered(at) {
        if let Some(list) = index.0.get_mut(&c) {
            list.retain(|&e| e != ctx.entity);
        }
    }
}

/// `CushionItem.useOn` → `Cushion.canBePlacedAt`: a cushion at `at` (its bottom centre) on the block `block`, when the host
/// accepts the place. `name` is the item's custom name.
pub fn place(w: &mut World, block: [i32; 3], at: Vec3, color: u8, yaw: f32, name: Option<Box<Tag>>) -> Option<Entity> {
    if w.get_resource::<CushionHost>().is_some_and(|h| !(h.can_place)(w, at)) {
        return None;
    }
    let e = w.spawn((Pos(at), OwnTime, Cushion { block, born: 0, color: color.min(15), yaw })).id();
    let born = now_of(w, e);
    w.get_mut::<Cushion>(e).expect("just spawned").born = born;
    if let Some(n) = name {
        w.entity_mut(e).insert(Named(n));
    }
    for c in covered(at) {
        w.resource_mut::<CushionIndex>().0.entry(c).or_default().push(e);
    }
    Some(e)
}

/// `Cushion.interact`: a rider sits on it, unless it is sneaking or the cushion is taken.
pub fn sit(w: &mut World, e: Entity, who: Who, sneak: bool) -> bool {
    if sneak || w.get::<Seat>(e).is_some() || w.get::<Cushion>(e).is_none() {
        return false;
    }
    w.entity_mut(e).insert(Seat(who));
    if let Who::Entity(r) = who {
        w.entity_mut(r).insert(Riding(e));
    }
    let at = pos_of(w, e);
    w.resource_mut::<Events>().host.sound("entity.cushion.sit", [at.x, at.y, at.z], 1.0, 1.0);
    true
}

/// `Cushion.removePassenger` on a cushion that lives on: the rider gets up, with the sound.
pub fn get_up(w: &mut World, e: Entity) -> Option<Who> {
    let Seat(who) = w.get::<Seat>(e).copied()?;
    w.entity_mut(e).remove::<Seat>();
    if let Who::Entity(r) = who {
        w.entity_mut(r).remove::<Riding>();
    }
    let at = pos_of(w, e);
    w.resource_mut::<Events>().host.sound("entity.cushion.get_up", [at.x, at.y, at.z], 1.0, 1.0);
    Some(who)
}

/// `Cushion.hurtServer`: a player who may not build or interact there hurts nothing (the hurt is refused, `false`); any other
/// hurt kills the cushion, with its source, and drops it.
pub fn hurt(w: &mut World, e: Entity, source: Source) -> bool {
    if let Source::Player { may_build, may_interact, .. } = source {
        if !may_build || !may_interact {
            return false;
        }
    }
    if w.get::<Cushion>(e).is_none() {
        return false;
    }
    remove(w, e, source);
    true
}

/// `Cushion.dropItem` (after `kill` or `discard`): the sound, the wool, and the cushion unless drops are off or a creative player broke
/// it. The rider of a removed cushion is let go without a sound.
fn remove(w: &mut World, e: Entity, source: Source) {
    let Some(c) = w.get::<Cushion>(e).copied() else { return };
    let at = pos_of(w, e);
    let named = w.get::<Named>(e).map(|n| n.0.clone());
    if let Some(Seat(Who::Entity(r))) = w.get::<Seat>(e).copied() {
        w.entity_mut(r).remove::<Riding>();
    }
    w.despawn(e);
    let p = [at.x, at.y, at.z];
    let mut events = w.resource_mut::<Events>();
    events.host.sound("entity.cushion.break", p, 1.0, 1.0);
    events.host.particles(WOOL[usize::from(c.color)], p, [0.25, 0.0625, 0.25], 0.05, 10);
    drop(events);
    let drops = w.get_resource::<EntityDrops>().is_none_or(|d| d.0);
    if !drops || matches!(source, Source::Player { creative: true, .. }) {
        return;
    }
    drop_item(w, item_stack(c.color, named), at, Vec3::ZERO, DROP_DELAY);
}
fn item_stack(color: u8, named: Option<Box<Tag>>) -> ItemStack {
    let mut stack = ItemStack::new(&format!("minecraft:{}_cushion", DYES[usize::from(color)]), 1);
    if let Some(n) = named {
        stack.data_mut().custom_name = Some(*n);
    }
    stack
}

/// `BlockAttachedEntity.tick` on its 101st tick: the lava and the fire break it; lost support discards it, dropping it all the same.
fn check(w: &mut World, e: Entity) {
    if w.get::<Cushion>(e).is_none() {
        return;
    }
    let at = pos_of(w, e);
    let (hazard, survives) = w.get_resource::<CushionHost>().map_or((Hazard::None, true), |h| ((h.hazard)(w, at), (h.survives)(w, at)));
    if hazard != Hazard::None {
        remove(w, e, Source::Environment);
    } else if !survives {
        remove(w, e, Source::Environment);
    }
}

fn arm(w: &mut World, e: Entity) {
    let Some(born) = w.get::<Cushion>(e).map(|c| c.born) else { return };
    let now = now_of(w, e);
    let next = born + CHECK_PERIOD * ((now - born) / CHECK_PERIOD + 1);
    after(w, e, (next - now) as i32, check);
}

/// Block changes wake the cushions that cover the changed cell: each arms its next check.
pub fn cushion_wake(w: &mut World) {
    let changed = match w.get_resource::<BlockEvents>() {
        Some(b) if !b.changed.is_empty() => b.changed.clone(),
        _ => return,
    };
    for c in changed {
        let on: Vec<Entity> = w.resource::<CushionIndex>().0.get(&c).cloned().unwrap_or_default();
        for e in on {
            arm(w, e);
        }
    }
}

pub fn insert_resources(w: &mut World) {
    w.insert_resource(CushionIndex::default());
}
pub fn register(s: &mut bevy_ecs::schedule::Schedule) {
    s.add_systems(cushion_wake.after(crate::cow::block_changes));
}
