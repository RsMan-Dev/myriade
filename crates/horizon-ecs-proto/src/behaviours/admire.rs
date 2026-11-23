//! `Admire` of the piglin (`mobs/piglins.rs:392-499`, `start_admiring` / `finish_admiring`): an item of the trade held in the
//! off hand is admired for `duration` ticks, with its sound at the start; at the end the item is traded: it is taken and the
//! loot of `loot` (the bartering table) drops at the animal's feet. What wakes it: the item put in the off hand (the item
//! event asks the goal). What ticks it: `tick`, over the admiring animals only.
use super::*;
use crate::{cow::*, equipment, goals::*, loot};
use crate::goals::Slot;
use crate::rules::admire;

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
pub struct Admire(pub &'static Params);
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
    arm,
    can_use,
    start,
    stop,
    tick: Some(tick),
    interact,
    requires: &[Cap::Body, Cap::Goals, Cap::Head, Cap::Locomotion],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Admire(params::<Params>(p)));
}
/// Asked again (the equipment changed, the animal entered the zone): the goal is tried one tick later.
fn arm(w: &mut World, e: Entity, inst: Inst) {
    if w.get::<Admire>(e).is_some() {
        crate::goals::arm(w, e, inst.slot, 1);
    }
}
/// `canUse`: the admired item is in the off hand, and no admiration is under way.
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let Some(a) = w.get::<Admire>(e).copied() else { return false };
    w.get::<Admiring>(e).is_none() && crate::goals::equipped(w, e, Slot::Off) == crate::items::item_id(a.0.item)
}
fn start(w: &mut World, e: Entity, _: Inst) {
    let a = w.get::<Admire>(e).copied().unwrap();
    let until = admire::until(now_of(w, e), a.0.duration);
    w.entity_mut(e).insert(Admiring { until });
    let at = w.get::<Pos>(e).unwrap().0;
    w.resource_mut::<Events>().host.sound("minecraft:entity.piglin.admiring_item", [at.x, at.y, at.z], 1.0, 1.0);
}
/// `PiglinAi.mobInteract`: a player's click with the admired item, on an adult whose off hand is empty and that admires nothing: one item goes to
/// its off hand (the goal then starts), the player's stack loses it. The jar (`PiglinAi.canAdmire`, `wasHurtBy`): refused for a baby, while admiring, and for 400 ticks after a player hit it; combat and a finished trade disable nothing. The item already in the off hand is thrown out (its stack count is not kept: one is thrown). DOUTE: the `Admiring` start on a hit (the off hand dropped, the admiring stopped) is not done.
fn interact(w: &mut World, e: Entity, _: Inst, i: &Interaction) -> bool {
    let Some(a) = w.get::<Admire>(e).copied() else { return false };
    if i.item == 0 || i.item != crate::items::item_id(a.0.item) || w.get::<GrowAt>(e).is_some() || w.get::<Admiring>(e).is_some() {
        return false;
    }
    // `canAdmire`: not while admiring is disabled (400 ticks after a player hit it). `holdInOffhand`: what the off hand held is thrown first.
    if crate::behaviours::hurt_by::hit_by_player_within(w, e, 400) {
        return false;
    }
    let held = crate::goals::equipped(w, e, Slot::Off);
    if held != 0 {
        equipment::take_slot(w, e, Slot::Off);
        let at = w.get::<Pos>(e).map_or([0.0; 3], |q| [q.0.x, q.0.y, q.0.z]);
        if let Some(name) = crate::items::item_name(held) {
            crate::behaviours::shear::drop_stacks(w, vec![myriade_engine_lib::items::ItemStack::new(name, 1)], at);
        }
    }
    equipment::set_slot(w, e, Slot::Off, i.item);
    crate::give_item::take_from_hand(w, i);
    true
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
    if crate::goals::equipped(w, e, Slot::Off) != crate::items::item_id(a.0.item) {
        return;
    }
    equipment::take_slot(w, e, Slot::Off);
    let seed = w.get_mut::<Rng>(e).unwrap().next_u32() as i64;
    let baby = w.get::<GrowAt>(e).is_some();
    let stacks = loot::roll(w, a.0.loot, false, baby, seed);
    crate::gift_drop::throw_toward_player_or_nearby(w, e, stacks, 16.0, 4, 2);
}
