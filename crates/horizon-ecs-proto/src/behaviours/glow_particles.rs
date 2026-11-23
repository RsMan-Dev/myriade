//! The glow of a glow squid: `Squid.aiStep` counts the dark timer down, then `GlowSquid.aiStep` adds one `glow` particle a tick within 0.6
//! blocks of its place (`getRandomX`, `getRandomY`, `getRandomZ`), whatever its AI. The hurt that sets the timer is not ported yet.
//! The module has no goal: `glow` reads the marker and the timer.
use super::{params, Inst, Module};
use crate::cow::*;
use crate::goals::Events;
use bevy_ecs::prelude::*;

pub static MODULE: Module = Module { name: "glow_particles", flags: 0, equip, ..Module::DEFAULT };

#[derive(Clone, Copy, Debug, Default)]
pub struct Params;

/// The glow squid's marker: the particle is emitted each tick.
#[derive(Component, Clone, Copy, Debug)]
pub struct GlowLight;

/// `Squid.darkTicks`: counted down each tick, zero when the squid is not dark.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct DarkTicks(pub i32);

fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let _ = params::<Params>(p);
    w.entity_mut(e).insert((GlowLight, DarkTicks::default()));
}

pub fn register(s: &mut bevy_ecs::schedule::Schedule) {
    s.add_systems(glow);
}

pub fn glow(mut q: Query<(&Pos, &mut Rng, &mut DarkTicks), With<GlowLight>>, mut events: ResMut<Events>) {
    for (p, mut rng, mut dark) in &mut q {
        if dark.0 > 0 {
            dark.0 -= 1;
        }
        let x = p.0.x + (rng.next_f64() * 2.0 - 1.0) * 0.6;
        let y = p.0.y + rng.next_f64();
        let z = p.0.z + (rng.next_f64() * 2.0 - 1.0) * 0.6;
        events.host.particles("glow", [x, y, z], [0.0; 3], 0.0, 1);
    }
}
