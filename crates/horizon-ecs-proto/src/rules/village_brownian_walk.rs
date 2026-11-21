//! The cells of the inside walk's search: the 3x3x3 block cube around `here`, z outermost, x innermost (the order before the shuffle).

/// The 27 cells around `here` (itself included), in the order the search lists them.
pub fn neighbours(here: [i32; 3]) -> Vec<[i32; 3]> {
    (-1..=1).flat_map(|z| (-1..=1).flat_map(move |y| (-1..=1).map(move |x| [here[0] + x, here[1] + y, here[2] + z]))).collect()
}
