//! `Admire` of the piglin (`mobs/piglins.rs:392-499`, `start_admiring` / `finish_admiring`): an item of the trade held in the
//! off hand is admired for `duration` ticks, with its sound at the start; at the end the item is traded: it is taken and the
//! loot of `loot` (the bartering table) drops at the animal's feet. What wakes it: the item put in the off hand (the item
//! event asks the goal). What ticks it: `tick`, over the admiring animals only.
use super::*;
use crate::{cow::*, equipment, goals::*, item, loot};
use crate::goals::Slot;
use horizon_core::geometry::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The item admired and traded, and the loot table of the trade.
    pub item: &'static str,
    pub loot: &'static str,
    /// Ticks of admiration (`ADMIRE_ITEM_TIME`).
    pub duration: i32,
}
impl Params {
    pub const DEFAULT: Self = Self { item: "minecraft:gold_ingot", loot: "minecraft:gameplay/piglin_bartering", duration: 119 };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Admire(pub Params);
/// The goal runs: the item is admired until `until` (`Clock` ticks).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Admiring {
    pub until: u64,
}

pub static MODULE: Module = Module {
    name: "admire",
    flags: MOVE | LOOK,
    equip,
    can_use,
    start,
    stop,
    tick: Some(tick),
    requires: &[Cap::Body, Cap::Goals, Cap::Head, Cap::Locomotion],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, _: Inst, p: &(dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Admire(*params::<Params>(p)));
}
/// `canUse`: the admired item is in the off hand, and no admiration is under way.
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let Some(a) = w.get::<Admire>(e).copied() else { return false };
    w.get::<Admiring>(e).is_none() && equipment::slot_item(w, e, Slot::OffHand) == Some(a.0.item)
}
fn start(w: &mut World, e: Entity, _: Inst) {
    let a = w.get::<Admire>(e).copied().unwrap();
    let until = now_of(w, e) + a.0.duration as u64;
    w.entity_mut(e).insert(Admiring { until });
    let at = w.get::<Pos>(e).unwrap().0;
    w.resource_mut::<Events>().host.sound("minecraft:entity.piglin.admiring_item", [at.x, at.y, at.z], 1.0, 1.0);
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Admiring>();
}
/// The tick of the module, run while an animal carries it: the admirations that are over end in a trade.
fn tick(w: &mut World) {
    let due: Vec<Entity> = {
        let mut q = w.query::<(Entity, &Admiring)>();
        q.iter(w).filter(|(e, a)| now_of(w, *e) >= a.until).map(|(e, _)| e).collect()
    };
    for e in due {
        trade(w, e);
    }
}
/// `finish_admiring`: the item leaves the off hand, and the loot of the trade drops at the animal's feet (the baby's
/// table is the same one, the baby flag only changes the roll).
fn trade(w: &mut World, e: Entity) {
    let a = w.get::<Admire>(e).copied().unwrap();
    w.entity_mut(e).remove::<Admiring>();
    if equipment::slot_item(w, e, Slot::OffHand) != Some(a.0.item) {
        return;
    }
    equipment::take_slot(w, e, Slot::OffHand);
    let seed = w.get_mut::<Rng>(e).unwrap().next_u32() as i64;
    let baby = w.get::<GrowAt>(e).is_some();
    let at: Vec3 = w.get::<Pos>(e).unwrap().0;
    for stack in loot::roll(w, a.0.loot, false, baby, seed) {
        item::drop_item(w, stack, at, Vec3::new(0.0, 0.3, 0.0), 40);
    }
}
