use crate::*;


/// Which of a column's heights (`Heightmap.Types`) a run's `y` counts from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Heightmap {
    WorldSurface,
    MotionBlocking,
    MotionBlockingNoLeaves,
    OceanFloor,
}

/// A run: `height` blocks up a column from `y` (from the column's `surface` height when one is
/// given), set to palette entry `block`, only where filter `only` matches when one is given.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlockRun {
    pub x: i32,
    pub z: i32,
    pub y: i32,
    pub height: u32,
    pub surface: Option<Heightmap>,
    pub block: u32,
    pub only: Option<u32>,
}

/// A bulk edit of complete typed states and partial predicates, named by run indices; NBT is not supported.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct BlockEdit {
    /// Complete states to write; properties are never parsed from command strings.
    pub palette: Vec<BlockState>,
    /// Partial block or tag predicates; omitted properties are wildcards.
    pub filters: Vec<StatePredicate>,
    pub runs: Vec<BlockRun>,
}

/// What `read_columns` read: the blocks round each column's height (`bases`, none where it isn't loaded),
/// as indexes into `palette`, a column after another, and three biome tints a column (grass, foliage, water).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ColumnBlocks {
    /// Distinct complete states, including separate entries for different property combinations.
    pub palette: Vec<BlockState>,
    pub bases: Vec<Option<i32>>,
    pub blocks: Vec<u16>,
    pub tints: Vec<u32>,
}
