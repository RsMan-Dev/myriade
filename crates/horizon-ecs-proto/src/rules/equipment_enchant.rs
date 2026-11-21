//! The rules of a mob's new piece: the enchantment cost's upper bound, and the drowned's main hand.
use horizon_core::random::RandomSource;

/// The upper bound of the enchantment cost of a piece at `special`: `5 + 17 * special`, floored.
pub fn cost_top(special: f32) -> i32 {
    5 + (special * 17.0) as i32
}

/// The drowned's main hand: `float > 0.9`, then `trident` in 10 of 16, else a fishing rod; `None` when the hand stays empty.
pub fn drowned_weapon<R: RandomSource>(random: &mut R, trident: &'static str) -> Option<&'static str> {
    if f64::from(random.next_float()) > 0.9 {
        Some(if random.next_int_bound(16).unwrap_or(0) < 10 { trident } else { "minecraft:fishing_rod" })
    } else {
        None
    }
}
