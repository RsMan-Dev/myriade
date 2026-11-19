//! `SetEntityLookTargetSometimes` for a player: with no look target and a player within 6 blocks in sight, a ticker of 40 to
//! 80 ticks counts down (only while that holds); when it reaches zero the player becomes the look target.
use crate::brain::*;
use crate::cow::*;
use crate::goals::d2;
use crate::targeting::Who;

#[derive(Component)]
pub struct Ticker(i32);
pub static BEHAVIOUR: Behaviour = Behaviour { name: "minecraft:look_at_player_sometimes", mems: &[&LOOK_TARGET, &VISIBLE_MOBS], one_shot: true, poll: true, equip, can_start, ..Behaviour::DEFAULT };
fn equip(w: &mut World, e: Entity, _: &Ctx) {
    w.entity_mut(e).insert(Ticker(0));
}
fn can_start(w: &mut World, e: Entity, _: &Ctx) -> bool {
    let Some(pos) = w.get::<Pos>(e).map(|p| p.0) else { return false };
    let found = find_closest(w, e, |w, who| matches!(who, Who::Player(_)) && crate::targeting::info(w, who).is_some_and(|i| d2(pos, i.pos) <= 36.0));
    let Some(who) = found else { return false };
    let t = w.get::<Ticker>(e).unwrap().0;
    if t == 0 {
        let n = 40 + with_rng(w, e, |r| r.next_int(41)) - 1;
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
