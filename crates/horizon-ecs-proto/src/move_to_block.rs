//! `MoveToBlockGoal`, once for every module that walks to a block (turtle eggs, berries, beds...): the search for the nearest
//! block a predicate accepts (rings outward, the heights alternating, the first cell taken), the walk to it (a path to the
//! block above, asked again every 40 ticks), the stay (`max_stay`, `try_ticks`) and "reached". It is `Mob`'s `MoveToBlock`
//! (`goals.rs`) ported line by line; a module keeps one in its state component, draws its `next_start` from the wheel (the
//! search is a deadline, not a poll) and asks `invalid_after_change` when blocks changed in its zone.
use bevy_ecs::prelude::*;
use crate::rules::move_to_block::{any_in_box, can_continue_ticks, first_valid, is_far, reach_point, search_zone, target_point};
use horizon_client_lib::mobs::MobWorld;

#[derive(Component, Clone, Debug, Default)]
pub struct MoveToBlock {
    pub speed: f64,
    pub range: i32,
    pub vertical_range: i32,
    pub vertical_start: i32,
    pub try_ticks: i32,
    pub max_stay: i32,
    pub reached: bool,
    pub block: [i32; 3],
}
impl MoveToBlock {
    pub fn new(speed: f64, range: i32, vertical_range: i32) -> Self {
        Self { speed, range, vertical_range, ..Default::default() }
    }
    /// `findNearestBlock` from the block `base` the entity stands in: the first cell `valid` accepts, or `None`; on a hit,
    /// `self.block` is it.
    pub fn find_nearest(&mut self, base: [i32; 3], mw: &MobWorld, valid: impl Fn(&MobWorld, [i32; 3]) -> bool) -> bool {
        match self.walk(base, |p| valid(mw, p)) {
            Some(p) => {
                self.block = p;
                true
            }
            None => false,
        }
    }
    fn walk(&self, base: [i32; 3], valid: impl FnMut([i32; 3]) -> bool) -> Option<[i32; 3]> {
        first_valid(base, self.range, self.vertical_start, self.vertical_range, valid)
    }
    /// The box of cells the search reaches from `base` (inclusive), or `None` when it walks none: a block change inside it
    /// invalidates a found block (and may make a failed search succeed).
    pub fn zone(&self, base: [i32; 3]) -> Option<([i32; 3], [i32; 3])> {
        search_zone(base, self.range, self.vertical_start, self.vertical_range)
    }
    /// `MoveToBlockGoal` of the `Mob`: whether the search from `base` may find a block whose state `matches` (false when no section of
    /// the cells it walks holds such a state in its palette: the walk would find nothing, and is not made).
    pub fn may_find(&self, base: [i32; 3], mw: &MobWorld, matches: impl FnMut(horizon_world::registry::BlockStateId) -> bool) -> bool {
        self.zone(base).is_some_and(|(lo, hi)| mw.world.may_hold(lo, hi, matches))
    }
    /// A block of `blocks` (changed positions) lies in the zone of the search from `base`.
    pub fn invalid_after_change(&self, base: [i32; 3], blocks: &[(i32, i32, i32)]) -> bool {
        self.zone(base).is_some_and(|(lo, hi)| any_in_box(lo, hi, blocks))
    }
    /// `canContinueToUse`: not tired, not too long, the block still valid.
    pub fn can_continue(&self, mw: &MobWorld, valid: impl Fn(&MobWorld, [i32; 3]) -> bool) -> bool {
        can_continue_ticks(self.try_ticks, self.max_stay) && valid(mw, self.block)
    }
    /// The point the walk aims at (the block above, its middle).
    pub fn target(&self) -> [f64; 3] {
        target_point(self.block)
    }
    /// `start`: the two draws of the stay (the walk is asked of the framework: `goals::go_to_at(w, e, self.target(), speed, reach)`).
    pub fn start(&mut self, rng: &mut crate::cow::Rng) {
        self.try_ticks = 0;
        let r = rng.next_int(1200) + 1200;
        self.max_stay = rng.next_int(r) + 1200;
    }
    /// `tick` with the goal's accepted distance: reached or not; true when the path must be asked again (every 40 ticks of walking).
    pub fn tick(&mut self, pos: [f64; 3], accepted: f64) -> bool {
        if is_far(pos, reach_point(self.block), accepted) {
            self.reached = false;
            self.try_ticks += 1;
            self.try_ticks % 40 == 0
        } else {
            self.reached = true;
            self.try_ticks -= 1;
            false
        }
    }
}
