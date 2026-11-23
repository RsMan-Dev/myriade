//! The food branch of `PiglinAi.pickUpItem` (`Piglin.pickUpItem`, `Mob` pickup loop): a piglin that changes column takes one food item
//! (`ItemTags.PIGLIN_FOOD`) of its box grown by 1 x 0 x 1, and eats it: `eat` sets `ATE_RECENTLY` for 200 ticks, and while it lasts no
//! food is taken. The item is consumed, not kept. Like `item_pickup`, the look happens when the piglin moves (DOUTE: an item that drifts
//! to a piglin that stands still is not seen). Not ported: the babies' ignored items, the repellents and the admiring refusal of `wantsToPickup`.
use super::*;
use crate::cow::*;
use crate::goals::*;
use crate::item;
use crate::items;
use horizon_core::geometry::Vec3;

/// `ATE_RECENTLY`: the piglin has eaten, until this time.
#[derive(Component, Clone, Copy, Debug)]
pub struct Ate {
    pub until: u64,
}
/// The piglin that takes food from the ground.
#[derive(Component, Clone, Copy, Debug)]
pub struct Eats;

pub static MODULE: Module = Module {
    name: "piglin_eat",
    flags: 0,
    equip,
    fired: never,
    can_use: never,
    tick: Some(tick),
    requires: &[Cap::Body, Cap::Goals],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, _: Inst, _: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Eats);
}
/// A rule, not a goal: nothing starts it.
fn never(_: &mut World, _: Entity, _: Inst) -> bool {
    false
}
fn tick(w: &mut World) {
    crate::goals::tick_state(w, moved);
}
/// The piglins that changed column this tick look at their food.
fn moved(moves: Res<CellMoves>, q: Query<(), With<Eats>>, mut reqs: ResMut<Requests>) {
    for &e in &moves.0 {
        if q.contains(e) {
            reqs.0.push(Req::Call(e, look));
        }
    }
}
/// `wantsToPickup` for food: the first pickable food of the grown box is eaten, unless the piglin ate in the last 200 ticks.
pub fn look(w: &mut World, e: Entity) {
    if w.get::<Ate>(e).is_some_and(|a| now_of(w, e) < a.until) {
        return;
    }
    let Some(b) = w.get::<Bounds>(e).map(|b| b.0) else { return };
    let area = b.inflate(Vec3::new(1.0, 0.0, 1.0));
    for it in item::items_in(w, area) {
        if !item::pickable(w, it) || !item::kind_of(w, it).is_some_and(|n| items::has_tag("minecraft:piglin_food", items::item_id(n))) {
            continue;
        }
        if item::pick_up(w, e, it, 1).is_some() {
            let until = now_of(w, e) + 200;
            w.entity_mut(e).insert(Ate { until });
            return;
        }
    }
}
