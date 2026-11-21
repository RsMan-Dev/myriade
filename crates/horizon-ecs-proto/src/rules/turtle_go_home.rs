//! The turtle's walk home: the wait of the chance, the aim at home, and the test that keeps the walk going.

/// The wait of the chance's deadline, in ticks: `2 * (geometric + 1)`, from the geometric draw.
pub fn chance_wait(geometric: i32) -> i32 {
    2 * (geometric + 1)
}

/// The point the walk leads toward: the home block's centre on x and z, its floor on y.
pub fn home_aim(home: [i32; 3]) -> [f64; 3] {
    [f64::from(home[0]) + 0.5, f64::from(home[1]), f64::from(home[2]) + 0.5]
}

/// Whether the walk goes on: the turtle is far from home, not stuck, and has not counted too many ticks near home.
pub fn walk_goes_on(far: bool, stuck: bool, close_ticks: i32, give_up: i32) -> bool {
    far && !stuck && close_ticks <= give_up
}
