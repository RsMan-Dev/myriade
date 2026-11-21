//! The rules of the End crystal: the cell of its feet, where its fire goes, and which blows it refuses.
use horizon_core::geometry::Vec3;

/// The block at the feet of a crystal at `p`: where it stands and where its fire goes (`blockPosition`).
pub fn cell(p: Vec3) -> [i32; 3] {
    [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32]
}

/// Whether a blow is refused: fire, a dragon's blow, or a shielded crystal's blow unless it is creative.
pub fn refused(fire: bool, dragon: bool, shielded: bool, creative: bool) -> bool {
    fire || dragon || (shielded && !creative)
}
