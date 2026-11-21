/// The target is in reach of the beam: within `range_h` blocks horizontally, under `range_v` vertically (offsets `dx`, `dy`, `dz`).
pub fn in_reach(dx: f64, dy: f64, dz: f64, range_h: f64, range_v: f64) -> bool {
    dx * dx + dz * dz < range_h * range_h && dy.abs() < range_v
}

/// The beam from `src` to `eye`: its unit direction (zero when they coincide) and its length.
pub fn beam(src: [f64; 3], eye: [f64; 3]) -> ([f64; 3], f64) {
    let d = [eye[0] - src[0], eye[1] - src[1], eye[2] - src[2]];
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    let n = if len < 1.0e-5 { [0.0; 3] } else { d.map(|c| c / len) };
    (n, len)
}

/// The number of particle steps of a beam of length `len`: its floor plus 7.
pub fn beam_steps(len: f64) -> i32 {
    len.floor() as i32 + 7
}

/// The point `k` steps along the beam from `src` in direction `n`.
pub fn beam_point(src: [f64; 3], n: [f64; 3], k: i32) -> [f64; 3] {
    let s = f64::from(k);
    [src[0] + n[0] * s, src[1] + n[1] * s, src[2] + n[2] * s]
}
