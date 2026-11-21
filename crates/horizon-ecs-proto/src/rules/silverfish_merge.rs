use horizon_core::geometry::Vec3;

/// `RandomStrollGoal`'s odds per ask: `reduced(10)` = 5.
const ODDS: f64 = 0.2;
/// The six cells a merge looks at, from the feet: down, up, north, south, west, east.
const DIRECTIONS: [[i32; 3]; 6] = [[0, -1, 0], [0, 1, 0], [0, 0, -1], [0, 0, 1], [-1, 0, 0], [1, 0, 0]];

/// The cell the merge reads for an animal at `p`: its feet, half a block up, floored.
pub fn feet_cell(p: Vec3) -> [i32; 3] {
    [p.x.floor() as i32, (p.y + 0.5).floor() as i32, p.z.floor() as i32]
}

/// The six cells around `base`, in the order of the directions.
pub fn host_cells(base: [i32; 3]) -> [[i32; 3]; 6] {
    DIRECTIONS.map(|d| [base[0] + d[0], base[1] + d[1], base[2] + d[2]])
}

/// The odds of one merge direction and of the stroll, with `hosts` host blocks.
pub fn odds(hosts: usize) -> (f64, f64) {
    let per_dir = ODDS / 6.0;
    (per_dir, (1.0 - per_dir * hosts as f64) * ODDS)
}

/// The number of asks until one succeeds, from a uniform draw `u` and the success odds `p`.
pub fn asks_until_success(u: f64, p: f64) -> i32 {
    let u = u.max(1.0e-12);
    (u.ln() / (1.0 - p).ln()).ceil().max(1.0) as i32
}

/// The merge direction a draw `u` picks among `hosts` directions of `per_dir` each, or none (the stroll).
pub fn merge_pick(u: f64, per_dir: f64, hosts: usize) -> Option<usize> {
    let mut acc = 0.0;
    for i in 0..hosts {
        acc += per_dir;
        if u < acc {
            return Some(i);
        }
    }
    None
}

/// The infested block of a host (`InfestedBlock`: the same stone with the `infested_` prefix).
pub fn infested(host: &str) -> &'static str {
    match host {
        "minecraft:cobblestone" => "minecraft:infested_cobblestone",
        "minecraft:stone_bricks" => "minecraft:infested_stone_bricks",
        "minecraft:mossy_stone_bricks" => "minecraft:infested_mossy_stone_bricks",
        "minecraft:cracked_stone_bricks" => "minecraft:infested_cracked_stone_bricks",
        "minecraft:chiseled_stone_bricks" => "minecraft:infested_chiseled_stone_bricks",
        "minecraft:deepslate" => "minecraft:infested_deepslate",
        _ => "minecraft:infested_stone",
    }
}
