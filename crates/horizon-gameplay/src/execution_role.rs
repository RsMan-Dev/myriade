//! The program is only a dedicated server: a Game's execution role is always
//! `DedicatedServer`, fixed at construction.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutionRole {
    DedicatedServer,
}

impl ExecutionRole {
    /// Select the phase once at an owning data-load boundary.
    pub fn data_load_origin(self) -> horizon_world::native_load_context::DataLoadOrigin {
        horizon_world::native_load_context::DataLoadOrigin::ServerDataLoad
    }

    /// The side a menu, recipe or player-list loop of this game's runs on.
    pub fn loop_side(self) -> horizon_world::native_load_context::LoopSide {
        horizon_world::native_load_context::LoopSide::Server
    }
}
