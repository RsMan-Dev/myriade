//! Rest of a cow on `Mob` (the hand-written memo, the same key and outputs as `ecs_cow/rest.rs`): a tick that starts from the very state the previous one started from, in unchanged
//! surroundings, ends where it ended (79 % of the ticks of a cow on flat ground). The step that
//! moves it (`travel`: the physics, the collisions, the fluids, the fall) is then not made again: its
//! outputs, kept from the tick that made them, are put back. Invisible by construction of the key and
//! by the equality test: the key holds everything `travel` reads, and a tick that drew a random, raised an
//! event or hurt the cow is never kept. It is a comparison of values (the same on a plain struct): the
//! bevy change ticks are not usable here, every step of the tick writes its fields.
use super::{MobEvent, MobWorld};
use crate::health::DamageLevel;
use myriade_level::world::RegionStamp;
use horizon_core::geometry::Vec3;

#[derive(Clone, Debug, Default)]
pub struct RestState {
    pub enabled: bool,
    pub memo: Option<Box<RestMemo>>,
    pub hits: u64,
    pub asks: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RestKey {
    position: [u64; 3],
    velocity: [u64; 3],
    yaw: u32,
    flags: [bool; 8],
    stuck: [u64; 3],
    supporting: Option<[i32; 3]>,
    fall: u64,
    input: [u32; 4],
    steps: [u32; 3],
    fluids: [u64; 2],
    fluid_flags: [bool; 4],
    baby: bool,
    stamp: RegionStamp,
    border: Option<[u64; 4]>,
}
#[derive(Clone, Debug)]
pub struct RestOut {
    position: Vec3,
    velocity: Vec3,
    flags: [bool; 4],
    supporting: Option<[i32; 3]>,
    stuck: Vec3,
    fall: f64,
    steps: [f32; 3],
    fluids: crate::mobs::living::Fluids,
}
#[derive(Clone, Debug)]
pub struct RestMemo {
    key: RestKey,
    out: RestOut,
}

/// The mutation of the test: the input and the speed leave the key (the test must then fail).


fn bits(v: Vec3) -> [u64; 3] {
    [v.x.to_bits(), v.y.to_bits(), v.z.to_bits()]
}

