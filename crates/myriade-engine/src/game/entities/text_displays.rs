//! Server text displays: metadata, interpolation, billboard transforms and world text.
use myriade_engine::text_display::TextDisplayData;

pub(super) struct RemoteTextDisplay {
    pub data: TextDisplayData,
    from: TextDisplayData,
    shown: TextDisplayData,
    position: glam::DVec3,
    previous: glam::DVec3,
    target: glam::DVec3,
    rotation: [f32; 2],
    old_rotation: [f32; 2],
    target_rotation: [f32; 2],
    steps: i32,
    age: i64,
    start: i64,
    duration: i32,
    dirty: bool,
    start_dirty: bool,
    duration_dirty: bool,
    initialized: bool,
    /// `Display.lastProgress`: the progress the last frame drew, where a new
    /// interpolation starts from.
    last_progress: f32,
    /// `Entity.vehicle`: the mob it rides (`SetPassengers`), which places it
    /// every tick (`positionRider`), as NPC plugins hang a name over a head.
    pub(super) vehicle: Option<u32>,
}

impl RemoteTextDisplay {
    /// Where a passenger of this display sits, this tick and the last
    /// (`Display`'s box is 0 by 0, so its passenger attachment is its feet).
    pub(super) fn seat(&self) -> (glam::DVec3, glam::DVec3) {
        (self.position, self.previous)
    }
    /// `Entity.positionRider` through `setPos`: where its vehicle seats it this
    /// tick. The previous position stays the last tick's, so it moves smoothly
    /// with the vehicle; a teleport's interpolation no longer applies.
    pub(super) fn ride(&mut self, at: glam::DVec3) {
        self.position = at;
        self.target = at;
        self.steps = 0;
    }
    pub(super) fn position(&self) -> glam::DVec3 {
        self.position
    }
}
