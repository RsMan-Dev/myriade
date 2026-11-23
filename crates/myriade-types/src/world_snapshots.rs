use crate::*;
use horizon_core::nbt::Tag;

/// Inputs to selecting a block's placement state.
#[derive(Clone, Debug, PartialEq)]
pub struct PlacementContext {
    /// Resolved destination of the block being placed.
    pub pos: BlockPos,
    /// Position of the clicked block.
    pub hit_pos: BlockPos,
    /// Clicked face.
    pub face: Direction,
    /// World-space click location.
    pub hit: [f64; 3],
    /// Whether the click originated inside the block's shape.
    pub inside: bool,
    /// Placer yaw in degrees.
    pub yaw: f32,
    /// Placer pitch in degrees.
    pub pitch: f32,
    /// Whether secondary use is active.
    pub sneaking: bool,
    /// The resolved item in the placing hand; `None` for an empty hand.
    pub item: Option<ItemKey>,
    /// Whether the clicked block itself is replaced.
    pub replace_clicked: bool,
}

impl PlacementContext {
    /// A placement at `pos`, with a click on `face` and an optional held item.
    /// The click starts at the block origin, with zero view angles and no flags set.
    pub fn new(pos: BlockPos, face: Direction, item: Option<ItemKey>) -> Self {
        Self {
            pos,
            hit_pos: pos,
            face,
            hit: [f64::from(pos.x), f64::from(pos.y), f64::from(pos.z)],
            inside: false,
            yaw: 0.0,
            pitch: 0.0,
            sneaking: false,
            item,
            replace_clicked: false,
        }
    }
}
