//! Conversion of a creature kept with its eyes in water (`Zombie.drowningTracker`, a `ConversionTracker`; the husk becomes a zombie): once the
//! eyes have stayed under for `soak` ticks (600, `InWaterTime`) the conversion starts (the synced shake, `convert_timer::Converting`), and
//! `convert` ticks later (300, `DrownedConversionTime`) the creature becomes `into` (`conversion::convert_or_ask`, equipment kept). Eyes out
//! of the water cancel the whole count (`afflictionTime = -1`, `setConverting(false)`) at the event of the eyes (`eye_left_water`) or of the
//! body (`left_fluid`). Nothing runs while dry: `eye_entered_water` wakes the goal, and the two ends of the count are deadlines.
//! Count of the tracker (`ConversionTracker.tick`, read from the jar): `afflictionTime` goes up by one on each tick with the eyes under, so it
//! reaches `soak` on the `soak`-th such tick (`began + soak - 1`, `began` being the tick the eyes went under); `startConversion` then sets
//! `conversionTime = convert`, which goes down by one a tick and converts at `< 0`: `convert + 1` ticks after the start. The event of the eyes is
//! read the tick after the change (`Flip` keeps the real tick). The saved `InWaterTime` of a count in progress resumes it from there.
use super::convert_timer::Converting;
use super::*;
use crate::cow::*;
use crate::goals::*;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The species it becomes, by name.
    pub into: &'static str,
    /// Ticks with the eyes under before the conversion starts, then ticks of the conversion.
    pub soak: i32,
    pub convert: i32,
    /// The level event of the conversion (`getConversionSound`: 1040 drowned, 1041 husk).
    pub sound: i32,
}
impl Params {
    pub const DEFAULT: Self = Self { into: "", soak: 600, convert: 300, sound: 1040 };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Soak(pub &'static Params, u8);
/// The count runs: its deadline is at `at` (entity time), `began` is when the eyes went under; `converting`: the conversion has begun.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Soaking {
    pub converting: bool,
    pub at: u64,
    pub began: u64,
}

/// The tick the eyes went under (set when the event is read, consumed by `start`).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
struct Flip(u64);

pub static MODULE: Module = Module {
    name: "convert_in_water",
    flags: 0,
    equip,
    can_use,
    start,
    stop,
    systems,
    requires: &[Cap::Body, Cap::Goals],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Soak(params::<Params>(p), inst.slot));
}
fn eyes_under(w: &World, e: Entity) -> bool {
    w.get::<FluidState>(e).is_some_and(|f| f.eye_in_water)
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    eyes_under(w, e)
}
fn start(w: &mut World, e: Entity, _: Inst) {
    let flip = w.entity_mut(e).take::<Flip>().map(|f| f.0);
    // A count resumed from a save has its deadline armed already.
    if w.get::<Soaking>(e).is_some() {
        return;
    }
    let soak = w.get::<Soak>(e).unwrap().0.soak;
    let now = now_of(w, e);
    let began = flip.unwrap_or(now);
    let at = (began + soak.max(1) as u64 - 1).max(now);
    w.entity_mut(e).insert(Soaking { converting: false, at, began });
    crate::cow::after(w, e, (at - now) as i32, due);
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<(Soaking, Converting, Flip)>();
}
/// The water events: the eyes going under wake the goal; the eyes or the body leaving the water end the count.
fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems(water_events.in_set(slot));
}
fn water_events(
    ev: Res<Events>,
    running: Query<&Soak, With<Soaking>>,
    idle: Query<(), (With<Soak>, Without<Soaking>)>,
    flipped: Query<(), With<Flip>>,
    mut reqs: ResMut<Requests>,
) {
    for e in ev.eye_entered_water.iter() {
        if idle.contains(e) {
            reqs.0.push(Req::Call(e, mark_and_wake));
        }
    }
    for e in ev.eye_left_water.iter().chain(ev.left_fluid.iter()) {
        if let Ok(s) = running.get(e) {
            reqs.0.push(Req::End(e, s.1));
        } else if flipped.contains(e) {
            reqs.0.push(Req::Call(e, clear_flip));
        }
    }
}
/// The event is read one tick after the change: the eyes went under at the previous tick.
fn mark_and_wake(w: &mut World, e: Entity) {
    let now = now_of(w, e);
    w.entity_mut(e).insert(Flip(now.saturating_sub(1)));
    wake_it(w, e);
}
fn clear_flip(w: &mut World, e: Entity) {
    w.entity_mut(e).remove::<Flip>();
}
fn wake_it(w: &mut World, e: Entity) {
    crate::goals::wake(w, e, &MODULE);
}
fn cancel(w: &mut World, e: Entity) {
    w.entity_mut(e).remove::<(Soaking, Converting)>();
    crate::goals::end(w, e, &MODULE);
}
/// A deadline: the soak is over (the conversion begins, if the eyes are still under) or the conversion is. A deadline of a count that was
/// cancelled and begun again is not this count's: only the one at `Soaking::at` counts.
fn due(w: &mut World, e: Entity) {
    let (Some(s), Some(p)) = (w.get::<Soaking>(e).copied(), w.get::<Soak>(e).copied()) else { return };
    let now = now_of(w, e);
    if now != s.at {
        return;
    }
    if !eyes_under(w, e) {
        cancel(w, e);
    } else if !s.converting {
        let ticks = p.0.convert + 1;
        let at = now + ticks.max(0) as u64;
        w.entity_mut(e).insert((Soaking { converting: true, at, began: s.began }, Converting { until: at }));
        crate::cow::after(w, e, ticks, due);
    } else {
        w.entity_mut(e).remove::<(Soaking, Converting)>();
        crate::conversion::convert_or_ask(w, e, p.0.into, crate::conversion::ConvertKeep { post: crate::conversion::Post::Tracker { sound: p.0.sound }, ..crate::conversion::ConvertKeep::ALL });
    }
}

/// `(InWaterTime, DrownedConversionTime)` of the count in progress, as the tracker saves them (-1: none).
pub fn saved(w: &World, e: Entity) -> Option<(i32, i32)> {
    let (s, p) = (w.get::<Soaking>(e)?, w.get::<Soak>(e)?.0);
    let now = now_of(w, e);
    if s.converting {
        Some((p.soak, (s.at.saturating_sub(now) as i32 - 1).max(0)))
    } else {
        Some((now.saturating_sub(s.began) as i32 + 1, -1))
    }
}
/// A count read from a save: `converting` with `left` ticks of conversion left, or `left` ticks already soaked. The goal is woken: it ends
/// the count at once if the eyes are not under.
pub fn resume(w: &mut World, e: Entity, converting: bool, left: i32) {
    let Some(p) = w.get::<Soak>(e).map(|s| s.0) else { return };
    let now = now_of(w, e);
    let ticks;
    if converting {
        ticks = left.max(0) + 1;
        let at = now + ticks as u64;
        w.entity_mut(e).insert((Soaking { converting: true, at, began: now }, Converting { until: at }));
    } else {
        ticks = (p.soak - left).max(0);
        w.entity_mut(e).insert(Soaking { converting: false, at: now + ticks as u64, began: (now + 1).saturating_sub(left.max(0) as u64) });
    }
    crate::cow::after(w, e, ticks, due);
    wake_it(w, e);
}
