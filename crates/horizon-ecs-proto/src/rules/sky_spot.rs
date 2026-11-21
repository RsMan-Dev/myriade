/// The offset of one candidate spot from the feet: x within ±10, y within ±3, z within ±10, drawn in that order.
pub fn sky_offset(draw: &mut impl FnMut(i32) -> i32) -> [i32; 3] {
    let mut o = [0; 3];
    for (k, d) in [10, 3, 10].into_iter().enumerate() {
        o[k] = draw(2 * d + 1) - d;
    }
    o
}
