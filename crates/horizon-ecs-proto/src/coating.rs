//! The rule `COAT`: a weathering coating (`CopperGolem`). A player's click: wax (honeycomb) seals the stage, an axe scrapes the wax off, else
//! steps the weathering back by one (both reset the clock of the weathering), the shears are `behaviours::shear`'s. The weathering over time is a chain of deadlines of the wheel (`after`), never a tick:
//! the first `period` ticks after creation, each next one the same after the last deadline. An entity that reached the last stage tries to
//! turn into a block (`Statue`) at the odds of one draw per tick (sampled once; the block under it must be air then): the host places it
//! (`BecomeBlock`, which carries the lead). The state is the rare component `Coat` (sparse), saved as `weather_state` and `next_weather_age`
//! (game time; -2 waxed, 0 oxidized) like the jar does.
//! Not modelled: the clock of a dormant entity (its own time stops, the game's goes on).
use crate::cow::*;
use crate::goals::*;
use crate::nbt::{accept, owner, Field};
use crate::rules::{no_attrs, params, Rule};
use bevy_ecs::prelude::*;
use myriade_engine_lib::items::ItemStack;
use horizon_core::geometry::Vec3;
use horizon_core::nbt::{Compound, Tag};
use std::borrow::Cow;

const STATES: [&str; 4] = ["unaffected", "exposed", "weathered", "oxidized"];
const WAXED: i64 = -2;
const UNSET: i64 = -1;

pub const FIELDS: &[Field] = &[
    Field { key: Cow::Borrowed("weather_state"), write: write_state, read: read_state, accepts: accept::string, owner: Some(owner::<Coat>) },
    Field { key: Cow::Borrowed("next_weather_age"), write: write_age, read: read_age, accepts: accept::long, owner: Some(owner::<Coat>) },
];

fn write_state(w: &World, e: Entity, c: &mut Compound) {
    if let Some(k) = w.get::<Coat>(e) {
        c.insert("weather_state".into(), Tag::String(STATES[usize::from(k.stage.min(LAST_STAGE))].into()));
    }
}
fn write_age(w: &World, e: Entity, c: &mut Compound) {
    let Some(k) = w.get::<Coat>(e) else { return };
    let age = if k.waxed {
        WAXED
    } else if k.stage >= LAST_STAGE || k.due == 0 {
        0
    } else {
        game_time_at(w, e, k.due as i64)
    };
    c.insert("next_weather_age".into(), Tag::Long(age));
}
/// The stage; an oxidized one starts its tries of the block (one draw per tick, by the wait of the first success).
fn read_state(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::String(s) = t else { return false };
    let Some(stage) = STATES.iter().position(|n| *n == s.to_string_lossy()) else { return false };
    let (Some(mut k), Some(CoatRule(p))) = (w.get::<Coat>(e).copied(), w.get::<CoatRule>(e).copied()) else { return false };
    k.stage = stage as u8;
    w.entity_mut(e).insert(k);
    if k.stage == LAST_STAGE && !k.waxed {
        match p.statue {
            Some(s) => {
                let wait = 1 + with_rng(w, e, |r| wait_for(r.next_f64(), s.chance));
                arm_deadline(w, e, k, wait);
            }
            None => {
                w.entity_mut(e).insert(Coat { due: 0, ..k });
            }
        }
    }
    true
}
/// The date: -2 waxed, -1 unset (a whole period from now), else the game time of the next step (past: at once).
fn read_age(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Long(age) = *t else { return false };
    let (Some(k), Some(CoatRule(p))) = (w.get::<Coat>(e).copied(), w.get::<CoatRule>(e).copied()) else { return false };
    if age == WAXED {
        w.entity_mut(e).insert(Coat { waxed: true, due: 0, ..k });
        return true;
    }
    let k = Coat { waxed: false, ..k };
    let wait = if age == UNSET { period_draw(w, e, p) } else { (age - game_now(w)).clamp(1, i64::from(i32::MAX)) as u64 };
    if k.stage < LAST_STAGE {
        arm_deadline(w, e, k, wait);
    } else {
        w.entity_mut(e).insert(k);
    }
    true
}

/// The coating of the entity: `stage` 0 (plain) to 3 (the oxidized), `waxed`, and `due`, the own tick of its next deadline (0: none, waxed).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[component(storage = "SparseSet")]
pub struct Coat {
    pub stage: u8,
    pub waxed: bool,
    pub due: u64,
}
/// The block the oxidized entity turns into.
#[derive(Clone, Copy, Debug)]
pub struct Statue {
    pub block: &'static str,
    /// The odds of each tick (`nextFloat() <= chance`).
    pub chance: f64,
    pub sound: &'static str,
    /// The poses the block has (a draw picks one).
    pub poses: i32,
}
#[derive(Clone, Copy, Debug)]
pub struct CoatParams {
    pub wax: &'static str,
    pub axes: &'static str,
    pub wax_on_sound: &'static str,
    pub scrape_sound: &'static str,
    /// Level events: wax on, wax off, scrape.
    pub events: [i32; 3],
    /// The ticks between two steps of the weathering: `period[0] + nextInt(period[1] - period[0] + 1)`.
    pub period: [i32; 2],
    pub statue: Option<Statue>,
}
impl CoatParams {
    pub const COPPER: Self = Self {
        wax: "minecraft:honeycomb",
        axes: "#minecraft:axes",
        wax_on_sound: "minecraft:item.honeycomb.wax_on",
        scrape_sound: "minecraft:item.axe.scrape",
        events: [3003, 3004, 3005],
        period: [504_000, 552_000],
        statue: Some(Statue { block: "minecraft:oxidized_copper_golem_statue", chance: 0.0058_f32 as f64, sound: "minecraft:entity.copper_golem_become_statue", poses: 4 }),
    };
}
/// The parameters the deadlines read (a callback of the wheel carries none).
#[derive(Component, Clone, Copy)]
pub struct CoatRule(pub &'static CoatParams);
pub const LAST_STAGE: u8 = 3;

pub static COAT: Rule = Rule { name: "coat", equip, attrs: no_attrs, interact };

/// The ticks until the first success of a draw that succeeds at each tick with the odds `p`, `u` in (0, 1] (the law of drawing every tick).
pub fn wait_for(u: f64, p: f64) -> u64 {
    (u.max(1.0e-12).ln() / (1.0 - p).ln()).floor() as u64
}
fn period_draw(w: &mut World, e: Entity, p: &CoatParams) -> u64 {
    let span = p.period[1] - p.period[0] + 1;
    (p.period[0] + with_rng(w, e, |r| r.next_int(span))) as u64
}
fn equip(w: &mut World, e: Entity, p: &'static (dyn std::any::Any + Sync)) {
    let p = params::<CoatParams>(p);
    w.entity_mut(e).insert(CoatRule(p));
    let wait = period_draw(w, e, p);
    arm_deadline(w, e, Coat::default(), wait);
}
/// Arms a deadline `ticks` from now and records it as the one that counts (the older ones still on the wheel find it later and do nothing).
fn arm_deadline(w: &mut World, e: Entity, coat: Coat, ticks: u64) {
    let due = now_of(w, e) + ticks.max(1);
    w.entity_mut(e).insert(Coat { due, ..coat });
    crate::cow::after(w, e, ticks.max(1) as i32, weather_due);
}
/// `CopperGolem.updateWeathering`, at the deadline: one stage on (the wait to the next one counted from this deadline), then the block try
/// once the last stage is there.
fn weather_due(w: &mut World, e: Entity) {
    let Some(CoatRule(p)) = w.get::<CoatRule>(e).copied() else { return };
    let Some(c) = w.get::<Coat>(e).copied() else { return };
    let now = now_of(w, e);
    // An older callback of the wheel finds the deadline moved on (`due`) and does nothing.
    if c.waxed || c.due == 0 || now < c.due {
        return;
    }
    if c.stage >= LAST_STAGE {
        try_statue(w, e, p, c);
        return;
    }
    let next = Coat { stage: c.stage + 1, ..c };
    if next.stage == LAST_STAGE {
        match p.statue {
            Some(s) => {
                let wait = 1 + with_rng(w, e, |r| wait_for(r.next_f64(), s.chance));
                arm_deadline(w, e, next, wait);
            }
            None => {
                w.entity_mut(e).insert(Coat { due: 0, ..next });
            }
        }
        return;
    }
    let wait = period_draw(w, e, p);
    // `nextWeatheringTick += period`: counted from the deadline (the first one of a plain entity is `now`).
    let late = if c.due > 0 { now.saturating_sub(c.due) } else { 0 };
    arm_deadline(w, e, next, wait.saturating_sub(late));
}
/// `canTurnToStatue` (the odds were met by the wait, the block at its feet must be air) then `turnToStatue`.
fn try_statue(w: &mut World, e: Entity, p: &'static CoatParams, c: Coat) {
    let Some(s) = p.statue else { return };
    let Some(at) = w.get::<Pos>(e).map(|q| q.0) else { return };
    let cell = [at.x.floor() as i32, at.y.floor() as i32, at.z.floor() as i32];
    if !crate::level::block_is_air(w, cell) {
        let wait = 1 + with_rng(w, e, |r| wait_for(r.next_f64(), s.chance));
        arm_deadline(w, e, c, wait);
        return;
    }
    let pose = with_rng(w, e, |r| r.next_int(s.poses));
    let yaw = w.get::<Yaw>(e).map_or(0.0, |y| y.0);
    let name = w.get::<crate::custom_name::CustomName>(e).map(|n| n.0.clone());
    drop_preserved(w, e, at);
    let lead = crate::leash::detach(w, e).map(|h| if let crate::leash::Holder::Knot(p) = h { Some(p) } else { None });
    let ev = &mut w.resource_mut::<Events>().host;
    ev.sound(s.sound, [at.x, at.y, at.z], 1.0, 1.0);
    ev.become_block(e, s.block, cell, pose as u8, yaw, name, lead);
}
/// `Mob.dropPreservedEquipment`: the slots whose drop chance is above 1, in slot order, fall at the feet and leave their slot.
fn drop_preserved(w: &mut World, e: Entity, at: Vec3) {
    for slot in [Slot::Main, Slot::Off, Slot::Feet, Slot::Legs, Slot::Chest, Slot::Head, Slot::Body, Slot::Saddle] {
        if crate::equipment_drop::chance_of(w, e, slot) <= 1.0 {
            continue;
        }
        let id = crate::equipment::take_slot(w, e, slot);
        let Some(name) = crate::items::item_name(id).filter(|_| id != 0) else { continue };
        crate::behaviours::shear::drop_stacks(w, vec![ItemStack::new(name, 1)], [at.x, at.y, at.z]);
    }
}

fn interact(w: &mut World, e: Entity, p: &'static (dyn std::any::Any + Sync), i: &crate::behaviours::Interaction) -> bool {
    let p = *params::<CoatParams>(p);
    if i.item == 0 {
        return false;
    }
    let coat = w.get::<Coat>(e).copied().unwrap_or_default();
    let at = w.get::<Pos>(e).map_or([0.0; 3], |q| [q.0.x, q.0.y, q.0.z]);
    let cell = [at[0].floor() as i32, at[1].floor() as i32, at[2].floor() as i32];
    if crate::items::is(p.wax, i.item) && !coat.waxed {
        w.entity_mut(e).insert(Coat { waxed: true, due: 0, ..coat });
        let ev = &mut w.resource_mut::<Events>().host;
        ev.level_event(p.events[0], cell, 0);
        ev.sound(p.wax_on_sound, at, 1.0, 1.0);
        crate::give_item::take_from_hand(w, i);
        return true;
    }
    if crate::items::is(p.axes, i.item) && (coat.waxed || coat.stage > 0) {
        let (next, event) = if coat.waxed { (Coat { waxed: false, ..coat }, p.events[1]) } else { (Coat { stage: coat.stage - 1, ..coat }, p.events[2]) };
        // `nextWeatheringTick = -1`: the clock starts again, a whole period from now.
        // An oxidized one tries the block at each tick once its clock is set again; the others wait a period.
        match p.statue.filter(|_| next.stage == LAST_STAGE) {
            Some(s) => {
                let wait = 2 + with_rng(w, e, |r| wait_for(r.next_f64(), s.chance));
                arm_deadline(w, e, next, wait);
            }
            None if next.stage == LAST_STAGE => {
                w.entity_mut(e).insert(Coat { due: 0, ..next });
            }
            None => {
                let wait = period_draw(w, e, &p);
                arm_deadline(w, e, next, wait);
            }
        }
        let ev = &mut w.resource_mut::<Events>().host;
        ev.sound(p.scrape_sound, at, 1.0, 1.0);
        ev.level_event(event, cell, 0);
        ev.held_worn(i.player, i.item, 1);
        return true;
    }
    false
}
