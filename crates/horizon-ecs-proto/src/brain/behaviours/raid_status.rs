//! `SetRaidStatus` / `ResetRaidStatus` (`villager_brain.rs:1800`, `:2464`), 1 tick in 20 each: the raid's state poses the activity.
use crate::behaviours::raid::{self, PRE_RAID_ON, RAID_ACTIVE, RAID_BEFORE, RAID_LOSS, RAID_NONE, RAID_ON, RAID_VICTORY};
use crate::brain::*;
use crate::goals::with_rng;

pub static SET: Behaviour = Behaviour { name: "minecraft:set_raid_status", mems: &[&RAID_ON, &PRE_RAID_ON], one_shot: true, can_start: set_can_start, ..Behaviour::DEFAULT };
pub static RESET: Behaviour = Behaviour { name: "minecraft:reset_raid_status", mems: &[&RAID_ON, &PRE_RAID_ON], one_shot: true, can_start: reset_can_start, ..Behaviour::DEFAULT };

fn set_can_start(w: &mut World, e: Entity, _: &Ctx) -> bool {
    if with_rng(w, e, |r| r.next_int(20)) != 0 {
        return false;
    }
    match raid::status(w, e) {
        RAID_ACTIVE | RAID_VICTORY => {
            erase(w, e, &PRE_RAID_ON);
            set(w, e, &RAID_ON, MemValue::Unit, None);
        }
        RAID_BEFORE => {
            erase(w, e, &RAID_ON);
            set(w, e, &PRE_RAID_ON, MemValue::Unit, None);
        }
        _ => {}
    }
    true
}

fn reset_can_start(w: &mut World, e: Entity, _: &Ctx) -> bool {
    if with_rng(w, e, |r| r.next_int(20)) != 0 {
        return false;
    }
    if matches!(raid::status(w, e), RAID_NONE | RAID_LOSS) {
        erase(w, e, &RAID_ON);
        erase(w, e, &PRE_RAID_ON);
    }
    true
}
