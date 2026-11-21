/// `ArmorStand.getClickedSlot` on a click at `rel_y` above the stand's feet (divided by 0.5 when small): the slot whose height it is,
/// the main hand when none, and only where the slot has something to take (`has`).
pub fn clicked_slot(small: bool, rel_y: f32, has: impl Fn(usize) -> bool) -> usize {
    let y = if small { rel_y / 0.5 } else { rel_y };
    if y >= 0.1 && y < 0.1 + if small { 0.8 } else { 0.45 } && has(2) {
        2
    } else if y >= 0.9 + if small { 0.3 } else { 0.0 } && y < 0.9 + if small { 1.0 } else { 0.7 } && has(4) {
        4
    } else if y >= 0.4 && y < 0.4 + if small { 1.0 } else { 0.8 } && has(3) {
        3
    } else if y >= 1.6 && has(5) {
        5
    } else if !has(0) && has(1) {
        1
    } else {
        0
    }
}

/// `ArmorStand.hurtServer` for a blow: a creative player or a `#always_kills_armor_stands` blow breaks it at once; otherwise a blow within
/// five ticks of the last wobble (`last`) breaks it, and any other only wobbles it.
pub fn blow_breaks(last: Option<u64>, now: u64, creative: bool, always_kills: bool) -> bool {
    creative || always_kills || last.is_some_and(|t| now - t <= 5)
}
