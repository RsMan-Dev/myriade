//! The raider's hold ground: when it may start, the raiders it shouts at, and the radius that turns it aggressive.

/// `Raider.HoldGroundAttackGoal` may start: a raider outside a raid, patrolling, with a target, not aggressive, and not hurt by a player.
pub fn hold_may_start(raider_outside: bool, patrolling: bool, has_target: bool, aggressive: bool, hurt_by_player: bool) -> bool {
    raider_outside && patrolling && has_target && !aggressive && !hurt_by_player
}
/// `shoutTargeting`: a raider within 8 blocks (`dist_sq` the squared distance).
pub fn in_shout(dist_sq: f64) -> bool {
    dist_sq <= 64.0
}
/// Beyond the radius the raider only looks at its target; `dist_sq` is the squared distance to it.
pub fn beyond_radius(dist_sq: f64, radius: f64) -> bool {
    dist_sq > radius * radius
}
