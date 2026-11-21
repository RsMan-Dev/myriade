//! Calculs de la nage : vitesse visée et son lissage, arrivée et blocage, sortie de l'eau, mouvement de la méduse, du gardien et de la tortue.
use horizon_client_lib::mobs::path::Path;
use horizon_core::geometry::{trig, Vec3};

pub const PI: f32 = std::f32::consts::PI;
/// `SmoothSwimmingMoveControl`: the squared distance under which the wanted point is reached.
pub const ARRIVED: f64 = 2.500_000_3e-7_f32 as f64;
pub const EPSILON: f64 = 1.0e-5_f32 as f64;

/// The speed an animal goes at: its speed modifier times its body speed.
pub fn wanted_speed(speed_modifier: f64, body_speed: f64) -> f32 {
    (speed_modifier * body_speed) as f32
}
/// `FishMoveControl`: the speed eases an eighth of the gap toward the wanted one.
pub fn ease_speed(current: f32, wanted: f32) -> f32 {
    current + (wanted - current) * 0.125
}
/// `SmoothSwimmingMoveControl.tick`: the wanted point is reached (squared distance `(xd, yd, zd)`).
pub fn arrived(xd: f64, yd: f64, zd: f64) -> bool {
    xd * xd + yd * yd + zd * zd < ARRIVED
}
/// `SmoothSwimmingMoveControl`, out of the water: the speed factor of the turn, `left` degrees off the course.
pub fn turn_factor(left: f32) -> f32 {
    1.0 - ((left - 10.0) / 50.0).clamp(0.0, 1.0)
}
/// `Turtle.TurtleMoveControl`: on the ground the speed is halved each tick, never under 0.06.
pub fn turtle_ground_speed(speed: f32) -> f32 {
    (speed / 2.0).max(0.06)
}
/// `GuardianMoveControl.tick`: the wobble of the velocity at time `t`, facing `yaw` degrees, with `yd` the vertical part of the way to the point.
pub fn guardian_velocity(v: Vec3, t: f64, yaw: f32, yd: f64, speed: f32) -> Vec3 {
    let push = (t * 0.5).sin() * 0.05;
    let rad = f64::from(yaw * trig::DEG_TO_RAD);
    let (cos, sin) = (f64::from(trig::cos(rad)), f64::from(trig::sin(rad)));
    let y_push = (t * 0.75).sin() * 0.05;
    Vec3::new(v.x + push * cos, v.y + y_push * (sin + cos) * 0.25 + f64::from(speed) * yd * 0.1, v.z + push * sin)
}
/// `WaterAnimal.handleAirSupply` out of the water and `AbstractFish.aiStep`'s flop: the velocity of a jump from the two draws `a`, `b` in [0, 1).
pub fn flop_velocity(v: Vec3, a: f32, b: f32) -> Vec3 {
    Vec3::new(v.x + f64::from((a * 2.0 - 1.0) * 0.05), v.y + 0.4, v.z + f64::from((b * 2.0 - 1.0) * 0.05))
}
/// `Squid.register` and `Squid.aiStep`: the tentacle cycle's speed, from the draw `draw` in [0, 1).
pub fn tentacle_speed(draw: f32) -> f32 {
    1.0 / (draw + 1.0) * 0.2
}
/// `Squid.aiStep`: the cycle has made a turn (its movement is over 2 PI).
pub fn cycle_turned(movement: f32) -> bool {
    f64::from(movement) > std::f64::consts::TAU
}
/// `Squid.aiStep`: the movement after a turn.
pub fn cycle_back(movement: f32) -> f32 {
    movement - PI * 2.0
}
/// `Squid.aiStep` in the water: the stroke from 3/4 of the first half sets the velocity to the goals' vector, the second half slows it by 0.9.
pub fn squid_swim(movement: f32, vector: [f64; 3], v: Vec3) -> Vec3 {
    if movement < PI {
        if movement / PI > 0.75 {
            return Vec3::new(vector[0], vector[1], vector[2]);
        }
        v
    } else {
        Vec3::new(v.x * 0.9, v.y * 0.9, v.z * 0.9)
    }
}
/// `Squid.aiStep` out of the water: it falls straight down.
pub fn squid_dry(v: Vec3, gravity: f64) -> Vec3 {
    Vec3::new(0.0, (v.y - gravity) * f64::from(0.98_f32), 0.0)
}
/// `Vec3.add(0.0, -0.005, 0.0)` after the move in the water: the sink of an animal with no target (a guardian only when idle).
pub fn sink_rate(sinks: bool, guardian: bool, idle: bool) -> f64 {
    if sinks && (!guardian || idle) {
        0.005
    } else {
        0.0
    }
}
/// The velocity after the move in the water: drag 0.9, minus the sink.
pub fn swim_drag(m: Vec3, sink: f64) -> Vec3 {
    Vec3::new(m.x * 0.9, m.y * 0.9 - sink, m.z * 0.9)
}
/// `WaterBoundPathNavigation`: half the body's width, the reach of a node horizontally.
pub fn node_reach_xz(width: f32) -> f64 {
    f64::from(if width > 0.75 { width / 2.0 } else { 0.75 - width / 2.0 })
}
/// The reach of a node vertically: 1 for an amphibious animal, 0.5 in the water.
pub fn node_reach_y(amphibious: bool) -> f64 {
    if amphibious {
        1.0
    } else {
        0.5
    }
}
/// `followThePath` of the water: the node is reached (offsets `xd`, `yd`, `zd`, reaches `max` and `vertical`).
pub fn node_close(xd: f64, yd: f64, zd: f64, max: f64, vertical: f64) -> bool {
    xd < max && zd < max && yd < vertical
}
/// `doStuckDetection`: the animal is stuck when it moved (squared distance `moved`) less than its speed allows over the check.
pub fn is_stuck(speed: f32, moved: f64) -> bool {
    let effective = if speed >= 1.0 { speed } else { speed * speed };
    let threshold = f64::from(effective * 100.0 * 0.25);
    moved < threshold * threshold
}
/// The squared distance between two points.
pub fn moved_sq(a: Vec3, b: Vec3) -> f64 {
    (a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2)
}
/// `PathNavigation.shouldTargetNextNode` with the water's `canMoveDirectly`.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
pub fn target_next_node(path: &Path, mob: Vec3, direct: &dyn Fn(Vec3, Vec3) -> bool, width: f32) -> bool {
    if path.next + 1 >= path.nodes.len() {
        return false;
    }
    let n = path.nodes[path.next];
    let current = Vec3::new(f64::from(n.x) + 0.5, f64::from(n.y), f64::from(n.z) + 0.5);
    let d = |a: Vec3, b: Vec3| (a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2);
    if d(mob, current) >= 4.0 {
        return false;
    }
    if direct(mob, path.entity_pos_at(path.next, width)) {
        return true;
    }
    let m = path.nodes[path.next + 1];
    let next = Vec3::new(f64::from(m.x) + 0.5, f64::from(m.y), f64::from(m.z) + 0.5);
    let (to_current, to_next) = (Vec3::new(current.x - mob.x, current.y - mob.y, current.z - mob.z), Vec3::new(next.x - mob.x, next.y - mob.y, next.z - mob.z));
    let (cs, ns) = (to_current.length_squared(), to_next.length_squared());
    if !(ns < cs) && !(cs < 0.5) {
        return false;
    }
    let (a, b) = (to_current.normalize(), to_next.normalize());
    a.x * b.x + a.y * b.y + a.z * b.z < 0.0
}
