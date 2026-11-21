//! `UseItemGoal` of the wandering trader (`mobs/trader.rs` `DrinkItem`): the potion of invisibility once it is dark outside (`dark`), the milk
//! by day while invisible. The drink lasts `ticks`, then the effect is given (milk: all effects are cleared), the drink sound is made and the
//! goal ends with its finish sound (pitch drawn on the creature's stream). The goal takes no flag. Nothing is polled: the day changing and an
//! effect changing wake it, the end is a deadline. Not done: the item in the hand, the dimension with a fixed time (no model of it).
use super::*;
use crate::{cow::*, effects::{self, Kind}, goals::*, hostile::Daylight};
use crate::rules::drink_item::{drink_now, finish_pitch};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The potion drunk in the dark (true), the milk drunk by day (false).
    pub dark: bool,
    pub ticks: i32,
    pub drink_sound: &'static str,
    pub finish_sound: &'static str,
    pub effect_ticks: i32,
}
impl Params {
    pub const POTION: Self = Self { dark: true, ticks: 32, drink_sound: "minecraft:entity.wandering_trader.drink_potion", finish_sound: "minecraft:entity.wandering_trader.disappeared", effect_ticks: 3600 };
    pub const MILK: Self = Self { dark: false, ticks: 32, drink_sound: "minecraft:entity.wandering_trader.drink_milk", finish_sound: "minecraft:entity.wandering_trader.reappeared", effect_ticks: 3600 };
}
/// The entries of the animal (at most two: the potion and the milk), by the rank of the module's entry.
#[derive(Component, Clone, Copy, Debug)]
pub struct Drinks([Option<(u8, &'static Params)>; 2]);
/// The entry that drinks now, and the tick its drink ends.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Drinking {
    pub slot: u8,
    pub until: u64,
}

pub static MODULE: Module = Module {
    name: "drink_item",
    flags: 0,
    equip,
    fired,
    can_use,
    start,
    stop,
    tick: Some(tick),
    requires: &[Cap::Body, Cap::Goals, Cap::Living],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert_if_new(Drinks([None; 2]));
    w.get_mut::<Drinks>(e).unwrap().0[inst.nth as usize] = Some((inst.slot, params::<Params>(p)));
    crate::goals::arm(w, e, inst.slot, 1);
}
fn of(w: &World, e: Entity, inst: Inst) -> &'static Params {
    w.get::<Drinks>(e).unwrap().0[inst.nth as usize].unwrap().1
}
/// `Level.isDarkOutside` / `isBrightOutside` and the effect: the potion in the dark while not invisible, the milk by day while invisible.
fn can_use(w: &mut World, e: Entity, inst: Inst) -> bool {
    let day = w.resource::<Daylight>().0;
    let invisible = effects::has(w, e, Kind::Invisibility);
    drink_now(of(w, e, inst).dark, day, invisible)
}
fn start(w: &mut World, e: Entity, inst: Inst) {
    let p = of(w, e, inst);
    let until = now_of(w, e) + p.ticks as u64;
    w.entity_mut(e).insert(Drinking { slot: inst.slot, until });
    crate::goals::arm(w, e, inst.slot, p.ticks);
}
/// The deadline: the drink is complete (`completeUsingItem`), the goal ends; any other deadline of the entry asks `can_use`.
fn fired(w: &mut World, e: Entity, inst: Inst) -> bool {
    let Some(d) = w.get::<Drinking>(e).copied().filter(|d| d.slot == inst.slot) else { return true };
    if now_of(w, e) < d.until {
        return false;
    }
    let p = of(w, e, inst);
    if p.dark {
        effects::add(w, e, Kind::Invisibility, 0, p.effect_ticks);
    } else {
        let kinds: Vec<Kind> = w.get::<effects::Effects>(e).map(|f| f.list.iter().map(|x| x.kind).collect()).unwrap_or_default();
        for k in kinds {
            effects::remove(w, e, k);
        }
    }
    super::act_phase::sound(w, e, p.drink_sound, 1.0, 1.0);
    super::act_phase::deadline(w, e, &MODULE);
    false
}
fn stop(w: &mut World, e: Entity, inst: Inst) {
    w.entity_mut(e).remove::<Drinking>();
    let p = of(w, e, inst);
    let pitch = finish_pitch(with_rng(w, e, |r| r.next_f64() as f32));
    super::act_phase::sound(w, e, p.finish_sound, 1.0, pitch);
}
fn tick(w: &mut World) {
    crate::goals::tick_state(w, wake);
}
/// The day came or went, or an effect of the creature changed (the potion given, the milk, the expiry): `can_use` is asked again.
fn wake(day: Res<Daylight>, ev: Res<Events>, all: Query<(Entity, &Drinks)>, mut reqs: ResMut<Requests>) {
    if day.is_changed() {
        for (e, d) in &all {
            reqs.0.extend(d.0.iter().flatten().map(|&(slot, _)| Req::Start(e, slot)));
        }
    }
    for e in ev.effect_changed.iter() {
        if let Ok((_, d)) = all.get(e) {
            reqs.0.extend(d.0.iter().flatten().map(|&(slot, _)| Req::Start(e, slot)));
        }
    }
}
