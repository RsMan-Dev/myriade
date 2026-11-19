//! A sulfur cube that has no block swallows the item it touches when the item's name is on its list (`SulfurCube.swallowItem`,
//! `isSwallowableItem`): event-driven, the item pickup pass reports each touch (`Ev<ItemTouch>`, to be produced by the socle), and the
//! cube holds the item with its archetype (`swallow_hold::hold`). The goal ends at once; no scan of the items.
//! DOUTE : `item_pickup` prend tout objet (pas de filtre) : son filtre doit être ajouté au socle (`item_pickup.rs`, `Params`) ;
//! ici le filtre est appliqué à l'événement seulement.
//! DOUTE : le nom de l'objet et son archétype viennent de l'événement (`ItemTouch.name`, `ItemTouch.archetype`) : à fournir.
use super::swallow_hold::{accepts, hold, may_search, Held};
use super::*;
use crate::cow::*;
use crate::goals::*;

/// The items this species swallows (the `sulfur_cube_swallowable` tag, resolved by the species).
#[derive(Component, Clone, Copy, Debug)]
pub struct Swallowable(pub &'static [&'static str]);

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub items: &'static [&'static str],
}
impl Params {
    pub const DEFAULT: Self = Self { items: &[] };
}

pub static MODULE: Module = Module {
    name: "swallow_item",
    flags: 0,
    equip,
    arm,
    can_use,
    start,
    stop,
    systems,
    requires: &[Cap::Body, Cap::Goals],
    ..Module::DEFAULT
};

fn equip(w: &mut World, e: Entity, _: Inst, p: &(dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Swallowable(params::<Params>(p).items));
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems(touched.in_set(slot));
}
/// An item touched a cube that may search: the cube takes it if the item is on its list.
fn touched(ev: Res<Events>, mut reqs: ResMut<Requests>, q: Query<&Swallowable>, mut commands: Commands) {
    for t in ev.item_touched.iter() {
        if q.get(t.cube).is_ok_and(|s| accepts(&t.name, s.0)) {
            commands.entity(t.cube).insert(Taking { item: t.item, name: t.name.clone() });
            reqs.0.push(Req::Start(t.cube, GoalSlot(0)));
        }
    }
}
/// The item the cube takes this tick.
#[derive(Component, Clone, Debug)]
#[component(storage = "SparseSet")]
pub struct Taking {
    pub item: Entity,
    pub name: std::sync::Arc<str>,
}
fn arm(_: &mut World, _: Entity, _: Inst) {}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    w.get::<Taking>(e).is_some() && may_search(w, e) && w.get::<Held>(e).is_none()
}
/// Swallows the item: the cube holds it, the item is removed from the world, the goal ends.
fn start(w: &mut World, e: Entity, inst: Inst) {
    let Some(t) = w.get::<Taking>(e).cloned() else { return };
    w.entity_mut(e).remove::<Taking>();
    let archetype = horizon_client_lib::mobs::sulfur_cube::archetype_of(&t.name);
    if let Some(a) = archetype {
        hold(w, e, t.name.clone(), a);
    }
    w.resource_mut::<Requests>().0.push(Req::End(e, inst.slot));
}
fn stop(_: &mut World, _: Entity, _: Inst) {}
