//! `LookAtPlayer` (`B::LookAtPlayer`, `animal_brain.rs:1384-1396`, a one-shot kid of the sniffer's idle gate): with no look target and a visible
//! player within 6 blocks, the nearest becomes the look target at once (`look_at_player_sometimes` waits 40 to 80 ticks first). "No look target"
//! and "the visible list is present" are the entry's `needs`; a rider looks at nobody.
use crate::brain::*;
use crate::cow::*;
use crate::goals::{d2, is_occupied, OCC_MOUNTED};
use crate::rules::look_at_player as rules;
use crate::targeting::{info, Who};

pub static BEHAVIOUR: Behaviour = Behaviour { name: "minecraft:look_at_player", mems: &[&LOOK_TARGET, &VISIBLE_MOBS], one_shot: true, can_start, ..Behaviour::DEFAULT };

fn can_start(w: &mut World, e: Entity, _: &Ctx) -> bool {
    let Some(pos) = w.get::<Pos>(e).map(|p| p.0) else { return false };
    if is_occupied(w, e, OCC_MOUNTED) {
        return false;
    }
    let found = find_closest(w, e, |w, who| matches!(who, Who::Player(_)) && info(w, who).is_some_and(|i| rules::in_reach(d2(pos, i.pos))));
    let Some(who) = found else { return false };
    set(w, e, &LOOK_TARGET, MemValue::Look(Tracker::Entity(who, true)), None);
    true
}
