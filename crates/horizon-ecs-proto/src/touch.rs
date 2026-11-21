//! Touches (`SulfurCube.mob_contacts`, `swallow`): when an animal or an item changes column (`CellMoves`, read before its clear),
//! the pairs whose boxes meet are pushed into `Events.touched` (toucher, other creature) or `Events.item_touched` (toucher, item).
//! A mover with a `Toucher` looks at its neighbours; a mover without one is looked at by the `Toucher`s near it. Nothing scans
//! every `Toucher`, and an entity without one costs nothing.
use crate::cow::*;
use crate::goals::*;
use crate::item::{self, Item, ItemIndex};
use crate::rules::touch::reach_box;
use bevy_ecs::prelude::*;
use horizon_core::geometry::Aabb;

/// The box of this entity, grown by `reach` (x, y, z), is where the entities that touch it are looked for.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Toucher {
    pub reach: [f64; 3],
}


/// The box of an animal, or of an item (its box on the ground).
fn box_of(e: Entity, bounds: &Query<&Bounds>, poses: &Query<&Pos>) -> Option<Aabb> {
    if let Ok(b) = bounds.get(e) {
        return Some(b.0);
    }
    poses.get(e).ok().map(|p| item::box_at(p.0))
}
fn push_once(pairs: &mut Vec<(Entity, Entity)>, pair: (Entity, Entity)) {
    if !pairs.contains(&pair) {
        pairs.push(pair);
    }
}
pub(crate) fn touch_events(
    moves: Res<CellMoves>,
    index: Res<Index>,
    items: Res<ItemIndex>,
    bounds: Query<&Bounds>,
    poses: Query<&Pos>,
    touchers: Query<&Toucher>,
    stacks: Query<(), With<Item>>,
    mut ev: ResMut<Events>,
) {
    // No toucher, no touch: a mover is looked at only by touchers (the sparse set is empty at once).
    if touchers.is_empty() {
        return;
    }
    let mut creatures: Vec<(Entity, Entity)> = Vec::new();
    let mut objects: Vec<(Entity, Entity)> = Vec::new();
    for &e in &moves.0 {
        let (Some(b), Ok(p)) = (box_of(e, &bounds, &poses), poses.get(e)) else { continue };
        if let Ok(t) = touchers.get(e) {
            // (a) the mover is a toucher: the neighbours whose box meets its grown box.
            let area = reach_box(b, t.reach);
            index.for_radius(p.0.x, p.0.z, t.reach[0] + 2.0, |o| {
                if o != e && bounds.get(o).is_ok_and(|ob| ob.0.intersects(area)) {
                    push_once(&mut creatures, (e, o));
                }
            });
            for o in items.near(area) {
                if box_of(o, &bounds, &poses).is_some_and(|ob| ob.intersects(area)) {
                    push_once(&mut objects, (e, o));
                }
            }
        } else {
            // (b) the mover is looked at by the touchers near it.
            let is_object = stacks.contains(e);
            index.for_radius(p.0.x, p.0.z, crate::goals::SCAN, |o| {
                let Ok(t) = touchers.get(o) else { return };
                let Some(ob) = bounds.get(o).ok().map(|x| x.0) else { return };
                let area = reach_box(ob, t.reach);
                if area.intersects(b) {
                    push_once(if is_object { &mut objects } else { &mut creatures }, (o, e));
                }
            });
        }
    }
    for pair in creatures {
        ev.touched.push(pair);
    }
    for pair in objects {
        ev.item_touched.push(pair);
    }
}
