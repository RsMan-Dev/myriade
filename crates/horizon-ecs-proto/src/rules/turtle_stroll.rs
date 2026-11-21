//! The turtle's stroll gate: a turtle strolls out of the water, not going home and without an egg.

/// Whether the turtle may stroll now (`gate` of the stroll): not in water, not going home, carrying no egg.
pub fn may_stroll(in_water: bool, going_home: bool, has_egg: bool) -> bool {
    !in_water && !going_home && !has_egg
}
