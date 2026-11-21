//! The lightning on the bridge: a strike is queued as `MiscEvent::Lightning` for the client, and the bolt runs on its own deadlines
//! (`lightning.rs`). No system visits it: it is awake only on its wheel entries, then gone.
use crate::colony_register::MiscOut;
use crate::explosion::ExplosionRng;
use crate::host_out_misc::MiscEvent;
use crate::lightning::{self, BoltLevel, Strikes};
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;

/// A bolt at `at`, drawn from the explosions' generator for its seed. `cosmetic`: a skeleton trap's, which strikes nothing. `level`: the
/// host's blocks for the strike (fire, rod, copper), kept for the bolts of this world until the next strike.
pub fn strike(w: &mut World, at: Vec3, cosmetic: bool, level: Option<Box<dyn BoltLevel>>) -> Entity {
    w.insert_resource(Strikes(level));
    let seed = u64::from(w.resource_mut::<ExplosionRng>().0.next_u32());
    let e = lightning::strike(w, at, cosmetic, seed);
    w.get_resource_or_insert_with(MiscOut::default).0.push(MiscEvent::Lightning { at: [at.x, at.y, at.z], cosmetic });
    e
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A bolt is queued for the client, and it is gone once its life and its flashes are spent (at most 3 flashes of at most 12 ticks).
    #[test]
    fn a_bolt_is_shown_then_gone() {
        let mut herd = crate::cow::Herd::new(0, 4.0, false, false, 7);
        let at = Vec3::new(0.5, 64.0, 0.5);
        let e = strike(&mut herd.world, at, true, None);
        let out = &herd.world.resource::<MiscOut>().0;
        assert_eq!(out.as_slice(), &[MiscEvent::Lightning { at: [0.5, 64.0, 0.5], cosmetic: true }]);
        for _ in 0..60 {
            herd.tick();
        }
        assert!(herd.world.get_entity(e).is_err());
    }
}
