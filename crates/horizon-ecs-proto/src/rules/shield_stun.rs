/// The stars of a stun: one tick in six over the stun, given as one burst of their count.
pub fn stun_stars(ticks: i32) -> u32 {
    ticks as u32 / 6
}
