use crate::*;


/// A block requested once through [`Registrar::block_key`](BlocksRegistrar::block_key). Its load-owned slot survives reload.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BlockKey(pub u32);

/// What an explosion does to blocks (`Explosion.BlockInteraction`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockInteraction {
    /// Blocks are left alone.
    Keep,
    /// Blocks are destroyed and drop everything.
    Destroy,
    /// Blocks are destroyed and their drops may decay.
    DestroyWithDecay,
    /// A wind burst: it pushes, and triggers doors, buttons, bells and the like.
    TriggerBlock,
}

/// Supported vanilla `Block.UPDATE_*` flags for a level block write.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BlockUpdateFlags(pub i32);

impl BlockUpdateFlags {
    /// No update flags.
    pub const NONE: Self = Self(0);
    /// Notify neighboring blocks.
    pub const NEIGHBORS: Self = Self(1);
    /// Send the block update to clients.
    pub const CLIENTS: Self = Self(2);
    /// Suppress the client-side block update notification.
    pub const INVISIBLE: Self = Self(4);
    /// Skip neighbor shape updates.
    pub const KNOWN_SHAPE: Self = Self(16);
    /// Suppress drops caused by neighbor shape updates.
    pub const SUPPRESS_DROPS: Self = Self(32);
    /// The replacement is caused by a piston.
    pub const MOVE_BY_PISTON: Self = Self(64);
    /// Skip wire shape updates.
    pub const SKIP_SHAPE_UPDATE_ON_WIRE: Self = Self(128);
    /// Skip the removed block entity's side effects, including container spills.
    pub const SKIP_BLOCK_ENTITY_SIDE_EFFECTS: Self = Self(256);
    /// Skip the new block's on-place behavior.
    pub const SKIP_ON_PLACE: Self = Self(512);
    // hidden: a game-side helper, not part of the mod surface
    #[doc(hidden)]
    pub fn bits(self) -> i32 {
        self.0
    }
}

impl std::ops::BitOr for BlockUpdateFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

/// An effect a fake player's action is stopped from starting. Kinds may be added.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FakeEffect {
    /// A menu or screen: a container, a station, a sign's editor.
    Menu,
    /// Sleeping in a bed.
    Sleep,
    /// Riding an entity or a vehicle.
    Ride,
    /// Going to another dimension.
    ChangeDimension,
}

/// Where a block use clicked, as vanilla's `BlockHitResult` says it. Fields
/// may be added: build one with [`BlockHit::new`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlockHit {
    /// The point clicked, in world coordinates (`getLocation`).
    pub point: [f64; 3],
    /// The face clicked, `[0, 1, 0]` for the top (`getDirection`).
    pub face: [i32; 3],
    /// The ray started inside the block's shape (`isInside`).
    pub inside: bool,
}

impl BlockHit {
    /// New.
    pub const fn new(point: [f64; 3], face: [i32; 3], inside: bool) -> Self {
        Self {
            point,
            face,
            inside,
        }
    }
}
