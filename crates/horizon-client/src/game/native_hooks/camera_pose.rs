use super::*;

/// The hook points. Each does nothing in Horizon.
impl Game<'_> {
}

/// The pose a native mod asks for the first-person hand, each out-of-range
/// or non-finite component replaced by its identity.
pub(in crate::game) fn hand_pose_matrix(values: [f32; 7]) -> glam::Mat4 {
    let valid = |value: f32, min: f32, max: f32, default: f32| {
        if value.is_finite() && (min..=max).contains(&value) {
            value
        } else {
            default
        }
    };
    let [x, y, z, pitch, yaw, roll, scale] = values;
    glam::Mat4::from_translation(glam::Vec3::new(
        valid(x, -16.0, 16.0, 0.0),
        valid(y, -16.0, 16.0, 0.0),
        valid(z, -16.0, 16.0, 0.0),
    )) * glam::Mat4::from_rotation_x(valid(pitch, -3600.0, 3600.0, 0.0).to_radians())
        * glam::Mat4::from_rotation_y(valid(yaw, -3600.0, 3600.0, 0.0).to_radians())
        * glam::Mat4::from_rotation_z(valid(roll, -3600.0, 3600.0, 0.0).to_radians())
        * glam::Mat4::from_scale(glam::Vec3::splat(valid(scale, 0.01, 100.0, 1.0)))
}

/// `shake` scaled by `scale` (Distortion Effects) and bounded on each axis;
/// a value that isn't finite counts as none.
pub(super) fn bounded_shake(shake: Shake, scale: f32) -> Shake {
    let scale = if scale.is_finite() {
        scale.clamp(0.0, 1.0)
    } else {
        1.0
    };
    let turn = |v: f32| {
        if v.is_finite() {
            (v * scale).clamp(-SHAKE_MAX_DEGREES, SHAKE_MAX_DEGREES)
        } else {
            0.0
        }
    };
    let shift = |v: f32| {
        if v.is_finite() {
            (v * scale).clamp(-SHAKE_MAX_OFFSET, SHAKE_MAX_OFFSET)
        } else {
            0.0
        }
    };
    Shake {
        pitch: turn(shake.pitch),
        yaw: turn(shake.yaw),
        roll: turn(shake.roll),
        offset: shake.offset.map(shift),
    }
}
