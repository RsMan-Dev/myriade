//! The outputs of the item frames and paintings toward the host, one per act the host follows (sound, comparators, block change,
//! drops, map markers). Not in `host_out.rs`: at fusion this enum becomes `HostEvent::Frame(FrameEvent)`, and `HostOut::frame` pushes
//! it. Each event carries `who` (the player, None when there is none) and the `Spot` of the hanging entity.
use crate::targeting::Who;
use myriade_engine_lib::items::ItemStack;

/// Where a hanging entity is: its air cell, the face it hangs on, and whether it is a glow frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spot {
    pub at: [i32; 3],
    pub face: u8,
    pub glow: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum FrameEvent {
    /// A frame hung on `spot`.
    Placed { who: Option<Who>, spot: Spot },
    /// A painting hung on `spot`.
    PaintingPlaced { who: Option<Who>, spot: Spot },
    /// `who` put `stack` in the empty frame on `spot`: the host consumes it (and checks the map markers limit).
    Put { who: Option<Who>, spot: Spot, stack: ItemStack },
    /// The frame on `spot` turned one step.
    Rotated { who: Option<Who>, spot: Spot },
    /// The item came out of the frame on `spot`, falling at `drop_at`; `drop` when the drops are on.
    Removed { who: Option<Who>, spot: Spot, stack: ItemStack, drop: bool, drop_at: [f64; 3] },
    /// An empty frame on `spot` broke and went; `drop` when its drop is on, falling at `drop_at`.
    Broke { who: Option<Who>, spot: Spot, drop: bool, drop_at: [f64; 3] },
    /// A painting on `spot` broke and went; `drop` when its drop is on, falling at `drop_at`.
    PaintingBroke { who: Option<Who>, spot: Spot, drop: bool, drop_at: [f64; 3] },
}
