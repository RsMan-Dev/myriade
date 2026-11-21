//! `LookAtEntity` (`brain/behaviours/look_at_entity.rs`): the reach of the entity the villager looks at.

/// Whether an entity at squared distance `dist2` can be looked at: 64 squared or less.
pub fn in_reach(dist2: f64) -> bool {
    dist2 <= 64.0
}
