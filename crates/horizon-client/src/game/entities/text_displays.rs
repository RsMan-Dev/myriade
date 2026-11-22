//! Server text displays: metadata, interpolation, billboard transforms and world text.
use horizon_client::text_display::TextDisplayData;

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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_and_position_interpolation_keep_their_separate_clocks() {
        let mut d = RemoteTextDisplay::new(Shows::Text, [0.0; 3], 0.0);
        d.tick();
        d.metadata(&[
            (8, 1, Value::Int(0)),
            (9, 1, Value::Int(4)),
            (10, 1, Value::Int(2)),
            (11, 0, Value::Floats(vec![4.0, 0.0, 0.0])),
        ]);
        d.tick();
        assert_eq!(d.shown.blend(&d.from, d.progress(0.5)).translation[0], 0.5);
        d.move_to([2.0, 0.0, 0.0], Some((90.0, 0.0)));
        d.tick();
        assert_eq!(d.position.x, 1.0);
        assert_eq!(d.rotation[0], 45.0);
        d.tick();
        assert_eq!(d.position.x, 2.0);
        assert_eq!(d.shown.blend(&d.from, d.progress(0.0)).translation[0], 2.0);
    }

    /// A display riding a vehicle sits where the vehicle seats it each tick,
    /// moving from the last tick's seat, and a teleport sent meanwhile does
    /// not drag it off the seat.
    #[test]
    fn a_ridden_display_moves_from_seat_to_seat() {
        let mut d = RemoteTextDisplay::new(Shows::Text, [0.0, 64.0, 0.0], 0.0);
        d.metadata(&[(10, 1, Value::Int(3))]);
        d.move_to([9.0, 64.0, 9.0], None);
        let seat = glam::DVec3::new(0.0, 65.8, 0.0);
        d.ride(seat);
        assert_eq!(d.position, seat);
        d.tick();
        assert_eq!((d.previous, d.position), (seat, seat));
        let next = glam::DVec3::new(1.0, 65.8, 0.0);
        d.ride(next);
        assert_eq!((d.previous, d.position), (seat, next));
    }
}
