//! Calculations of the random stand (`RandomStand`): the asks until a one-in-a-thousand draw succeeds from a counter, and the
//! tick of the stand from its first ask.

/// The asks until the draw succeeds: the counter `next` starts the count (a negative value is spent as asks first), then
/// each ask adds one and draws `draw(1000)`, which must be under the count.
pub fn asks_until_draw(next: i32, mut draw: impl FnMut(i32) -> i32) -> i32 {
    let (mut asks, mut n) = (0, next);
    if n <= 0 {
        asks -= n;
        n = 0;
    }
    loop {
        asks += 1;
        n += 1;
        if draw(1000) < n {
            return asks;
        }
    }
}
/// The tick of the stand: two ticks per ask after the first, which is at `first`.
pub fn stand_at(first: u64, asks: i32) -> u64 {
    first + 2 * (asks as u64 - 1)
}
