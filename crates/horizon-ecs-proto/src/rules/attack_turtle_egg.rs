//! Calculations of the turtle-egg hunt (`ZombieAttackTurtleEggGoal`): the delay of a search, the hop of a stamp, the egg under a
//! mob, the cells that are an egg.
use crate::rules::nearest_target::reduced;
use myriade_engine_lib::mobs::MobWorld;

/// The ticks before the next search, from the draw `draw` of `nextInt(200)`: `reduced(200 + draw)` asks, two ticks each.
pub fn search_ticks(draw: i32) -> i32 {
    2 * (reduced(200 + draw) + 1)
}
/// The ticks before the next search after an egg was found: `reduced(20)` asks, two ticks each.
pub fn found_search_ticks() -> i32 {
    2 * (reduced(20) + 1)
}
/// The vertical speed of a stamp `since_reached` ticks after the mob reached the egg: a hop up after the first tick, down on the even ones.
pub fn stamp_hop_y(since_reached: i32) -> Option<f64> {
    let mut vy = None;
    if since_reached > 0 {
        vy = Some(0.3);
    }
    if since_reached % 2 == 0 {
        vy = Some(-0.3);
    }
    vy
}
/// The egg breaks once the mob has stood by it for more than 60 ticks.
pub fn egg_breaks(since_reached: i32) -> bool {
    since_reached > 60
}
/// The egg the mob stamps: the first of its feet, beside them, or two below holding a turtle egg.
pub fn egg_under(mw: &MobWorld, at: [i32; 3]) -> Option<[i32; 3]> {
    [
        at,
        [at[0], at[1] - 1, at[2]],
        [at[0] - 1, at[1], at[2]],
        [at[0] + 1, at[1], at[2]],
        [at[0], at[1], at[2] - 1],
        [at[0], at[1], at[2] + 1],
        [at[0], at[1] - 2, at[2]],
    ]
    .into_iter()
    .find(|p| mw.block_name(p[0], p[1], p[2]) == "minecraft:turtle_egg")
}
/// The state ids of the turtle egg block, read once from the facts of the process (the states of a block are contiguous): the
/// search compares numbers, not names (a 24-block search reads some 16 000 cells).
fn egg_states(mw: &MobWorld) -> (u32, u32) {
    static IDS: std::sync::OnceLock<(u32, u32)> = std::sync::OnceLock::new();
    *IDS.get_or_init(|| {
        let mut it = mw.facts.states.iter().enumerate().filter(|(_, s)| &*s.name == "minecraft:turtle_egg").map(|(i, _)| i as u32);
        let first = it.next();
        first.map_or((1, 0), |f| (f, it.last().unwrap_or(f)))
    })
}
/// `RemoveBlockGoal.isValidTarget` without `canEntityDestroy` (no such service: taken as true, DOUTE): an egg in a loaded
/// chunk with two air blocks above it.
pub fn egg_target(mw: &MobWorld, p: [i32; 3]) -> bool {
    if !mw.world.loaded(p[0], p[2]) {
        return false;
    }
    let (lo, hi) = egg_states(mw);
    let id = mw.world.block(p[0], p[1], p[2]).0;
    id >= lo && id <= hi && mw.facts.is_air(mw.world.block(p[0], p[1] + 1, p[2])) && mw.facts.is_air(mw.world.block(p[0], p[1] + 2, p[2]))
}
