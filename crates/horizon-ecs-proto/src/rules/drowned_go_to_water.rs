//! The drowned's water point: the candidate from three draws around its block, and the centre of the water block.

/// A candidate water point: the base block moved by the draws (x and z in -10..10, y from 2 up to 5 down).
pub fn candidate(base: [i32; 3], draws: [i32; 3]) -> [i32; 3] {
    [base[0] + draws[0] - 10, base[1] + 2 - draws[1], base[2] + draws[2] - 10]
}

/// The centre of the block `q`, the point the walk aims at.
pub fn block_centre(q: [i32; 3]) -> [f64; 3] {
    [f64::from(q[0]) + 0.5, f64::from(q[1]), f64::from(q[2]) + 0.5]
}
