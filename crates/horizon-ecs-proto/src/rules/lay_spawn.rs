//! The cells searched for a spawn spot (`lay_spawn`): the Manhattan box around an origin.

/// `BlockPos.withinManhattan`: nearest first, then x, y, and +z before -z.
pub fn manhattan_cells(o: [i32; 3], range: i32) -> Vec<[i32; 3]> {
    let mut out = Vec::new();
    for depth in 0..=range * 3 {
        let max_x = range.min(depth);
        for x in -max_x..=max_x {
            let max_y = range.min(depth - x.abs());
            for y in -max_y..=max_y {
                let z = depth - x.abs() - y.abs();
                if z <= range {
                    out.push([o[0] + x, o[1] + y, o[2] + z]);
                    if z != 0 {
                        out.push([o[0] + x, o[1] + y, o[2] - z]);
                    }
                }
            }
        }
    }
    out
}
