//! Calculations of the door breaking (`BreakDoorGoal`): the time of cracking, the difficulty and the raid's start odds,
//! and when the goal goes on.

/// `DefaultDoorBreakTime`: `BreakDoorGoal.getDoorBreakTime` is `max(240, doorBreakTime)`, and the vindicator's 6 is below it.
pub const BREAK_TIME: i32 = 240;
/// A raider starts breaking a door on one draw in this many of the raid's checks.
pub const RAID_START_ODDS: i32 = 10;
/// The door's sound is one draw in this many of the cracking ticks.
pub const SOUND_ODDS: i32 = 20;

/// A draw hit the odds (the draw is `next_int(odds)`, a hit is 0).
pub fn hit(draw: i32) -> bool {
    draw == 0
}

/// The goal goes on: in a raid it needs an active raid, the cracking time is not over, the door is shut and close, and the
/// difficulty allows it (`difficulty_ok`).
pub fn keeps_breaking(raid: bool, raid_active: bool, time: i32, open: bool, close: bool, difficulty_ok: bool) -> bool {
    (!raid || raid_active) && time <= BREAK_TIME && !open && close && difficulty_ok
}
