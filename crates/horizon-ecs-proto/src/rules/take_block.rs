//! The enderman's block taking: the wait of the draw, the cell drawn in its box, and the sight line that must reach that cell.
use horizon_core::geometry::Vec3;

/// The wait of the next take, in ticks: twice the geometric draw.
pub fn take_wait(geometric: i32) -> i32 {
    2 * geometric
}

/// The cell drawn in the 4 x 3 x 4 box around `p`, from three uniform draws `u` in [0, 1).
pub fn take_cell(p: Vec3, u: [f64; 3]) -> [i32; 3] {
    [(p.x - 2.0 + u[0] * 4.0).floor() as i32, (p.y + u[1] * 3.0).floor() as i32, (p.z - 2.0 + u[2] * 4.0).floor() as i32]
}

/// The sight line of the reach test: from the centre of the enderman's column at the cell's height, to the cell's centre.
pub fn reach_line(p: Vec3, cell: [i32; 3]) -> (Vec3, Vec3) {
    let from = Vec3::new(p.x.floor() + 0.5, f64::from(cell[1]) + 0.5, p.z.floor() + 0.5);
    let to = Vec3::new(f64::from(cell[0]) + 0.5, f64::from(cell[1]) + 0.5, f64::from(cell[2]) + 0.5);
    (from, to)
}
