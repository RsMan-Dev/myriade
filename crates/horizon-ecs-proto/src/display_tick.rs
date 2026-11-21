//! The interpolation of a display as the server writes it: its delay and duration are `Shown`'s data, which the clients play. The
//! server holds no clock for them, so nothing is armed and an unmoved display costs nothing per tick.
use crate::display::{self, Shown};
use bevy_ecs::prelude::*;

/// Sets the interpolation of display `e` to `delay` then `duration` ticks; one update when it changed. False when nothing changed or
/// `e` has no `Shown`.
pub fn start_interpolation(w: &mut World, e: Entity, delay: i32, duration: i32) -> bool {
    let Some(s) = w.get::<Shown>(e).copied() else { return false };
    display::set(w, e, Shown { interpolation_delay: delay, interpolation_duration: duration, ..s })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::display::{self, BlockState, Transform};
    use crate::goals::Events;
    use horizon_core::geometry::Vec3;
    use horizon_world::registry::BlockStateId;

    #[test]
    fn interpolation_is_written_once_and_an_unmoved_display_stays_still() {
        let mut w = World::new();
        w.init_resource::<Events>();
        let e = display::spawn(&mut w, Vec3::new(0.0, 0.0, 0.0), Transform::default(), Shown::default(), BlockState(BlockStateId(1)));
        assert!(!start_interpolation(&mut w, e, 0, 0));
        assert!(start_interpolation(&mut w, e, 2, 5));
        assert!(!start_interpolation(&mut w, e, 2, 5));
        let s = w.get::<Shown>(e).expect("the display keeps its way of showing");
        assert_eq!((s.interpolation_delay, s.interpolation_duration), (2, 5));
    }
}
