//! Values of the egg-laying goal (`lay_egg`): the closeness to the anchor's centre and the countdown halved.

/// `closerToCenterThan`: the centre of the anchor block is nearer than `d` to `p`.
pub fn near_anchor(a: [i32; 3], p: [f64; 3], d: f64) -> bool {
    let v = [f64::from(a[0]) + 0.5 - p[0], f64::from(a[1]) + 0.5 - p[1], f64::from(a[2]) + 0.5 - p[2]];
    v[0] * v[0] + v[1] * v[1] + v[2] * v[2] < d * d
}

/// The countdown to the next search: half of `n`, rounded up.
pub fn halved_up(n: i32) -> i32 {
    (n + 1) / 2
}
