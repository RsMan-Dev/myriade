/// `TamableAnimal.tryToTeleportToOwner`: a try is kept only outside the two-block square under the owner, given its offsets `(xd, zd)`.
pub fn teleport_ring(xd: i32, zd: i32) -> bool {
    xd.abs() >= 2 || zd.abs() >= 2
}

/// The spot of a teleport try: the owner's block `base` moved by the offsets.
pub fn teleport_spot(base: [i32; 3], off: [i32; 3]) -> [i32; 3] {
    [base[0] + off[0], base[1] + off[1], base[2] + off[2]]
}

/// `TamableAnimal.canTeleportTo` on the block below and the spot itself: a full block that is not leaves, and no fluid in the spot.
pub fn can_stand_on(below_full_collision: bool, below_name: &str, spot_has_fluid: bool) -> bool {
    below_full_collision && !below_name.ends_with("_leaves") && !spot_has_fluid
}
