//! Llama caravans and the trader's llamas: `LlamaFollowCaravanGoal` (a
//! llama joins the nearest line of llamas that starts at a leashed one and
//! follows the llama ahead), `TraderLlamaDefendWanderingTraderGoal` and the
//! trader llama's despawn.
use super::{
    MobWorld, Seen, Species, Who,
    goals::{Cx, Goal, TARGET, TargetState, goal_clone, reduced, target_continue},
    llama::is_llama,
};
use horizon_core::geometry::Vec3;

const CARAVAN_LIMIT: i32 = 8;

fn is_leashed_seen(s: &Seen) -> bool {
    s.leash.is_some()
}
/// Whether another llama follows `s` (`hasCaravanTail`).
fn has_tail(cx: &Cx, s: &Seen) -> bool {
    cx.w.others
        .iter()
        .any(|o| is_llama(o.species) && o.alive && o.caravan == s.id + 1)
}
/// `firstIsLeashed`: the line leads back to a leashed llama in at most
/// eight links.
fn first_is_leashed(cx: &Cx, caravan: u32, leashed: bool, counter: i32) -> bool {
    if counter > CARAVAN_LIMIT {
        return false;
    }
    let _ = leashed;
    if caravan == 0 {
        return false;
    }
    let Some(head) = cx.w.seen(caravan - 1) else {
        return false;
    };
    if is_leashed_seen(head) {
        true
    } else {
        first_is_leashed(cx, head.caravan, false, counter + 1)
    }
}
/// `LlamaFollowCaravanGoal`.
#[derive(Clone, Debug)]
pub struct FollowCaravan {
    speed: f64,
    dist_check: i32,
}
impl FollowCaravan {
    pub fn new(speed: f64) -> Self {
        Self {
            speed,
            dist_check: 0,
        }
    }
}
impl Goal for FollowCaravan {
    goal_clone!();
    fn name(&self) -> &'static str {
        "LlamaFollowCaravanGoal"
    }
    fn flags(&self) -> u8 {
        super::goals::MOVE
    }
}
/// `TraderLlamaDefendWanderingTraderGoal`: attack whoever last hurt the
/// wandering trader holding this llama's lead.
#[derive(Clone, Debug, Default)]
pub struct DefendTrader {
    attacker: Option<Who>,
    timestamp: i32,
    st: TargetState,
}
impl Goal for DefendTrader {
    goal_clone!();
    fn flags(&self) -> u8 {
        TARGET
    }
}

#[cfg(test)]
mod defend_trader_tests {
    use super::*;
    use crate::mobs::world::{test_player, with_test_world};
    use crate::mobs::{Species, sensor_scan_tests::kind};
    use horizon_core::geometry::Vec3;
    use std::sync::Arc;

}
