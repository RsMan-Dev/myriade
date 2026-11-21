//! Choix de la cloche à sonner (`RingBell`) : la plus proche parmi les cloches candidates, dans une portée au carré donnée.
use horizon_core::geometry::Vec3;

/// The nearest block of `candidates` whose centre is closer than `reach_sq` (squared) to `at`; `None` when there is none.
pub fn nearest_within(candidates: impl Iterator<Item = [i32; 3]>, at: Vec3, reach_sq: f64) -> Option<[i32; 3]> {
    let dist = |q: [i32; 3]| crate::rules::block_reach::centre(q).distance_squared(at);
    candidates.filter(|&q| dist(q) < reach_sq).min_by(|a, b| dist(*a).total_cmp(&dist(*b)))
}
