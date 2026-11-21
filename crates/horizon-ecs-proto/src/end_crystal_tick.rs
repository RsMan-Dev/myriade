//! The end crystals on the bridge: a crystal has no tick (its fire is set by its first wake-up, `end_crystal.rs`). A blow that destroys it
//! queues `MiscEvent::CrystalBlast` for the host, next to the fight's own record.
use crate::colony_register::MiscOut;
use crate::cow::Pos;
use crate::end_crystal::{self, Blow};
use crate::host_out_misc::MiscEvent;
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;

/// A crystal at `at` (its feet), `show_bottom` as saved.
pub fn spawn(w: &mut World, at: Vec3, show_bottom: bool) -> Entity {
    end_crystal::spawn(w, at, show_bottom)
}

/// A blow on `e` (see `end_crystal::hurt`); when the crystal goes, its blast is queued.
pub fn hurt(w: &mut World, e: Entity, blow: Blow) -> bool {
    let Some(at) = w.get::<Pos>(e).map(|p| p.0) else { return false };
    let gone = end_crystal::hurt(w, e, blow);
    if gone {
        w.get_resource_or_insert_with(MiscOut::default).0.push(MiscEvent::CrystalBlast { at: [at.x, at.y, at.z] });
    }
    gone
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::targeting::Who;

    /// A blow that goes destroys the crystal and queues its blast once; a shielded crystal refuses an ordinary blow.
    #[test]
    fn a_crystal_blast_is_queued_once() {
        let mut herd = crate::cow::Herd::new(0, 4.0, false, false, 7);
        let at = Vec3::new(0.5, 64.0, 0.5);
        let e = spawn(&mut herd.world, at, true);
        let blow = Blow { by: Some(Who::Player(1)), fire: false, explosion: true, creative: false, dragon: false };
        assert!(hurt(&mut herd.world, e, blow));
        assert!(!hurt(&mut herd.world, e, blow));
        let out = &herd.world.resource::<MiscOut>().0;
        assert_eq!(out.as_slice(), &[MiscEvent::CrystalBlast { at: [0.5, 64.0, 0.5] }]);
    }
}
