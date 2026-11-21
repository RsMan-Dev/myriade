//! Knock of a sulfur cube holding a block when it is hit (`SulfurCube.knockback` and its `launch`, `sulfur_cube.rs` 1100-1210).
use horizon_client_lib::mobs::sulfur_cube::Archetype;

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
