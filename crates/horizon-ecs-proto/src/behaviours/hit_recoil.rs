//! The recoil of a sulfur cube holding a block when it is hit (`SulfurCube.knockback` and its `launch`, `sulfur_cube.rs` 1100-1210):
//! the archetype's horizontal and vertical powers, scaled by `sqrt(damage)` and by the effect scale, cut by the knockback resistance,
//! multiplied by 0.4, clamped to 128, then added to the velocity (vertical times 1.2). Called by the hurt path with the hit's direction.
//! DOUTE : la rotation par la hauteur de l'attaquant (`feet`) et le visage (`aim`, `horizontal_hit_angle_scale`) ne sont pas portés :
//! le `Hit` du cadre n'a ni pieds ni regard de l'attaquant. Sans eux, la rotation vaut zéro (cas `feet == position`).
//! DOUTE : `resistance` est pris comme `push_factor` (1 - kr, borné à 0) ; la fonction vanille n'a pas été relue ici.
//! DOUTE : le son de coup (`hit_sound`) est émis par le socle, pas ici ; `needsSync` est sans objet dans le cadre.
use crate::cow::*;
use bevy_ecs::prelude::*;
use horizon_client_lib::mobs::sulfur_cube::Archetype;
use horizon_core::geometry::Vec3;

/// `launch` without the aim and without the feet: the knock of one blow, as a velocity delta.
pub fn launch(a: &Archetype, damage: f32, (xd, zd): (f64, f64), effect_scale: f32) -> [f64; 3] {
    let (h0, v0) = (a.horizontal_power, a.vertical_power);
    let (mut h, mut v) = (h0, v0);
    let hr = if h0 > 0.0 { h.abs() / h0 } else { 0.0 };
    let vr = if v0 > 0.0 { v.abs() / v0 } else { 0.0 };
    let max = hr.max(vr);
    if max > 1.0 {
        h *= 1.0 / max;
        v *= 1.0 / max;
    }
    let k = damage.sqrt() * effect_scale;
    h *= k;
    v *= k;
    let resistance = (1.0 - a.knockback_resistance.clamp(-2.0, 1.0)).max(0.0) as f32;
    h *= 1.0 - resistance;
    v *= 1.0 - resistance;
    h *= 0.4;
    let (h, v) = (h.clamp(-128.0, 128.0), v.clamp(-128.0, 128.0));
    let len = (xd * xd + zd * zd).sqrt();
    let (kx, kz) = if len < 1.0e-5 { (0.0, 0.0) } else { (xd / len * f64::from(h), zd / len * f64::from(h)) };
    [-kx, f64::from(v) * 1.2, -kz]
}

/// The blow reached a holding cube: its velocity takes the recoil. `direction` is from the attacker to the cube (x, z).
pub fn recoil(w: &mut World, e: Entity, archetype: &Archetype, damage: f32, direction: (f64, f64)) {
    let delta = launch(archetype, damage, direction, 1.0);
    if let Some(mut v) = w.get_mut::<Vel>(e) {
        v.0 += Vec3::new(delta[0], delta[1], delta[2]);
    }
}
