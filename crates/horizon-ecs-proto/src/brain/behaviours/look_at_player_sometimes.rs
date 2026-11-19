//! `SetEntityLookTargetSometimes`: with no look target and an entity within `range` blocks in sight (a player only, by default, within
//! 6 blocks), a ticker of `min` to `min + spread` ticks (40 to 80 by default) counts down (only while that holds); when it reaches zero
//! that entity becomes the look target. Parameters are optional: an entry that gives none has the animals' (`Params::DEFAULT`).
use crate::brain::*;
use crate::cow::*;
use crate::goals::d2;
use crate::targeting::Who;

#[derive(Component)]
pub struct Ticker(i32);
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
    w.entity_mut(e).insert(Ticker(0));
}
fn can_start(w: &mut World, e: Entity, c: &Ctx) -> bool {
    let p = params(c);
    let Some(pos) = w.get::<Pos>(e).map(|p| p.0) else { return false };
    let found = find_closest(w, e, |w, who| (!p.players_only || matches!(who, Who::Player(_))) && crate::targeting::info(w, who).is_some_and(|i| d2(pos, i.pos) <= p.range_sqr));
    let Some(who) = found else { return false };
    // The ticker counts while a player is in sight: the brain is stepped again next tick (and not otherwise).
    wake(w, e);
    let t = w.get::<Ticker>(e).unwrap().0;
    if t == 0 {
        let n = p.min + with_rng(w, e, |r| r.next_int(p.spread)) - 1;
        w.get_mut::<Ticker>(e).unwrap().0 = n;
        return false;
    }
    w.get_mut::<Ticker>(e).unwrap().0 = t - 1;
    if t - 1 != 0 {
        return false;
    }
    set(w, e, &LOOK_TARGET, MemValue::Look(Tracker::Entity(who, true)), None);
    true
}
