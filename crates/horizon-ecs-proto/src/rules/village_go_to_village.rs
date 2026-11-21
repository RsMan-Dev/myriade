//! The village distance, in sections (16 blocks): the Chebyshev distance to the nearest village section, at most `VILLAGE_CAP`.

/// The distance beyond which a section is not counted as near a village.
pub const VILLAGE_CAP: i32 = 7;

/// The section (16x16x16 blocks) that holds the point `p`.
pub fn section(p: [f64; 3]) -> [i32; 3] {
    [p[0].floor() as i32 >> 4, p[1].floor() as i32 >> 4, p[2].floor() as i32 >> 4]
}

/// `sectionsToVillage`: the Chebyshev distance in sections to the nearest village section, at most 7.
pub fn to_village(villages: &[[i32; 3]], s: [i32; 3]) -> i32 {
    villages.iter().map(|v| (0..3).map(|i| (v[i] - s[i]).abs()).max().unwrap_or(0)).min().unwrap_or(VILLAGE_CAP).min(VILLAGE_CAP)
}
