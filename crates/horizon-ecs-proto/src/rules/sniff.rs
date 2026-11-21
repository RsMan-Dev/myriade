//! The seed hunt (`brain/behaviours/sniff.rs`): the cells of the head and of the feet, the cell below a dig, the places dug, the seed's seed.
use crate::IFloor as _;
use horizon_core::geometry::trig::Mth;

/// `Sniffer.getHeadBlock`: 2.25 blocks along the facing of `yaw` degrees, at the feet plus 0.2 (the head pitch is not read).
pub fn head_cell(p: [f64; 3], yaw: f32) -> [i32; 3] {
    let y = -yaw * (std::f32::consts::PI / 180.0) - std::f32::consts::PI;
    let (fx, fz) = (f64::from(-y.msin()), f64::from(-y.mcos()));
    [(p[0] + fx * 2.25).ifloor(), (p[1] + f64::from(0.2_f32)).ifloor(), (p[2] + fz * 2.25).ifloor()]
}

/// `Sniffer.getOnPos`: the block under the feet (0.2 below them).
pub fn on_cell(p: [f64; 3]) -> [i32; 3] {
    [p[0].ifloor(), (p[1] - f64::from(0.2_f32)).ifloor(), p[2].ifloor()]
}

/// The block below a land position `p`, the cell a dig reads.
pub fn below_cell(p: [f64; 3]) -> [i32; 3] {
    [p[0].ifloor(), p[1].ifloor() - 1, p[2].ifloor()]
}

/// The places dug, newest first: `p` in front, the list cut to `max` before it is added.
pub fn explored_after(mut list: Vec<[i32; 3]>, p: [i32; 3], max: usize) -> Vec<[i32; 3]> {
    list.truncate(max);
    list.insert(0, p);
    list
}

/// The loot seed of a dig: the first draw is the high half, the second the low half.
pub fn loot_seed(hi: u32, lo: u32) -> i64 {
    (u64::from(hi) << 32 | u64::from(lo)) as i64
}
