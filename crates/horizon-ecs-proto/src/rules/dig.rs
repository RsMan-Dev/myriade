//! Calculations of the burrowing: the conditions under which the animal digs.

/// The burrowing may start: on the ground or in a fluid, with no target and no walk under way.
pub fn may_dig(grounded: bool, fluid: bool, has_target: bool, walking: bool) -> bool {
    (grounded || fluid) && !has_target && !walking
}
