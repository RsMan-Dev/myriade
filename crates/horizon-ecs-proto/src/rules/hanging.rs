//! The geometry of a hanging entity (`BlockAttachedEntity`): the step of its face, the block it is fixed to, the cells of its footprint,
//! and the period of its survival check.

/// `BlockAttachedEntity.tick`: the checks fall on the 101st tick after the birth.
pub const CHECK_PERIOD: u64 = 101;

/// The face's outward step (`Direction.getStep`, the order of `Direction.get3DDataValue`).
pub fn step(face: u8) -> [i32; 3] {
    [[0, -1, 0], [0, 1, 0], [0, 0, -1], [0, 0, 1], [-1, 0, 0], [1, 0, 0]][usize::from(face % 6)]
}

/// The block the entity is fixed to: one step behind its cell.
pub fn support(pos: [i32; 3], face: u8) -> [i32; 3] {
    let s = step(face);
    [pos[0] - s[0], pos[1] - s[1], pos[2] - s[2]]
}

/// The direction a painting's width runs in (`Direction.getCounterClockWise`), per face.
pub fn left(face: u8) -> [i32; 3] {
    [[0, 0, 0], [0, 0, 0], [-1, 0, 0], [1, 0, 0], [0, 0, 1], [0, 0, -1]][usize::from(face % 6)]
}

/// The cells a footprint of `n` cells spans from its cell: `Painting.calculateBoundingBox` centres an odd size on the cell and an
/// even one on its edge, so the first offset is `-(n-1)/2` and the last `n/2`.
pub fn span(n: u8) -> std::ops::RangeInclusive<i32> {
    -((i32::from(n) - 1) / 2)..=(i32::from(n) / 2)
}

/// The cells of the footprint of a hanging entity on `face` at `pos` (in front of the wall).
pub fn cells(pos: [i32; 3], face: u8, size: (u8, u8)) -> Vec<[i32; 3]> {
    let l = left(face);
    let mut out = Vec::new();
    for a in span(size.0) {
        for b in span(size.1) {
            out.push(std::array::from_fn(|i| pos[i] + a * l[i] + b * i32::from(i == 1)));
        }
    }
    out
}

/// The ticks from `now` to the next survival check: the first multiple of the period after the birth that is strictly after now.
pub fn check_delay(born: u64, now: u64) -> i32 {
    (born + CHECK_PERIOD * ((now - born) / CHECK_PERIOD + 1) - now) as i32
}
