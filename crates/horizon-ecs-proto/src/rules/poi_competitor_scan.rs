//! The job site's competition: the villager loses its site when a rival at least as experienced holds it (the second wins a tie).

/// `selectWinner` of the `Mob`: true when one of the `rivals` (their experience, in the order of the list) is at least `own`.
pub fn loses_to(own: i32, rivals: impl Iterator<Item = i32>) -> bool {
    let mut best = own;
    let mut lost = false;
    for xp in rivals {
        if best <= xp {
            lost = true;
            best = xp;
        }
    }
    lost
}
