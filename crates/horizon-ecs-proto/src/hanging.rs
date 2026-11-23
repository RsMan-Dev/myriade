//! Hanging entities (`HangingEntity`, `BlockAttachedEntity`): an item frame, a glow frame, a painting, fixed on a face of a block.
//! State: `Attached` (its cell, its face, its kind, its birth in its own time), `Size` (a painting's footprint; a frame is 1 x 1) and
//! the item frame's `Held` (`frame_item.rs`: its item and rotation). Nothing ticks. Vanilla's survival check runs every 101 ticks counted from the birth
//! (`ticksSinceLastCheck` is incremented, then compared with 100 before the test: the 101st tick). It is a deadline of the wheel
//! (`cow::after`), armed when the entity is hung beside one it overlaps, and when a block changes under it (`BlockEvents`). An intact
//! hanging entity costs nothing per tick. The walls and the free space are the host's (`Walls`).
use crate::block_events::BlockEvents;
use crate::cow::*;
use crate::host_out_frames::Spot;
use crate::rules::explosion::distance_share;
use crate::targeting::Who;
use crate::goals::Events;
pub use crate::rules::hanging::CHECK_PERIOD;
use crate::rules::hanging::{cells, check_delay, left, step, support};
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;
use std::collections::HashMap;

pub const ITEM_FRAME: u8 = 0;
pub const GLOW_ITEM_FRAME: u8 = 1;
pub const PAINTING: u8 = 2;

/// What a kind drops when it breaks, and its break sound.
pub struct Kind {
    pub item: &'static str,
    pub break_sound: &'static str,
}
pub const KINDS: &[Kind] = &[
    Kind { item: crate::species::lists::ITEM_FRAME, break_sound: "entity.item_frame.break" },
    Kind { item: crate::species::lists::GLOW_ITEM_FRAME, break_sound: "entity.item_frame.break" },
    Kind { item: crate::species::lists::PAINTING, break_sound: "entity.painting.break" },
];

/// A hanging entity: the cell it occupies, the face it hangs on (`Direction.get3DDataValue`: down 0, up 1, north 2, south 3,
/// west 4, east 5), its kind and its birth (its own time).
#[derive(Component, Clone, Copy, Debug)]
#[component(on_add = arrived, on_remove = leave_index)]
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
/// The hanging entities by the cell of the block they hang on: the wall that decides whether they stay.
#[derive(Resource, Default)]
pub struct AttachIndex(HashMap<[i32; 3], Vec<Entity>>);

/// The host's answer for the wall: `(world, pos, face, size) -> true` when the block behind the footprint holds the face and the
/// footprint is free of solid blocks. Absent: the wall is taken to hold.
#[derive(Resource)]
pub struct Walls(pub fn(&World, [i32; 3], u8, (u8, u8)) -> bool);

/// When an entity joined the level (`JoinSeq`): taken as it is hung, or else when it is made (`cow::TypeName`). The host tells the mods of it in that order.
#[derive(Component, Clone, Copy, Debug)]
pub struct JoinedAt(pub myriade_engine_lib::join_seq::JoinSeq);

fn arrived(mut world: bevy_ecs::world::DeferredWorld, ctx: bevy_ecs::lifecycle::HookContext) {
    let at = JoinedAt(myriade_engine_lib::join_seq::JoinSeq::next());
    world.commands().entity(ctx.entity).insert(at);
}

fn leave_index(mut world: bevy_ecs::world::DeferredWorld, ctx: bevy_ecs::lifecycle::HookContext) {
    let Some(a) = world.get::<Attached>(ctx.entity).copied() else { return };
    let key = support(a.pos, a.face);
    if let Some(list) = world.resource_mut::<AttachIndex>().0.get_mut(&key) {
        list.retain(|&e| e != ctx.entity);
    }
}

fn size_of(w: &World, e: Entity) -> (u8, u8) {
    w.get::<Size>(e).map_or((1, 1), |s| (s.width, s.height))
}

/// The box of a hanging entity (`ItemFrame.createBoundingBox`, `Painting.calculateBoundingBox`): 1/16 thick, 0.46875 out from the cell's
/// centre; a painting is its width by its height, its centre shifted along its left when its width is even.
pub fn bounds_of(a: &Attached, size: Option<(u8, u8)>) -> horizon_core::geometry::Aabb {
    let (s, l) = (step(a.face).map(f64::from), left(a.face).map(f64::from));
    let (w, h) = size.map_or((1.0, 1.0), |(w, h)| (f64::from(w), f64::from(h)));
    let shift = |n: f64| if n % 2.0 == 0.0 { 0.5 } else { 0.0 };
    let p = a.pos.map(f64::from);
    let c = [p[0] + 0.5 - 0.46875 * s[0] + l[0] * shift(w), p[1] + 0.5 + shift(h), p[2] + 0.5 - 0.46875 * s[2] + l[2] * shift(w)];
    let sz = match size {
        Some(_) => [if s[0] != 0.0 { 0.0625 } else { w }, h, if s[2] != 0.0 { 0.0625 } else { w }],
        None => [0, 1, 2].map(|i| if s[i] != 0.0 { 0.0625 } else { 0.75 }),
    };
    horizon_core::geometry::Aabb::new(
        Vec3::new(c[0] - sz[0] / 2.0, c[1] - sz[1] / 2.0, c[2] - sz[2] / 2.0),
        Vec3::new(c[0] + sz[0] / 2.0, c[1] + sz[1] / 2.0, c[2] + sz[2] / 2.0),
    )
}

impl AttachIndex {
    /// The hanging entities that hang on a block within one cell of `area` (their box may reach into it).
    pub fn near(&self, area: horizon_core::geometry::Aabb) -> Vec<Entity> {
        let (lo, hi) = (area.min_position(), area.max_position());
        let (a, b) = ([lo.x, lo.y, lo.z].map(|v| v.floor() as i32 - 1), [hi.x, hi.y, hi.z].map(|v| v.floor() as i32 + 1));
        let mut found = Vec::new();
        for x in a[0]..=b[0] {
            for y in a[1]..=b[1] {
                for z in a[2]..=b[2] {
                    found.extend(self.0.get(&[x, y, z]).into_iter().flatten().copied());
                }
            }
        }
        found
    }
}

/// Hangs an entity of `kind` at `pos` on `face`. Its survival check is armed when it overlaps another hanging entity (the one
/// created and the ones it overlaps, which vanilla checks in the same way).
pub fn hang(w: &mut World, pos: [i32; 3], face: u8, kind: u8, size: Option<Size>) -> Entity {
    let at = Vec3::new(f64::from(pos[0]) + 0.5, f64::from(pos[1]) + 0.5, f64::from(pos[2]) + 0.5);
    let e = w.spawn((crate::cow::Pos(at), OwnTime, Attached { pos, face, kind, born: 0 })).id();
    let id = KINDS[usize::from(kind)].item;
    w.entity_mut(e).insert(crate::cow::TypeName(id));
    crate::nbt::mark_saved(w, e, id);
    let born = now_of(w, e);
    w.get_mut::<Attached>(e).expect("just spawned").born = born;
    if let Some(s) = size {
        w.entity_mut(e).insert(s);
    }
    w.resource_mut::<AttachIndex>().0.entry(support(pos, face)).or_default().push(e);
    let overlapped = overlapping(w, e);
    if !overlapped.is_empty() {
        // The older ones first: vanilla ticks entities in the order they were added, so the older breaks and the newer then stays.
        for o in overlapped {
            arm(w, o);
        }
        arm(w, e);
    }
    e
}

/// `setPos` and `setDirection` of a hanging entity: it moves to `pos` on `face` when the wall of its footprint holds there, and its support
/// index follows. False when the wall does not hold, or `e` hangs on nothing.
pub fn set_placement(w: &mut World, e: Entity, pos: [i32; 3], face: u8) -> bool {
    if w.get::<Attached>(e).is_none() {
        return false;
    }
    let size = size_of(w, e);
    if !w.get_resource::<Walls>().map_or(true, |f| (f.0)(w, pos, face, size)) {
        return false;
    }
    relocate(w, e, pos, face);
    true
}

/// `BlockAttachedEntity.setPos`: the entity takes the cell `pos` (the block holding the requested position), its face kept, with no wall
/// check (the survival check breaks it later when nothing holds it). False when `e` hangs on nothing.
pub fn move_to(w: &mut World, e: Entity, pos: [i32; 3]) -> bool {
    let Some(a) = w.get::<Attached>(e).copied() else { return false };
    relocate(w, e, pos, a.face);
    true
}

fn relocate(w: &mut World, e: Entity, pos: [i32; 3], face: u8) {
    let Some(a) = w.get::<Attached>(e).copied() else { return };
    let index = &mut w.resource_mut::<AttachIndex>().0;
    if let Some(list) = index.get_mut(&support(a.pos, a.face)) {
        list.retain(|&o| o != e);
    }
    index.entry(support(pos, face)).or_default().push(e);
    if let Some(mut at) = w.get_mut::<Attached>(e) {
        (at.pos, at.face) = (pos, face);
    }
    let centre = Vec3::new(f64::from(pos[0]) + 0.5, f64::from(pos[1]) + 0.5, f64::from(pos[2]) + 0.5);
    w.entity_mut(e).insert(crate::cow::Pos(centre));
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

/// The hanging entities on `face` of the air cell `pos`, read from the index of the wall they hang on.
pub fn at_cell(w: &World, pos: [i32; 3], face: u8) -> Vec<Entity> {
    let Some(list) = w.get_resource::<AttachIndex>().and_then(|i| i.0.get(&support(pos, face))) else { return Vec::new() };
    list.iter().copied().filter(|&e| w.get::<Attached>(e).is_some_and(|a| a.pos == pos && a.face == face)).collect()
}

/// The first entity `want` accepts that the ray `eye + dir * t` (`dir` a unit vector, `t` in 0..=reach) crosses, with `t`. The ray
/// is sampled by 1/16 block and only the cells it crosses are read: no pass over all the entities.
pub fn aimed(w: &World, eye: [f64; 3], dir: [f64; 3], reach: f64, want: fn(&World, Entity) -> bool) -> Option<(Entity, f64)> {
    const STEP: f64 = 1.0 / 16.0;
    let mut last = None;
    for k in 0..=(reach / STEP) as u32 {
        let t = f64::from(k) * STEP;
        let cell = [0, 1, 2].map(|i| (eye[i] + dir[i] * t).floor() as i32);
        if last == Some(cell) {
            continue;
        }
        last = Some(cell);
        for face in 0..6 {
            for e in at_cell(w, cell, face) {
                if want(w, e) {
                    return Some((e, t));
                }
            }
        }
    }
    None
}

/// `Explosion` on the hanging entities (`BlockAttachedEntity.ignoreExplosion`, `hurtServer`): within the blast's diameter, one breaks when
/// the blast breaks blocks, a fixed frame stays (`canHurtWhenFixed`). Read from the index, once per blast.
pub fn blast(w: &mut World, at: Vec3, radius: f32, who: Option<Who>, breaks_blocks: bool) {
    if !breaks_blocks {
        return;
    }
    let diameter = f64::from(radius * 2.0);
    let found: Vec<Entity> = w.get_resource::<AttachIndex>().map(|i| i.0.values().flatten().copied().collect()).unwrap_or_default();
    for e in found {
        let (Some(a), Some(feet)) = (w.get::<Attached>(e).copied(), w.get::<Pos>(e).map(|p| p.0)) else { continue };
        if distance_share(feet, at, diameter) > 1.0 {
            continue;
        }
        if a.kind == PAINTING {
            crate::painting::break_out(w, e, who, false);
        } else if !w.get::<crate::frame_item::Fixed>(e).is_some_and(|f| f.0) {
            let spot = Spot { at: a.pos, face: a.face, glow: a.kind == GLOW_ITEM_FRAME };
            crate::frame_item::break_out(w, e, spot, who, false);
        }
    }
}

/// Arms the entity's next survival check.
pub fn arm(w: &mut World, e: Entity) {
    let Some(a) = w.get::<Attached>(e).copied() else { return };
    let now = now_of(w, e);
    after(w, e, check_delay(a.born, now), check);
}

/// The hanging entities on `face` whose footprint holds `c`. A footprint spans from one cell before its anchor to two after
/// (`rules::hanging::span`), so the anchors read are those within two cells of `c`, each from the index.
fn covering(w: &World, c: [i32; 3], face: u8) -> Vec<Entity> {
    let l = left(face);
    let mut found = Vec::new();
    for j in -2..=2 {
        for k in -2..=2 {
            let p: [i32; 3] = std::array::from_fn(|i| c[i] - j * l[i] - k * i32::from(i == 1));
            for e in at_cell(w, p, face) {
                if cells(p, face, size_of(w, e)).contains(&c) && !found.contains(&e) {
                    found.push(e);
                }
            }
        }
    }
    found
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

/// `dropItem(level, null)`: the sound, then the entity goes and its item falls (`frame_item::break_out`, `painting::break_out`).
fn break_entity(w: &mut World, e: Entity, a: Attached) {
    let kind = &KINDS[usize::from(a.kind)];
    let at = Vec3::new(f64::from(a.pos[0]) + 0.5, f64::from(a.pos[1]) + 0.5, f64::from(a.pos[2]) + 0.5);
    w.resource_mut::<Events>().host.sound(kind.break_sound, [at.x, at.y, at.z], 1.0, 1.0);
    if a.kind == PAINTING {
        crate::painting::break_out(w, e, None, false);
    } else {
        let spot = Spot { at: a.pos, face: a.face, glow: a.kind == GLOW_ITEM_FRAME };
        crate::frame_item::break_out(w, e, spot, None, false);
    }
}

/// Block changes wake the hanging entities whose wall or footprint is the changed cell: each arms its next check. Read per changed
/// cell from the index, never per tick.
pub fn attached_wake(w: &mut World) {
    let changed = match w.get_resource::<BlockEvents>() {
        Some(b) if !b.changed.is_empty() => b.changed.clone(),
        _ => return,
    };
    for c in changed {
        let mut on: Vec<Entity> = Vec::new();
        for face in 0..6 {
            let s = step(face);
            let front: [i32; 3] = std::array::from_fn(|i| c[i] + s[i]);
            for e in covering(w, c, face).into_iter().chain(covering(w, front, face)) {
                if !on.contains(&e) {
                    on.push(e);
                }
            }
        }
        for e in on {
            arm(w, e);
        }
    }
}

pub fn insert_resources(w: &mut World) {
    w.insert_resource(AttachIndex::default());
}
pub fn register(s: &mut bevy_ecs::schedule::Schedule) {
    s.add_systems((attached_wake.after(crate::cow::block_changes), crate::block_events::drain_block_events.after(attached_wake)));
}
