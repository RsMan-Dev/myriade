//! Calculations of the peek (`attached_face` peek goal): its chance per ask and the length of one peek.
use crate::rules::cat_state::reduced;

/// The ticks of one peek: `ticks` times `1 + draw` (the spread's draw), halved.
pub fn peek_time(ticks: i32, draw: i32) -> i32 {
    reduced(ticks * (1 + draw))
}
/// One ask succeeds with one chance in `reduced(one_in)`: `draw` gives the index drawn, which must be zero.
pub fn peek_roll(one_in: i32, draw: impl FnOnce(i32) -> i32) -> bool {
    draw(reduced(one_in)) == 0
}
