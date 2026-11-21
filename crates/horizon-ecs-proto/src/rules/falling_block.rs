//! The numbers of a falling anvil (`AnvilBlock.falling`): the steps it fell, its damage, its chance to wear and the next state of its wear.

/// The steps an anvil fell: `ceil(fell - 1)`, negative when it fell less than a block.
pub fn anvil_steps(fell: f64) -> i32 {
    (fell - 1.0).ceil() as i32
}

/// The damage of a fall of `steps`: 2 a step, at most 40.
pub fn anvil_damage(steps: i32) -> f32 {
    ((steps as f32 * 2.0).floor() as i32).min(40) as f32
}

/// The chance that a fall of `steps` wears the anvil one step: `0.05 + 0.05 * steps`.
pub fn anvil_wear_chance(steps: i32) -> f32 {
    0.05 + steps as f32 * 0.05
}

/// The anvil a worn one becomes, by its kind (`""`, `"chipped_"`); None when it is destroyed.
pub fn anvil_worn_name(kind: &str) -> Option<&'static str> {
    match kind {
        "" => Some("minecraft:chipped_anvil"),
        "chipped_" => Some("minecraft:damaged_anvil"),
        _ => None,
    }
}
