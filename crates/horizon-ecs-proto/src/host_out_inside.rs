//! The "sensitive cells touched" output toward the host: an entity that triggers blocks moved, and its box reaches into cells whose block
//! reacts to an entity inside (`entityInside`: plates, tripwires, buttons, detector rails, lily pads...). They queue in `InsideOut`; the
//! channel carries them as `HostEvent::Inside`; the host calls its `entity_inside` for each cell.
use bevy_ecs::prelude::*;

#[derive(Clone, Debug, PartialEq)]
pub struct InsideEvent {
    pub entity: Entity,
    /// The entity is a boat (a lily pad breaks under it, `LilyPadBlock.entityInside`).
    pub boat: bool,
    /// The sensitive cells its box reaches into (deflated by 1e-5 as `Entity.checkInsideBlocks`), in x, y, z order.
    pub cells: Vec<[i32; 3]>,
    /// Its feet (centre x, bottom y, centre z), half its width and whether it stands on the ground: the big dripleaf reads them.
    pub at: [f64; 3],
    pub half: f64,
    pub on_ground: bool,
}

/// The queue of the touched cells, drained by the host.
#[derive(Resource, Default)]
pub struct InsideOut(pub Vec<InsideEvent>);
