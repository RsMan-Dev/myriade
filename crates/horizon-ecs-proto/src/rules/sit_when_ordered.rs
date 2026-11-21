/// The owner is within 12 blocks (`dist_sqr` below 144) and was hurt a moment ago: the pet gets up to help.
pub fn owner_needs_help(dist_sqr: f64, attacked: bool) -> bool {
    dist_sqr < 144.0 && attacked
}
