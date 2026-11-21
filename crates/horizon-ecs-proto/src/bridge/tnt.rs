//! The primed TNT on the bridge: lit by a player or a mob (`prime_tnt`, with the push of its constructor and its `Fuse`), its blast at the
//! fuse's end (`tnt_explode`, explosion.rs `explode_tnt` through `explode_with`), and the events read at the step (`drain_tnt`).
use crate::bridge::host::EcsWorld;
use crate::explosion::ExplosionRng;
use crate::host_out_tnt::TntEvent;
use crate::projectile::Blast;
use crate::targeting::Who;
use crate::tnt::{self, Primed};
use crate::tnt_tick::TntOut;
use horizon_client_lib::mobs::MobWorld;
use horizon_core::geometry::Vec3;

impl EcsWorld {
    /// `PrimedTnt(level, x, y, z, owner)` with `fuse` ticks: its constructor's push (`push_of`, on the explosion's rng), `owner` kept for the
    /// credit, and the `Fuse` queued.
    pub fn prime_tnt(&mut self, mw: &MobWorld<'_>, at: [f64; 3], fuse: i32, owner: Option<Who>, power: f32) -> Entity {
        let at = Vec3::new(at[0], at[1], at[2]);
        self.lent(mw, |w| {
            let vel = tnt::push_of(&mut w.resource_mut::<ExplosionRng>().0);
            let e = tnt::spawn_primed(w, at, vel, fuse, power);
            if let Some(mut p) = w.get_mut::<Primed>(e) {
                p.owner = owner;
            }
            w.get_resource_or_insert_with(TntOut::default).0.push(TntEvent::Fuse { entity: e });
            e
        })
    }

    /// The host's blast of a primed TNT (`Explode`): `explode_tnt` (explosion.rs), credited to `owner`.
    pub fn tnt_explode(&mut self, mw: &MobWorld<'_>, at: [f64; 3], power: f32, owner: Option<Who>) {
        let b = Blast { at: Vec3::new(at[0], at[1], at[2]), radius: power, fire: false, owner, source: None };
        self.explode_with(mw, b, true);
    }

}
