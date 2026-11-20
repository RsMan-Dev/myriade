//! Hanging entities (`HangingEntity`, `BlockAttachedEntity`): an item frame, a glow frame, a painting, fixed on a face of a block.
//! State: `Attached` (its cell, its face, its kind, its birth in its own time), `Size` (a painting's footprint; a frame is 1 x 1) and
//! `Frame` (the frame's rotation and its item). Nothing ticks. Vanilla's survival check runs every 101 ticks counted from the birth
//! (`ticksSinceLastCheck` is incremented, then compared with 100 before the test: the 101st tick). It is a deadline of the wheel
//! (`cow::after`), armed when the entity is hung beside one it overlaps, and when a block changes under it (`BlockEvents`). An intact
//! hanging entity costs nothing per tick. The walls and the free space are the host's (`Walls`).
use crate::block_events::BlockEvents;
use crate::cow::*;
use crate::goals::Events;
use crate::item::drop_item;
use bevy_ecs::prelude::*;
use horizon_client_lib::items::ItemStack;
use horizon_core::geometry::Vec3;
use std::collections::HashMap;

pub const ITEM_FRAME: u8 = 0;
pub const GLOW_ITEM_FRAME: u8 = 1;
pub const PAINTING: u8 = 2;
/// `BlockAttachedEntity.tick`: the checks fall on the 101st tick after the birth.
pub const CHECK_PERIOD: u64 = 101;
/// Ticks of the dropped item's pickup delay (`Entity.spawnAtLocation`).
const DROP_DELAY: i64 = 10;

/// What a kind drops when it breaks, and its break sound.
pub struct Kind {
    pub item: &'static str,
    pub break_sound: &'static str,
}
pub const KINDS: &[Kind] = &[
    Kind { item: "minecraft:item_frame", break_sound: "entity.item_frame.break" },
    Kind { item: "minecraft:glow_item_frame", break_sound: "entity.item_frame.break" },
    Kind { item: "minecraft:painting", break_sound: "entity.painting.break" },
];

/// A hanging entity: the cell it occupies, the face it hangs on (`Direction.get3DDataValue`: down 0, up 1, north 2, south 3,
/// west 4, east 5), its kind and its birth (its own time).
#[derive(Component, Clone, Copy, Debug)]
#[component(on_remove = leave_index)]
pub struct Attached {
    pub pos: [i32; 3],
    pub face: u8,
    pub kind: u8,
    pub born: u64,
}
/// The footprint of a painting in cells along its left and up (`width`, `height`); a frame has none.
#[derive(Component, Clone, Copy, Debug)]
pub struct Size {
    pub width: u8,
    pub height: u8,
}
/// An item frame: its rotation (`ItemFrame.rotation`, -7..=7 in 45 degree steps) and the item it holds.
#[derive(Component, Clone, Debug)]
pub struct Frame {
    pub rotation: i8,
    pub item: Option<Box<ItemStack>>,
}

/// The hanging entities by the cell of the block they hang on: the wall that decides whether they stay.
#[derive(Resource, Default)]
pub struct AttachIndex(HashMap<[i32; 3], Vec<Entity>>);

/// The host's answer for the wall: `(world, pos, face, size) -> true` when the block behind the footprint holds the face and the
/// footprint is free of solid blocks. Absent: the wall is taken to hold.
#[derive(Resource)]
pub struct Walls(pub fn(&World, [i32; 3], u8, (u8, u8)) -> bool);

fn leave_index(mut world: bevy_ecs::world::DeferredWorld, ctx: bevy_ecs::lifecycle::HookContext) {
    let Some(a) = world.get::<Attached>(ctx.entity).copied() else { return };
    let key = support(a.pos, a.face);
    if let Some(list) = world.resource_mut::<AttachIndex>().0.get_mut(&key) {
        list.retain(|&e| e != ctx.entity);
    }
}

/// The face's outward step (`Direction.getStep`, the order of `Direction.get3DDataValue`).
fn step(face: u8) -> [i32; 3] {
    [[0, -1, 0], [0, 1, 0], [0, 0, -1], [0, 0, 1], [-1, 0, 0], [1, 0, 0]][usize::from(face % 6)]
}
/// The block the entity is fixed to: one step behind its cell.
fn support(pos: [i32; 3], face: u8) -> [i32; 3] {
    let s = step(face);
    [pos[0] - s[0], pos[1] - s[1], pos[2] - s[2]]
}
/// The direction a painting's width runs in (`Direction.getCounterClockWise`), per face.
fn left(face: u8) -> [i32; 3] {
    [[0, 0, 0], [0, 0, 0], [-1, 0, 0], [1, 0, 0], [0, 0, 1], [0, 0, -1]][usize::from(face % 6)]
}
/// The cells a footprint of `n` cells spans from its cell: `Painting.calculateBoundingBox` centres an odd size on the cell and an
/// even one on its edge, so the first offset is `-(n-1)/2` and the last `n/2`.
fn span(n: u8) -> std::ops::RangeInclusive<i32> {
    -((i32::from(n) - 1) / 2)..=(i32::from(n) / 2)
}
/// The cells of the footprint of a hanging entity on `face` at `pos` (in front of the wall).
fn cells(pos: [i32; 3], face: u8, size: (u8, u8)) -> Vec<[i32; 3]> {
    let l = left(face);
    let mut out = Vec::new();
    for a in span(size.0) {
        for b in span(size.1) {
            out.push(std::array::from_fn(|i| pos[i] + a * l[i] + b * i32::from(i == 1)));
        }
    }
    out
}
fn size_of(w: &World, e: Entity) -> (u8, u8) {
    w.get::<Size>(e).map_or((1, 1), |s| (s.width, s.height))
}

/// Hangs an entity of `kind` at `pos` on `face`. Its survival check is armed when it overlaps another hanging entity (the one
/// created and the ones it overlaps, which vanilla checks in the same way).
pub fn hang(w: &mut World, pos: [i32; 3], face: u8, kind: u8, size: Option<Size>, frame: Option<Frame>) -> Entity {
    let at = Vec3::new(f64::from(pos[0]) + 0.5, f64::from(pos[1]) + 0.5, f64::from(pos[2]) + 0.5);
    let e = w.spawn((crate::cow::Pos(at), OwnTime, Attached { pos, face, kind, born: 0 })).id();
    let born = now_of(w, e);
    w.get_mut::<Attached>(e).expect("just spawned").born = born;
    if let Some(s) = size {
        w.entity_mut(e).insert(s);
    }
    if let Some(f) = frame {
        w.entity_mut(e).insert(f);
    }
    w.resource_mut::<AttachIndex>().0.entry(support(pos, face)).or_default().push(e);
    let overlapped = overlapping(w, e);
    if !overlapped.is_empty() {
        arm(w, e);
        for o in overlapped {
            arm(w, o);
        }
    }
    e
}

/// The other hanging entities whose footprint shares a cell with `e`'s (same face: the cells of one plane).
fn overlapping(w: &World, e: Entity) -> Vec<Entity> {
    let Some(a) = w.get::<Attached>(e).copied() else { return Vec::new() };
    let mine = cells(a.pos, a.face, size_of(w, e));
    let Some(index) = w.get_resource::<AttachIndex>() else { return Vec::new() };
    let mut found = Vec::new();
    for c in &mine {
        for &o in index.0.get(&support(*c, a.face)).into_iter().flatten() {
            if o == e || found.contains(&o) {
                continue;
            }
            let Some(b) = w.get::<Attached>(o).copied() else { continue };
            if b.face == a.face && cells(b.pos, b.face, size_of(w, o)).iter().any(|x| mine.contains(x)) {
                found.push(o);
            }
        }
    }
    found
}

/// Arms the entity's next survival check: the first multiple of 101 after its birth that is strictly after now.
pub fn arm(w: &mut World, e: Entity) {
    let Some(a) = w.get::<Attached>(e).copied() else { return };
    let now = now_of(w, e);
    let next = a.born + CHECK_PERIOD * ((now - a.born) / CHECK_PERIOD + 1);
    after(w, e, (next - now) as i32, check);
}

/// The survival check (`BlockAttachedEntity.tick`'s `survives`): an entity that no longer survives breaks.
fn check(w: &mut World, e: Entity) {
    let Some(a) = w.get::<Attached>(e).copied() else { return };
    let size = size_of(w, e);
    let walls_hold = w.get_resource::<Walls>().map_or(true, |f| (f.0)(w, a.pos, a.face, size));
    let free = overlapping(w, e).is_empty();
    if !walls_hold || !free {
        break_entity(w, e, a);
    }
}

/// `dropItem(level, null)`: the sound, the item (and the framed item), then the entity goes.
fn break_entity(w: &mut World, e: Entity, a: Attached) {
    let kind = &KINDS[usize::from(a.kind)];
    let at = Vec3::new(f64::from(a.pos[0]) + 0.5, f64::from(a.pos[1]) + 0.5, f64::from(a.pos[2]) + 0.5);
    let held = w.get_mut::<Frame>(e).and_then(|mut f| f.item.take());
    w.despawn(e);
    w.resource_mut::<Events>().host.sound(kind.break_sound, [at.x, at.y, at.z], 1.0, 1.0);
    drop_item(w, ItemStack::new(kind.item, 1), at, Vec3::ZERO, DROP_DELAY);
    if let Some(item) = held {
        drop_item(w, *item, at, Vec3::ZERO, DROP_DELAY);
    }
}

/// Block changes wake the hanging entities that hang on the changed cell: each arms its next check.
pub fn attached_wake(w: &mut World) {
    let changed = match w.get_resource::<BlockEvents>() {
        Some(b) if !b.changed.is_empty() => b.changed.clone(),
        _ => return,
    };
    for c in changed {
        let on: Vec<Entity> = w.resource::<AttachIndex>().0.get(&c).cloned().unwrap_or_default();
        for e in on {
            arm(w, e);
        }
    }
}

pub fn insert_resources(w: &mut World) {
    w.insert_resource(AttachIndex::default());
}
pub fn register(s: &mut bevy_ecs::schedule::Schedule) {
    s.add_systems(attached_wake);
}
