/// The chunk (x, z) a position lies in.
pub fn chunk_of(p: [f64; 3]) -> (i32, i32) {
    ((p[0].floor() as i32) >> 4, (p[2].floor() as i32) >> 4)
}

/// Whether the chunk `c` is close to a player at `q`: within 8 chunks on each axis, and its centre less than 128 blocks away.
pub fn chunk_near(c: (i32, i32), q: [f64; 3]) -> bool {
    let (qcx, qcz) = chunk_of(q);
    (qcx - c.0).abs() <= 8 && (qcz - c.1).abs() <= 8 && {
        let (x, z) = (f64::from(c.0 * 16 + 8) - q[0], f64::from(c.1 * 16 + 8) - q[2]);
        x * x + z * z < 16384.0
    }
}
