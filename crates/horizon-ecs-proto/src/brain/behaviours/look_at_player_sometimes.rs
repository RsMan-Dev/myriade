//! `SetEntityLookTargetSometimes`: with no look target and an entity within `range` blocks in sight (a player only, by default, within
//! 6 blocks), a ticker of `min` to `min + spread` ticks (40 to 80 by default) counts down (only while that holds); when it reaches zero
//! that entity becomes the look target. Parameters are optional: an entry that gives none has the animals' (`Params::DEFAULT`).
use crate::brain::*;
use crate::cow::*;
use crate::goals::d2;
use crate::targeting::Who;

#[derive(Component, Clone, Copy)]
pub struct Ticker {
    /// Ticks still to count while a target is in sight (paused when none is).
    left: i32,
    /// The own time at which the count ends, while it runs: the brain is woken then, not every tick.
    due: Option<u64>,
}
/// The creaking: any visible entity within 8 blocks, 30 to 60 ticks.
pub struct Params {
    pub range_sqr: f64,
    pub min: i32,
    pub spread: i32,
    pub players_only: bool,
}
impl Params {
    pub const DEFAULT: Params = Params { range_sqr: 36.0, min: 40, spread: 41, players_only: true };
}
fn params(c: &Ctx) -> &'static Params {
    let p: &'static dyn std::any::Any = c.params;
    p.downcast_ref::<Params>().unwrap_or(&Params::DEFAULT)
}
pub static BEHAVIOUR: Behaviour = Behaviour { name: "minecraft:look_at_player_sometimes", mems: &[&LOOK_TARGET, &VISIBLE_MOBS], one_shot: true, equip, can_start, ..Behaviour::DEFAULT };
fn equip(w: &mut World, e: Entity, _: &Ctx) {
    w.entity_mut(e).insert(Ticker { left: 0, due: None });
}
fn can_start(w: &mut World, e: Entity, c: &Ctx) -> bool {
    let p = params(c);
    let Some(pos) = w.get::<Pos>(e).map(|p| p.0) else { return false };
    let found = find_closest(w, e, |w, who| (!p.players_only || matches!(who, Who::Player(_))) && crate::targeting::info(w, who).is_some_and(|i| d2(pos, i.pos) <= p.range_sqr));
    let now = now_of(w, e);
    let Some(who) = found else {
        // Nobody in sight: the count pauses with what is left.
        let mut t = w.get_mut::<Ticker>(e).unwrap();
        if let Some(due) = t.due.take() {
            t.left = due.saturating_sub(now) as i32;
        }
        return false;
    };
    let t = *w.get::<Ticker>(e).unwrap();
    match t.due {
        None => {
            // `Ticker.tickDownAndCheck`: the first tick draws the count, the next ones count it down; here the end is a deadline.
            let n = if t.left == 0 { p.min + with_rng(w, e, |r| r.next_int(p.spread)) - 1 } else { t.left };
            let due = now + n.max(1) as u64;
            *w.get_mut::<Ticker>(e).unwrap() = Ticker { left: 0, due: Some(due) };
            arm_at(w, e, due, Due::BrainWake);
            return false;
        }
        Some(due) if now < due => return false,
        Some(_) => w.get_mut::<Ticker>(e).unwrap().due = None,
    }
    set(w, e, &LOOK_TARGET, MemValue::Look(Tracker::Entity(who, true)), None);
    true
}
