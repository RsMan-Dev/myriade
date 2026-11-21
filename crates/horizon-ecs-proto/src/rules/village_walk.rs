//! The village walk's sections: the section of a position, its centre, and the sections around a section.
use horizon_core::geometry::Vec3;

/// The village section that holds a position.
pub fn section_of(p: Vec3) -> [i32; 3] {
    [(p.x.floor() as i32) >> 4, (p.y.floor() as i32) >> 4, (p.z.floor() as i32) >> 4]
}

/// `SectionPos.center` as `Vec3.atBottomCenterOf` reads it.
pub fn section_center(s: [i32; 3]) -> [f64; 3] {
    [f64::from(s[0] * 16 + 8) + 0.5, f64::from(s[1] * 16 + 8), f64::from(s[2] * 16 + 8) + 0.5]
}

/// The sections within two of `here` on each axis, in the order z, y, x.
pub fn nearby_sections(here: [i32; 3]) -> Vec<[i32; 3]> {
    let mut v = Vec::new();
    for z in here[2] - 2..=here[2] + 2 {
        for y in here[1] - 2..=here[1] + 2 {
            for x in here[0] - 2..=here[0] + 2 {
                v.push([x, y, z]);
            }
        }
    }
    v
}
