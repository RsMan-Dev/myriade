//! The seat of a passenger: an attachment point of its vehicle, turned with the vehicle's yaw, less the rider's own offset.
use horizon_core::geometry::Vec3;

/// `Vec3.scale`: an attachment point in the scale of its entity (`getDimensions` scales the attachments with the box).
pub fn scaled([x, y, z]: [f64; 3], s: f64) -> [f64; 3] {
    [x * s, y * s, z * s]
}
/// `Vec3.yRot(-yaw)` of an attachment point.
pub fn rotated([x, y, z]: [f64; 3], yaw: f32) -> [f64; 3] {
    let a = -f64::from(yaw).to_radians();
    let (sin, cos) = (a.sin(), a.cos());
    [x * cos + z * sin, y, z * cos - x * sin]
}
/// `Entity.positionRider`: the vehicle's position plus its rotated passenger attachment, less the rider's own vehicle attachment.
pub fn seat(attachment: [f64; 3], rider_offset: f64, at: Vec3, yaw: f32) -> Vec3 {
    let [x, y, z] = rotated(attachment, yaw);
    Vec3::new(at.x + x, at.y + y - rider_offset, at.z + z)
}
/// The passenger attachment of seat `index`: the last one serves the riders past the list, a baby takes its own when there is one.
pub fn attachment(adult: &[[f64; 3]], baby: Option<[f64; 3]>, is_baby: bool, index: usize) -> [f64; 3] {
    match baby {
        Some(b) if is_baby => b,
        _ => adult[index.min(adult.len() - 1)],
    }
}
/// `Camel.getBodyAnchorAnimationYOffset`: the height of the front (`front`) or back seat of a camel of `height` at `scale`, lowered through
/// the sit and the stand (`transition`: the ticks run in it; `sits`: the posture it goes to). Sitting, the body sinks by its bottom point.
pub fn camel_anchor_y(height: f32, baby: bool, scale: f32, front: bool, sits: bool, transition: Option<f32>) -> f64 {
    let age_offset = if baby { 0.09375 } else { 0.375 };
    let mut base = f64::from(height) - age_offset;
    let diff = scale * 1.43;
    let drop = diff - scale * 0.2;
    let bottom = diff - drop;
    if let Some(time) = transition {
        let (duration, half, flex_off) = match (sits, front) {
            (true, true) => (40.0_f32, 28.0_f32, 0.5_f32),
            (true, false) => (40.0, 28.0, 0.1),
            (false, true) => (52.0, 24.0, 0.6),
            (false, false) => (52.0, 32.0, 0.35),
        };
        let flex = diff - flex_off * drop;
        let first = time < half;
        let part = if first { time / half } else { (time - half) / (duration - half) };
        let lerp = |d: f32, a: f32, b: f32| a + d * (b - a);
        base += f64::from(if sits {
            lerp(part, if first { diff } else { flex }, if first { flex } else { bottom })
        } else {
            lerp(part, if first { bottom - diff } else { bottom - flex }, if first { bottom - flex } else { 0.0 })
        });
    } else if sits {
        base += f64::from(bottom);
    }
    base
}
/// `Camel.getPassengerAttachmentPoint` of the seat `index` of `count` aboard: the driver half a block ahead, the second seat 0,7 behind,
/// an animal 0,2 further ahead; all in the camel's scale (0,6 for a baby), with the sinking body anchor of `camel_anchor_y`.
pub fn camel_attachment(height: f32, baby: bool, index: usize, count: usize, animal: bool, sits: bool, transition: Option<f32>) -> [f64; 3] {
    let scale = if baby { 0.6_f32 } else { 1.0 };
    let driver = index == 0;
    let y = camel_anchor_y(height, baby, scale, driver, sits, transition);
    let mut offset = 0.5_f32;
    if count > 1 {
        if !driver {
            offset = -0.7;
        }
        if animal {
            offset += 0.2;
        }
    }
    [0.0, y, f64::from(offset * scale)]
}
/// `Horse.getPassengerAttachmentPoint` of a rearing horse (`equine.rs` `rear_offset`): the stand value `stand` lifts the seat and draws it
/// back, in the scale of the foal (0,5) or of the adult.
pub fn rear_slide(stand: f32, baby: bool) -> [f64; 3] {
    let scale = if baby { 0.5_f32 } else { 1.0 };
    [0.0, f64::from(0.15_f32 * stand * scale), f64::from(-0.7_f32 * stand * scale)]
}
/// The rider's own vehicle attachment: its narrow one on a vehicle no wider than the rider, else its baby one or its adult one, in the
/// rider's scale.
pub fn rider_offset(seat: crate::mount::RiderSeat, vehicle_width: f64, rider_width: f64, is_baby: bool, scale: f64) -> f64 {
    let offset = match (seat.narrow, seat.baby) {
        (Some(n), _) if vehicle_width <= rider_width => n,
        (_, Some(b)) if is_baby => b,
        _ => seat.adult,
    };
    offset * scale
}
