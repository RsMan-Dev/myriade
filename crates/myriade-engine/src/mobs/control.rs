//! Mob controls: `MoveControl` (with the flying and smooth-swimming
//! variants), `LookControl`, `JumpControl` and `BodyRotationControl`.
use super::{MobWorld, Species, living::wrap};
use horizon_core::geometry::trig;

/// `2.5000003E-7F` widened: the squared distance controls treat as arrived.
const ARRIVED: f64 = 2.500_000_3e-7_f32 as f64;
/// `1.0E-5F` widened.
pub(super) const EPSILON: f64 = 1.0e-5_f32 as f64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Operation {
    #[default]
    Wait,
    MoveTo,
    Strafe,
    Jumping,
}
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum MoveKind {
    #[default]
    Ground,
    /// FlyingMoveControl(maxTurn, hoversInPlace)
    Flying { max_turn: f32, hovers: bool },
    /// SmoothSwimmingMoveControl(maxTurnX, maxTurnY, inWater, outsideWater, applyGravity)
    SmoothSwimming {
        max_turn_x: f32,
        max_turn_y: f32,
        in_water: f32,
        outside: f32,
        gravity: bool,
    },
    /// Goals drive the mob directly (slimes, ghasts, squid, fish, bats, phantoms).
    Custom,
}
#[derive(Clone, Debug, Default)]
pub struct MoveControl {
    pub kind: MoveKind,
    pub wanted: [f64; 3],
    pub speed_modifier: f64,
    pub strafe_forwards: f32,
    pub strafe_right: f32,
    pub operation: Operation,
}
impl MoveControl {
    pub fn has_wanted(&self) -> bool {
        self.operation == Operation::MoveTo
    }
    pub fn set_wanted(&mut self, x: f64, y: f64, z: f64, speed: f64) {
        self.wanted = [x, y, z];
        self.speed_modifier = speed;
        if self.operation != Operation::Jumping {
            self.operation = Operation::MoveTo;
        }
    }
    pub fn strafe(&mut self, forwards: f32, right: f32) {
        self.operation = Operation::Strafe;
        self.strafe_forwards = forwards;
        self.strafe_right = right;
        self.speed_modifier = 0.25;
    }
}
/// MoveControl.rotlerp: turn at most `max` degrees, result in [0, 360].
pub fn rotlerp(a: f32, b: f32, max: f32) -> f32 {
    let diff = wrap(b - a).clamp(-max, max);
    let mut r = a + diff;
    if r < 0.0 {
        r += 360.0;
    } else if r > 360.0 {
        r -= 360.0;
    }
    r
}
/// Control.rotateTowards.
pub fn rotate_towards(from: f32, to: f32, max: f32) -> f32 {
    from + wrap(to - from).clamp(-max, max)
}
/// Mth.rotateIfNecessary.
pub fn rotate_if_necessary(base: f32, target: f32, max: f32) -> f32 {
    let d = wrap(target - base).clamp(-max, max);
    target - d
}
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum LookKind {
    #[default]
    Default,
    /// SmoothSwimmingLookControl(maxYRotFromCenter)
    SmoothSwimming(f32),
    /// Look controls that never reset pitch (e.g. ghasts keep their own).
    Fixed,
    /// `HappyGhast.HappyGhastLookControl`.
    HappyGhast,
    /// `Shulker.ShulkerLookControl`: the yaw turns in the attach face's
    /// plane and the head is never clamped to the body.
    Shulker,
}
#[derive(Clone, Debug, Default)]
pub struct LookControl {
    pub kind: LookKind,
    pub wanted: [f64; 3],
    pub y_max_rot_speed: f32,
    pub x_max_rot_angle: f32,
    pub cooldown: i32,
}
impl LookControl {
    pub fn set_look_at(&mut self, at: [f64; 3], y_speed: f32, x_max: f32) {
        self.wanted = at;
        self.y_max_rot_speed = y_speed;
        self.x_max_rot_angle = x_max;
        self.cooldown = 2;
    }
}
#[derive(Clone, Debug, Default)]
pub struct BodyRotation {
    pub(crate) head_stable_time: i32,
    pub(crate) last_stable_head: f32,
}
/// Entity.rotlerp used by Mob.lookAt (clamped turn, no wrap of the result).
fn rotate_toward_max(from: f32, to: f32, max: f32) -> f32 {
    from + wrap(to - from).clamp(-max, max)
}


#[cfg(test)]
mod goat_head_tests {
    use super::*;
    use horizon_core::geometry::Vec3;
    use std::sync::Arc;


    /// `Goat.setYHeadRot`: whatever the look control asks for, a goat's head
    /// stays within `getMaxHeadYRot` (15 degrees) of its body, standing still
    /// or walking; another mob turns as far as it likes.
    #[test]
    fn a_goats_head_stays_within_fifteen_degrees_of_its_body() {
        let turned = |species: Species| {
            let mut m = mob(species);
            m.body_yaw = 0.0;
            // A target 40 blocks to the side: a quarter turn away.
            m.ai.look.kind = LookKind::Default;
            m.ai.look.wanted = [40.0, 1.0, 0.0];
            m.ai.look.y_max_rot_speed = 1000.0;
            m.ai.look.x_max_rot_angle = 40.0;
            m.ai.look.cooldown = 20;
            m.look_control_tick();
            wrap(m.head_yaw - m.body_yaw)
        };
        let goat = turned(Species::Goat);
        assert!(
            goat.abs() <= 15.0 + 1.0e-3,
            "the goat's head turned {goat} degrees from its body"
        );
        assert!(
            turned(Species::Cow).abs() > 15.0,
            "a cow is not held to the goat's 15 degrees"
        );
    }
}
