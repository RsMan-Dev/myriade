//! Conversion of an animal kept in a place where it cannot live (the zombification of `PiglinAi`, `mobs/piglin.rs:1964-2000`):
//! while `goals::in_unsafe_place` holds, the animal counts `after` ticks of its own time and then becomes `into`
//! (`conversion::convert`, socle absent). The count is dropped when the place turns safe again (`stop`).
use crate::goals;
use super::*;
use crate::rules::convert_timer::{count_over as is_over, until};
use crate::{cow::*, goals::*};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The species it becomes, by name.
    pub into: &'static str,
    /// Ticks in the unsafe place before the conversion.
    pub after: u64,
    /// The carried items fall first (`Piglin.finishConversion`): the admired off-hand item, then the inventory.
    pub drop_carried: bool,
}
impl Params {
    pub const DEFAULT: Self = Self { into: "", after: 300, drop_carried: false };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Convert(pub &'static Params);
/// The count runs: the conversion is due at `until`.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet", on_insert = crate::bridge::view::touched, on_remove = crate::bridge::view::touched)]
pub struct Converting {
    pub until: u64,
}

/// `isImmuneToZombification`: the piglin or hoglin never counts (`IsImmuneToZombification`, synchronized as a flag).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet", on_insert = crate::bridge::view::touched, on_remove = crate::bridge::view::touched)]
pub struct ZombImmune;

pub static MODULE: Module = Module {
    name: "convert_timer",
    flags: 0,
    equip,
    arm,
    can_use,
    start,
    stop,
    requires: &[Cap::Body, Cap::Goals],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Convert(params::<Params>(p)));
}
/// One check at creation: the place a creature lives in does not change while it lives (the host's `UnsafePlace` names a dimension), so no
/// later poll exists; `stop` (place turned safe) is the only exit.
fn arm(w: &mut World, e: Entity, i: Inst) {
    goals::arm(w, e, i.slot, 1);
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    w.get::<Converting>(e).is_none() && w.get::<ZombImmune>(e).is_none() && goals::in_unsafe_place(w, e)
}
fn start(w: &mut World, e: Entity, _: Inst) {
    let after = w.get::<Convert>(e).unwrap().0.after;
    // The first check runs one tick after creation, the `Mob`'s count began with its first tick: one tick is already counted.
    let due = until(now_of(w, e), after);
    w.entity_mut(e).insert(Converting { until: due });
    crate::cow::after(w, e, (after - 1) as i32, count_over);
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Converting>();
}
/// The deadline of a count: the conversion is asked at the selector's end of this tick, unless `stop` dropped the count.
fn count_over(w: &mut World, e: Entity) {
    if w.get::<Converting>(e).is_some_and(|c| is_over(now_of(w, e), c.until)) {
        w.resource_mut::<Requests>().0.push(Req::Call(e, convert_now));
    }
}
fn convert_now(w: &mut World, e: Entity) {
    let (Some(c), Some(cv)) = (w.get::<Converting>(e).copied(), w.get::<Convert>(e).copied()) else { return };
    if is_over(now_of(w, e), c.until) {
        w.entity_mut(e).remove::<Converting>();
        if cv.0.drop_carried {
            drop_carried(w, e);
        }
        crate::conversion::convert_or_ask(w, e, cv.0.into, crate::conversion::ConvertKeep::ALL);
    }
}
/// `Piglin.finishConversion`: `cancelAdmiring` drops the admired off-hand item, then `removeAllItems` drops the inventory, at the feet.
fn drop_carried(w: &mut World, e: Entity) {
    let Some(p) = w.get::<Pos>(e).map(|p| p.0) else { return };
    let at = [p.x, p.y, p.z];
    if w.get::<crate::behaviours::admire::Admiring>(e).is_some() {
        let id = crate::goals::equipped(w, e, crate::goals::Slot::Off);
        if let Some(name) = crate::items::item_name(id) {
            crate::equipment::take_slot(w, e, crate::goals::Slot::Off);
            crate::behaviours::shear::drop_stacks(w, vec![myriade_engine_lib::items::ItemStack::new(name, 1)], at);
        }
    }
    let stacks = crate::item_store::death_stacks(w, e);
    crate::behaviours::shear::drop_stacks(w, stacks, at);
}
/// `ZombieVillager.getConversionProgress`: a tick counts 1. On the 1% of ticks that pass the gate, each bed or iron bars found in the
/// 8-block box around the zombie (the first 14 found) adds 1 at 30%.
pub fn cure_progress(w: &mut World, e: Entity) -> i32 {
    if goals::with_rng(w, e, |r| r.next_f64()) >= 0.01 {
        return 1;
    }
    let Some(p) = w.get::<Pos>(e).map(|p| p.0) else { return 1 };
    let (x0, y0, z0) = (p.x as i32, p.y as i32, p.z as i32);
    let (mut progress, mut found) = (1, 0);
    'scan: for x in x0 - 4..x0 + 4 {
        for y in y0 - 4..y0 + 4 {
            for z in z0 - 4..z0 + 4 {
                if found == 14 {
                    break 'scan;
                }
                if crate::level::has_block_tag(w, "minecraft:speeds_up_zombie_villager_curing", [x, y, z]) {
                    found += 1;
                    if goals::with_rng(w, e, |r| r.next_f64()) < 0.3 {
                        progress += 1;
                    }
                }
            }
        }
    }
    progress
}
