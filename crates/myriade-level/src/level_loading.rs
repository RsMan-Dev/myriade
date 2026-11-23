//! The chunk status steps the loading screen's chunk map shows.

/// `ChunkStatus`, as far as the chunk status map tells them apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Empty,
    StructureStarts,
    StructureReferences,
    Biomes,
    Terrain,
    Features,
    InitializeLight,
    Light,
    Spawn,
    Full,
}
