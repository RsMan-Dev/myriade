//! `PathNavigation` and its ground, wall-climbing, water-bound, flying and
//! amphibious variants: path creation, waypoint following, corner cutting,
//! stuck detection and path timeouts.
use super::{
    MobWorld,
    path::{Evaluator, Finder, Path, PathMob, PathNode, PathType},
};
use horizon_core::geometry::Vec3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum NavKind {
    #[default]
    Ground,
    WallClimber,
    Water {
        breach: bool,
    },
    Flying,
    /// `AmphibiousPathNavigation` (`Axolotl`, `Drowned`, `Turtle`, and the
    /// `Frog`, whose `FrogPathNavigation.prefersShallowSwimming` costs the
    /// water nodes deeper than ten blocks under sea level.
    Amphibious {
        prefers_shallow: bool,
    },
}
/// The searches `create_path_raw` made (a counter for the benches of the ecs prototype).
pub static FINDER_CALLS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[derive(Clone, Debug)]
pub struct Navigation {
    pub kind: NavKind,
    pub path: Option<Path>,
    pub speed_modifier: f64,
    pub(crate) tick: i64,
    pub(crate) last_stuck_check: i64,
    pub(crate) last_stuck_pos: Vec3,
    pub(crate) timeout_node: [i32; 3],
    pub(crate) timeout_timer: i64,
    pub(crate) last_timeout_check: i64,
    pub(crate) timeout_limit: f64,
    pub max_distance_to_waypoint: f32,
    pub(crate) delayed_recompute: bool,
    /// `ServerLevel.sendBlockUpdated` asked for a recompute: made at the next tick.
    pub(crate) pending_recompute: bool,
    pub(crate) time_last_recompute: i64,
    pub target_pos: Option<[i32; 3]>,
    pub(crate) reach_range: i32,
    pub max_visited_multiplier: f32,
    pub stuck: bool,
    /// Set through [`Mob::set_required_path_length`].
    pub required_path_length: f32,
    /// `PathFinder.maxVisitedNodes`: the base follow range times 16 when the
    /// navigation is made, then [`Mob::update_max_visited_nodes`].
    pub max_visited_nodes: i32,
    pub can_float: bool,
    pub can_open_doors: bool,
    pub can_pass_doors: bool,
    pub can_walk_over_fences: bool,
    pub avoid_sun: bool,
    pub below_surface: bool,
    /// WallClimberNavigation.pathToPosition
    pub(crate) climb_to: Option<[i32; 3]>,
}
/// A controlling passenger uses the vehicle's actual navigation, but the
/// passenger's private navigation still ticks in Mob.serverAiStep.
#[derive(Clone, Debug)]
pub(super) struct Redirect {
    pub own: Navigation,
    pub body: PathMob,
    pub follow_range: f32,
    pub passenger: bool,
    pub in_liquid: bool,
}
impl Default for Navigation {
    fn default() -> Self {
        Self {
            kind: NavKind::Ground,
            path: None,
            speed_modifier: 0.0,
            tick: 0,
            last_stuck_check: 0,
            last_stuck_pos: Vec3::ZERO,
            timeout_node: [0; 3],
            timeout_timer: 0,
            last_timeout_check: 0,
            timeout_limit: 0.0,
            max_distance_to_waypoint: 0.5,
            delayed_recompute: false,
            pending_recompute: false,
            time_last_recompute: 0,
            target_pos: None,
            reach_range: 0,
            max_visited_multiplier: 1.0,
            stuck: false,
            required_path_length: 16.0,
            max_visited_nodes: 256,
            can_float: false,
            can_open_doors: false,
            can_pass_doors: true,
            can_walk_over_fences: false,
            avoid_sun: false,
            below_surface: false,
            climb_to: None,
        }
    }
}
impl Navigation {
    pub fn is_done(&self) -> bool {
        self.path.as_ref().is_none_or(Path::is_done)
    }
    pub fn is_in_progress(&self) -> bool {
        !self.is_done()
    }
    pub fn stop(&mut self) {
        self.path = None;
    }
    /// A mob whose AI step a mod paused (`mobs::ai_pause`) keeps its path,
    /// and `doStuckDetection`'s timeout clock does not count the pause: on
    /// resume it adds one tick, not the paused game time.
    pub(super) fn hold_timeout_clock(&mut self, game_time: i64) {
        if self.path.as_ref().is_some_and(|p| !p.is_done()) {
            self.last_timeout_check = game_time;
        }
    }
    pub(crate) fn reset_stuck_timeout(&mut self) {
        self.timeout_node = [0; 3];
        self.timeout_timer = 0;
        self.timeout_limit = 0.0;
        self.stuck = false;
    }
    pub(crate) fn evaluator(&self) -> Evaluator {
        match self.kind {
            NavKind::Ground | NavKind::WallClimber => Evaluator::Walk,
            NavKind::Water { breach } => Evaluator::Swim { breach },
            NavKind::Flying => Evaluator::Fly,
            NavKind::Amphibious { prefers_shallow } => Evaluator::Amphibious { prefers_shallow },
        }
    }
    pub fn can_cut_corner(t: PathType) -> bool {
        !matches!(
            t,
            PathType::FireInNeighbor | PathType::DamagingInNeighbor | PathType::WalkableDoor
        )
    }
}
/// `LivingEntity.getComfortableFallDistance`: the drop an entity accepts on
/// top of what it was asked for.
fn comfortable_fall_distance(f: f32) -> i32 {
    (f + 3.0).floor() as i32
}

#[cfg(test)]
mod recompute_tests {
    use super::*;
    use crate::mobs::path::{Path, PathNode, PathType};

    fn straight(len: i32) -> Path {
        Path {
            nodes: (0..len)
                .map(|x| PathNode {
                    x,
                    y: 0,
                    z: 0,
                    kind: PathType::Walkable,
                })
                .collect(),
            next: 0,
            target: [len - 1, 0, 0],
            dist_to_target: 0.0,
            reached: true,
        }
    }

    /// `PathNavigation.shouldRecomputePath`: a block within (nodes left) of the
    /// middle of the way to the path's end, and nothing while a recompute waits.
    #[test]
    fn a_change_near_the_middle_of_the_path_asks_for_a_recompute() {
        let mut m = walker();
        assert!(!m.should_recompute_path([5, 0, 0]), "no path");
        m.ai.nav.path = Some(straight(11));
        // The middle of (10, 0, 0) and (0.5, 0, 0.5) is about (5.25, 0, 0.25);
        // 11 nodes are left.
        assert!(m.should_recompute_path([5, 0, 0]));
        assert!(m.should_recompute_path([5, 5, 0]));
        assert!(!m.should_recompute_path([5, 12, 0]), "too far");
        assert!(!m.should_recompute_path([-9, 0, 0]), "behind the mob");
        m.ai.nav.delayed_recompute = true;
        assert!(
            !m.should_recompute_path([5, 0, 0]),
            "one is already waiting"
        );
        m.ai.nav.delayed_recompute = false;
        m.ai.nav.path.as_mut().unwrap().next = 11;
        assert!(!m.should_recompute_path([5, 0, 0]), "the path is done");
    }
}

#[cfg(test)]
mod tests {
    use super::*;


    /// `Frog.FrogPathNavigation.prefersShallowSwimming` has to reach the path
    /// finder, or the shallow-water malus never applies to a frog.
    #[test]
    fn the_amphibious_navigation_keeps_its_shallow_preference() {
        let mut nav = Navigation::default();
        assert!(
            matches!(nav.evaluator(), Evaluator::Walk),
            "ground by default"
        );
        nav.kind = NavKind::Amphibious {
            prefers_shallow: true,
        };
        assert!(matches!(
            nav.evaluator(),
            Evaluator::Amphibious {
                prefers_shallow: true
            }
        ));
        nav.kind = NavKind::Amphibious {
            prefers_shallow: false,
        };
        assert!(matches!(
            nav.evaluator(),
            Evaluator::Amphibious {
                prefers_shallow: false
            }
        ));
    }


}

#[cfg(test)]
mod supplied_path_tests {
    use super::*;
    use crate::mobs::world::{with_built_test_world, with_test_world};
    use horizon_core::geometry::Vec3;

    fn cow(types: &crate::mobs::MobTypes) -> std::sync::Arc<crate::mobs::MobType> {
        std::sync::Arc::new(types.types["minecraft:cow"].clone())
    }









}
