//! An effect that answers a hurt by spawning creatures (`MobEffect.onMobHurt`): Infested lets silverfish out of the bearer. Nothing runs per
//! tick: the damage step calls `on_hurt` for each hit that lands, and an entity without `Effects` returns at once.
use crate::cow::{species_table, Body, Pos, Rng, Vel, Yaw};
use crate::effects::{self, Kind};
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;

/// One effect of this kind: the roll and what comes out.
pub struct HurtSpawn {
    pub effect: Kind,
    /// `chanceToSpawn`: spawns when `nextFloat <= chance`.
    pub chance: f32,
    /// The count is `1 + nextInt(extra)` (`Mth.randomBetween(1, extra)`).
    pub extra: i32,
    pub species: &'static str,
    /// What the new creature says (`playSound` at its own position).
    pub sound: &'static str,
}
pub const TABLE: &[HurtSpawn] =
    &[HurtSpawn { effect: Kind::Infested, chance: 0.1, extra: 2, species: crate::species::lists::SILVERFISH, sound: "minecraft:entity.silverfish.hurt" }];

/// The velocity of one spawn: the look direction times 0.3 (1.5 on y), turned about y by `angle`.
pub fn thrown(look: Vec3, angle: f32) -> Vec3 {
    let (sin, cos) = angle.sin_cos();
    let (x, y, z) = (look.x as f32 * 0.3, look.y as f32 * 0.3 * 1.5, look.z as f32 * 0.3);
    Vec3::new(f64::from(x * cos + z * sin), f64::from(y), f64::from(-x * sin + z * cos))
}

/// A hit landed on `e` (it survived it or not): each effect of the table it bears rolls on its own random.
pub fn on_hurt(w: &mut World, e: Entity) {
    if w.get::<effects::Effects>(e).is_none() {
        return;
    }
    for t in TABLE {
        if !effects::has(w, e, t.effect) {
            continue;
        }
        let Some(mut rng) = w.get::<Rng>(e).copied() else { return };
        let n = if rng.next_f64() <= f64::from(t.chance) { 1 + rng.next_int(t.extra) } else { 0 };
        let (Some(pos), Some(height)) = (w.get::<Pos>(e).map(|p| p.0), w.get::<Body>(e).map(|b| b.height)) else { return };
        let yaw = w.get::<Yaw>(e).map_or(0.0, |y| y.0);
        let pitch = w.get::<crate::nbt_fields::common::Pitch>(e).map_or(0.0, |p| p.0);
        let look = crate::rules::fight::view_vector(yaw, pitch);
        let half = std::f32::consts::FRAC_PI_2;
        let mut out = Vec::new();
        for _ in 0..n {
            let angle = (rng.next_f64() as f32) * (2.0 * half) - half;
            let facing = (rng.next_f64() as f32) * 360.0;
            out.push((thrown(look, angle), facing));
        }
        *w.get_mut::<Rng>(e).unwrap() = rng;
        let at = [pos.x, pos.y + f64::from(height) / 2.0, pos.z];
        for (v, facing) in out {
            spawn_one(w, t, at, v, facing);
        }
    }
}

fn spawn_one(w: &mut World, t: &HurtSpawn, at: [f64; 3], v: Vec3, facing: f32) {
    let table = species_table();
    let Some(sp) = table.0.iter().find(|s| s.name == t.species) else { return };
    let e = crate::cow::spawn_armed(w, sp, at[0], at[2], u64::from(facing.to_bits()) | 1);
    w.get_mut::<Pos>(e).unwrap().0 = Vec3::new(at[0], at[1], at[2]);
    if let Some(mut b) = w.get_mut::<crate::cow::Bounds>(e) {
        b.0 = crate::cow::box_at(Vec3::new(at[0], at[1], at[2]), sp.width, sp.height);
    }
    if let Some(mut y) = w.get_mut::<Yaw>(e) {
        y.0 = facing;
    }
    if let Some(mut vel) = w.get_mut::<Vel>(e) {
        vel.0 = v;
    }
    crate::zone::admit(w, &[e]);
    crate::bridge::view::mark(w, e);
    crate::bridge::host_events_more::said_spawned(w, e, false, crate::spawn_reason::SpawnReason::Triggered);
    w.resource_mut::<crate::goals::Events>().host.sound(t.sound, at, 1.0, 1.0);
}
