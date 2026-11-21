use horizon_core::geometry::Vec3;

/// The air drag of a primed TNT (0.98 on every axis), then the ground's friction when it stands on a block (x and z times 0.7, y times -0.5).
pub fn drag(v: Vec3, on_ground: bool) -> Vec3 {
    let drag = f64::from(0.98_f32);
    let v = Vec3::new(v.x * drag, v.y * drag, v.z * drag);
    if on_ground {
        Vec3::new(v.x * 0.7, v.y * -0.5, v.z * 0.7)
    } else {
        v
    }
}

/// The point a TNT blows from: at `0.0625` of its height above its position (`PrimedTnt.explode`, `getY(0.0625)`).
pub fn blast_point(at: Vec3, size: f32) -> Vec3 {
    Vec3::new(at.x, at.y + f64::from(size) * 0.0625, at.z)
}
