//! Blocks that react to an entity inside (`entityInside`), driven by the mover. An entity that triggers blocks and moves (its `Bounds`
//! changed, or the block beside it did: `block_changes` marks them) reports the cells its box reaches into whose block is in the
//! `Sensitive` table; a resting entity reports nothing and costs nothing. What keeps a plate pressed is the block's own scheduled tick,
//! which asks the zone queries below (read from the spatial index, never a walk over every entity).
//! Jar facts: `Entity.checkInsideBlocks` deflates the box by 1e-5 (as `f32`); `isIgnoringBlockTriggers` is true for Display, Interaction,
//! Marker, OminousItemSpawner, Bat and ArmorStand (`IgnoresTriggers`); a wooden button counts `AbstractArrow`s, which include tridents.
use crate::boat::Boat;
use crate::cart_state::Cart;
use crate::cow::{Bounds, Contact, Health, Index, Level, Pos, reindex};
use crate::fishing_hook::{Hook, SIZE as HOOK_SIZE};
use crate::frame_item::Held;
use crate::host_out_inside::{InsideEvent, InsideOut};
use crate::projectile::{Flight, Flying, Projectile};
use crate::stand::Stand;
use crate::trident::{Trident, TridentFlying};
use bevy_ecs::prelude::*;
use horizon_core::geometry::Aabb;
use std::collections::HashSet;
use std::sync::Arc;

/// `Entity.checkInsideBlocks`: the deflation of the box (`1.0E-5F`).
const DEFLATE: f64 = 1.0e-5_f32 as f64;

/// The states whose block reacts to an entity inside, as a bit per state id: built once by the host, shared by reference.
#[derive(Resource, Default)]
pub struct Sensitive(pub Option<Arc<[u64]>>);

impl Sensitive {
    pub fn has(&self, state: u32) -> bool {
        self.0.as_ref().is_some_and(|t| t.get(state as usize / 64).is_some_and(|w| w >> (state % 64) & 1 == 1))
    }
}

/// `Entity.isIgnoringBlockTriggers`: it never triggers a block. Sparse: only the kinds above carry it.
#[derive(Component, Clone, Copy, Debug, Default)]
#[component(storage = "SparseSet")]
pub struct IgnoresTriggers;

pub fn insert_resources(w: &mut World) {
    w.insert_resource(Sensitive::default());
    w.insert_resource(InsideOut::default());
}

pub fn register(s: &mut bevy_ecs::schedule::Schedule) {
    s.add_systems((moved_inside, shot_inside, hooks_inside).after(reindex).run_if(armed));
}

fn armed(t: Res<Sensitive>) -> bool {
    t.0.is_some()
}

/// The cells `b` reaches into, deflated, that are in the table, in x, y, z order.
fn sensitive_cells(b: &Aabb, level: &Level, table: &Sensitive) -> Vec<[i32; 3]> {
    let (lo, hi) = (b.min_position(), b.max_position());
    let world = &level.mw.0.world;
    let mut cells = Vec::new();
    for x in (lo.x + DEFLATE).floor() as i32..=(hi.x - DEFLATE).floor() as i32 {
        for y in (lo.y + DEFLATE).floor() as i32..=(hi.y - DEFLATE).floor() as i32 {
            for z in (lo.z + DEFLATE).floor() as i32..=(hi.z - DEFLATE).floor() as i32 {
                if table.has(world.block(x, y, z).0) {
                    cells.push([x, y, z]);
                }
            }
        }
    }
    cells
}

/// Queues the sensitive cells `b` reaches into (nothing when none), with the feet and the ground flag the big dripleaf asks.
fn report(out: &mut InsideOut, level: &Level, table: &Sensitive, entity: Entity, boat: bool, b: &Aabb, on_ground: bool) {
    let cells = sensitive_cells(b, level, table);
    if cells.is_empty() {
        return;
    }
    let (lo, hi) = (b.min_position(), b.max_position());
    let at = [(lo.x + hi.x) / 2.0, lo.y, (lo.z + hi.z) / 2.0];
    out.0.push(InsideEvent { entity, boat, cells, at, half: (hi.x - lo.x) / 2.0, on_ground });
}

/// The creatures, items, carts, boats... that moved: their box changed this tick.
fn moved_inside(
    table: Res<Sensitive>,
    level: Res<Level>,
    mut out: ResMut<InsideOut>,
    q: Query<(Entity, &Bounds, Has<Boat>, Option<&Contact>, Option<&Stand>), (Changed<Bounds>, Without<IgnoresTriggers>)>,
) {
    for (e, b, boat, c, stand) in &q {
        // `ArmorStand.isIgnoringBlockTriggers` is `isMarker`: the flag changes, so it is read here.
        if stand.is_some_and(|s| s.flags & crate::stand::MARKER != 0) {
            continue;
        }
        report(&mut out, &level, &table, e, boat, &b.0, c.is_some_and(|c| c.on_ground));
    }
}

/// The projectiles in flight, and the ones that stopped this tick (their last move counts).
#[allow(clippy::type_complexity)]
fn shot_inside(
    table: Res<Sensitive>,
    level: Res<Level>,
    mut out: ResMut<InsideOut>,
    flying: Query<(Entity, &Pos, &Flight), (With<Flying>, Without<Hook>, Without<IgnoresTriggers>)>,
    planes: Query<(Entity, &Pos), (With<Trident>, With<TridentFlying>, Without<Flight>)>,
    stopped: Query<(&Pos, Option<&Flight>), With<Projectile>>,
    mut landed: (RemovedComponents<Flying>, RemovedComponents<TridentFlying>),
) {
    let size_of = |f: Option<&Flight>| f.map_or(Flight::TRIDENT.size, |f| f.size);
    for (e, p, f) in &flying {
        report(&mut out, &level, &table, e, false, &crate::arrow_more::arrow_box(p.0, f.size), false);
    }
    for (e, p) in &planes {
        report(&mut out, &level, &table, e, false, &crate::arrow_more::arrow_box(p.0, Flight::TRIDENT.size), false);
    }
    for e in landed.0.read().chain(landed.1.read()) {
        if let Ok((p, f)) = stopped.get(e) {
            report(&mut out, &level, &table, e, false, &crate::arrow_more::arrow_box(p.0, size_of(f)), false);
        }
    }
}

fn hooks_inside(table: Res<Sensitive>, level: Res<Level>, mut out: ResMut<InsideOut>, q: Query<(Entity, &Pos), (With<Hook>, Changed<Pos>)>) {
    for (e, p) in &q {
        report(&mut out, &level, &table, e, false, &crate::arrow_more::arrow_box(p.0, HOOK_SIZE), false);
    }
}

/// The indexed entities whose box meets `area` and that `keep` accepts. The index holds the entities by 2-block column of their box's centre
/// (a box at most 2 wide), read from the columns the area spans; the boxes are then tested for real.
fn indexed_in(w: &World, area: Aabb, mut keep: impl FnMut(Entity) -> bool) -> Vec<(Entity, Aabb)> {
    let Some(index) = w.get_resource::<Index>() else { return Vec::new() };
    let (lo, hi) = (area.min_position(), area.max_position());
    let (mut seen, mut out) = (HashSet::new(), Vec::new());
    let steps = |a: f64, b: f64| (0..=((b - a).max(0.0).ceil() as u32)).map(move |i| (a + f64::from(i)).min(b));
    for x in steps(lo.x, hi.x) {
        for z in steps(lo.z, hi.z) {
            index.for_near_touching(x, z, (lo.x, lo.z), (hi.x, hi.z), |e| {
                if let Some(b) = w.get::<Bounds>(e)
                    && b.0.intersects(area)
                    && seen.insert(e)
                    && keep(e)
                {
                    out.push((e, b.0));
                }
            });
        }
    }
    out
}

/// The boxes of the projectiles that stick (arrows, spectral arrows: `Flight.sticks`) and of the tridents, meeting `area`: they are not in
/// the index, so this reads the `Projectile` marker, bounded by the projectiles alive; call it from a block's tick, never per tick.
pub fn arrows_in(w: &World, area: Aabb) -> Vec<Entity> {
    let mut out = Vec::new();
    if let Some(mut q) = w.try_query_filtered::<(Entity, &Pos, &Flight), (With<Projectile>, Without<Hook>)>() {
        out.extend(q.iter(w).filter(|(_, p, f)| f.sticks && crate::arrow_more::arrow_box(p.0, f.size).intersects(area)).map(|(e, _, _)| e));
    }
    if let Some(mut q) = w.try_query_filtered::<(Entity, &Pos), (With<Trident>, Without<Flight>)>() {
        out.extend(q.iter(w).filter(|(_, p)| crate::arrow_more::arrow_box(p.0, Flight::TRIDENT.size).intersects(area)).map(|(e, _)| e));
    }
    out
}

/// The entities that press plates and tripwires in `area` (`getEntitiesOfClass(Entity or LivingEntity, ..., !isIgnoringBlockTriggers)`):
/// the indexed ones, and for `mobs_only == false` the arrows, other projectiles and hooks too. Players are the host's.
pub fn triggers_in(w: &World, area: Aabb, mobs_only: bool) -> Vec<Entity> {
    let mut out: Vec<Entity> = indexed_in(w, area, |e| w.get::<IgnoresTriggers>(e).is_none() && !w.get::<crate::stand::Stand>(e).is_some_and(|s| s.flags & crate::stand::MARKER != 0) && (!mobs_only || w.get::<Health>(e).is_some()))
        .into_iter()
        .map(|(e, _)| e)
        .collect();
    if !mobs_only {
        if let Some(mut q) = w.try_query_filtered::<(Entity, &Pos, &Flight), (With<Projectile>, Without<Hook>, Without<IgnoresTriggers>)>() {
            out.extend(q.iter(w).filter(|(_, p, f)| crate::arrow_more::arrow_box(p.0, f.size).intersects(area)).map(|(e, _, _)| e));
        }
        if let Some(mut q) = w.try_query_filtered::<(Entity, &Pos), (With<Trident>, Without<Flight>, Without<IgnoresTriggers>)>() {
            out.extend(q.iter(w).filter(|(_, p)| crate::arrow_more::arrow_box(p.0, Flight::TRIDENT.size).intersects(area)).map(|(e, _)| e));
        }
        out.extend(hook_boxes(w, area).into_iter().map(|(e, _)| e));
    }
    out
}

fn hook_boxes(w: &World, area: Aabb) -> Vec<(Entity, Aabb)> {
    let Some(mut q) = w.try_query_filtered::<(Entity, &Pos), With<Hook>>() else { return Vec::new() };
    q.iter(w).map(|(e, p)| (e, crate::arrow_more::arrow_box(p.0, HOOK_SIZE))).filter(|(_, b)| b.intersects(area)).collect()
}

/// The boxes of the fishing hooks meeting `area`, and whether each triggers blocks (all do: no hook ignores them).
pub fn hooks_in(w: &World, area: Aabb) -> Vec<(Aabb, bool)> {
    hook_boxes(w, area).into_iter().map(|(_, b)| (b, true)).collect()
}

/// The boxes of the minecarts meeting `area`.
pub fn cart_boxes_in(w: &World, area: Aabb) -> Vec<Aabb> {
    indexed_in(w, area, |e| w.get::<Cart>(e).is_some()).into_iter().map(|(_, b)| b).collect()
}

/// The boxes of the boats meeting `area`, by the spatial index.
pub fn boat_boxes_in(w: &World, area: Aabb) -> Vec<Aabb> {
    indexed_in(w, area, |e| w.get::<crate::boat_state::BoatMarker>(e).is_some()).into_iter().map(|(_, b)| b).collect()
}

/// The item frames hanging in the cell `pos` on `face` (`Direction.get3DDataValue`): `(holds an item, rotation)` of each.
pub fn frames_at(w: &World, pos: [i32; 3], face: u8) -> Vec<(bool, u8)> {
    crate::hanging::at_cell(w, pos, face).into_iter().filter_map(|e| w.get::<Held>(e)).map(|h| (h.stack.is_some(), h.rotation)).collect()
}
