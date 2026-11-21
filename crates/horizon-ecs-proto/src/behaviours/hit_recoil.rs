//! The recoil of a sulfur cube holding a block when it is hit (`SulfurCube.knockback` and its `launch`, `sulfur_cube.rs` 1100-1210):
//! the archetype's horizontal and vertical powers, scaled by `sqrt(damage)` and by the effect scale, cut by the knockback resistance,
//! multiplied by 0.4, clamped to 128, then added to the velocity (vertical times 1.2). Called by the hurt path with the hit's direction.
//! DOUTE : la rotation par la hauteur de l'attaquant (`feet`) et le visage (`aim`, `horizontal_hit_angle_scale`) ne sont pas portés :
//! le `Hit` du cadre n'a ni pieds ni regard de l'attaquant. Sans eux, la rotation vaut zéro (cas `feet == position`).
//! DOUTE : `resistance` est pris comme `push_factor` (1 - kr, borné à 0) ; la fonction vanille n'a pas été relue ici.
//! DOUTE : le son de coup (`hit_sound`) est émis par le socle, pas ici ; `needsSync` est sans objet dans le cadre.
use crate::cow::*;
use bevy_ecs::prelude::*;
use crate::rules::hit_recoil::launch;
use horizon_client_lib::mobs::sulfur_cube::Archetype;
use horizon_core::geometry::Vec3;

/// The blow reached a holding cube: its velocity takes the recoil. `direction` is from the attacker to the cube (x, z).
pub fn recoil(w: &mut World, e: Entity, archetype: &Archetype, damage: f32, direction: (f64, f64)) {
    let delta = launch(archetype, damage, direction, 1.0);
    if let Some(mut v) = w.get_mut::<Vel>(e) {
        v.0 = v.0.add_vector(Vec3::new(delta[0], delta[1], delta[2]));
    }
}
