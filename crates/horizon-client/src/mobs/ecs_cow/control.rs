//! `MoveControl`, `LookControl`, `JumpControl` and `BodyRotationControl` of a ground cow
//! (`mobs/control.rs`).
use super::view::CowItem;
use crate::mobs::{
    MobWorld,
    control::{Operation, rotate_if_necessary, rotate_towards, rotlerp},
    living::wrap,
};
use horizon_core::geometry::trig;

const ARRIVED: f64 = 2.500_000_3e-7_f32 as f64;
const EPSILON: f64 = 1.0e-5_f32 as f64;

fn rotate_toward_max(from: f32, to: f32, max: f32) -> f32 {
    from + wrap(to - from).clamp(-max, max)
}

impl CowItem<'_, '_> {
    pub fn max_head_x_rot(&self) -> f32 {
        self.id.kind.max_head_x_rot
    }
    pub fn look_at(&mut self, at: [f64; 3]) {
        let (y, x) = (self.id.kind.head_rot_speed, self.max_head_x_rot());
        self.ai.look.set_look_at(at, y, x);
    }
    pub fn look_at_speed(&mut self, at: [f64; 3], y_speed: f32, x_max: f32) {
        self.ai.look.set_look_at(at, y_speed, x_max);
    }
    /// `Mob.lookAt(entity, yMax, xMax)`.
    pub fn turn_to(&mut self, at: [f64; 3], y_max: f32, x_max: f32) {
        let (dx, dz) = (at[0] - self.motion.position.x, at[2] - self.motion.position.z);
        let dy = at[1] - self.eye_y();
        let sd = (dx * dx + dz * dz).sqrt();
        let y_rot = trig::atan2_degrees(dz, dx) - 90.0;
        let x_rot = -trig::atan2_degrees(dy, sd);
        self.motion.head_pitch = rotate_toward_max(self.motion.head_pitch, x_rot, x_max);
        self.motion.yaw = rotate_toward_max(self.motion.yaw, y_rot, y_max);
    }
    pub fn movement_speed(&self) -> f64 {
        self.attrs.speed
    }
    pub fn set_speed(&mut self, speed: f32) {
        self.ai.speed = speed;
        self.ai.zza = speed;
    }
    pub fn move_control_tick(&mut self, w: &MobWorld) {
        let attr_speed = self.movement_speed();
        self.ground_move_tick(w, attr_speed);
    }
    fn ground_move_tick(&mut self, w: &MobWorld, attr_speed: f64) {
        match self.ai.mov.operation {
            Operation::Strafe => {
                let speed = self.ai.mov.speed_modifier as f32 * attr_speed as f32;
                let (mut xa, mut za) = (self.ai.mov.strafe_forwards, self.ai.mov.strafe_right);
                let mut dist = (xa * xa + za * za).sqrt();
                if dist < 1.0 {
                    dist = 1.0;
                }
                dist = speed / dist;
                xa *= dist;
                za *= dist;
                let angle = f64::from(self.motion.yaw * trig::DEG_TO_RAD);
                let (s, c) = (trig::sin(angle), trig::cos(angle));
                let dx = xa * c - za * s;
                let dz = za * c + xa * s;
                if !self.walkable(w, dx, dz) {
                    self.ai.mov.strafe_forwards = 1.0;
                    self.ai.mov.strafe_right = 0.0;
                }
                self.set_speed(speed);
                self.ai.zza = self.ai.mov.strafe_forwards;
                self.ai.xxa = self.ai.mov.strafe_right;
                self.ai.mov.operation = Operation::Wait;
            }
            Operation::MoveTo => {
                self.ai.mov.operation = Operation::Wait;
                let [wx, wy, wz] = self.ai.mov.wanted;
                let p = self.motion.position;
                let (xd, yd, zd) = (wx - p.x, wy - p.y, wz - p.z);
                if xd * xd + yd * yd + zd * zd < ARRIVED {
                    self.ai.zza = 0.0;
                    return;
                }
                let y_rot = trig::atan2_degrees(zd, xd) - 90.0;
                self.motion.yaw = rotlerp(self.motion.yaw, y_rot, 90.0);
                self.set_speed((self.ai.mov.speed_modifier * attr_speed) as f32);
                let (bx, by, bz) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
                let state = w.world.block(bx, by, bz);
                let top = w.data.state(state).top;
                let name = &w.facts.get(state).name;
                let door_or_fence = name.ends_with("_door") || name.ends_with("fence");
                if yd > self.id.kind.step_height && xd * xd + zd * zd < f64::from(self.width().max(1.0))
                    || top > 0.0 && p.y < f64::from(top) + f64::from(by) && !door_or_fence
                {
                    self.ai.jump = true;
                    self.ai.mov.operation = Operation::Jumping;
                }
            }
            Operation::Jumping => {
                self.set_speed((self.ai.mov.speed_modifier * attr_speed) as f32);
                if self.motion.on_ground || self.is_in_liquid() {
                    self.ai.mov.operation = Operation::Wait;
                }
            }
            Operation::Wait => self.ai.zza = 0.0,
        }
    }
    fn walkable(&self, w: &MobWorld, dx: f32, dz: f32) -> bool {
        let x = (self.motion.position.x + f64::from(dx)).floor() as i32;
        let z = (self.motion.position.z + f64::from(dz)).floor() as i32;
        let y = self.motion.position.y.floor() as i32;
        w.path_type_static(x, y, z) == crate::mobs::path::PathType::Walkable
    }
    /// `LookControl.tick` (the default kind).
    pub fn look_control_tick(&mut self) {
        let look = self.ai.look.clone();
        let (x_d, y_d) = self.look_angles(look.wanted);
        self.motion.head_pitch = 0.0;
        if look.cooldown > 0 {
            self.ai.look.cooldown -= 1;
            if let Some(y) = y_d {
                self.motion.head_yaw = rotate_towards(self.motion.head_yaw, y, look.y_max_rot_speed);
            }
            if let Some(x) = x_d {
                self.motion.head_pitch = rotate_towards(self.motion.head_pitch, x, look.x_max_rot_angle);
            }
        } else {
            self.motion.head_yaw = rotate_towards(self.motion.head_yaw, self.motion.body_yaw, 10.0);
        }
        if !self.ai.nav.is_done() {
            self.motion.head_yaw =
                rotate_if_necessary(self.motion.head_yaw, self.motion.body_yaw, self.id.kind.max_head_y_rot);
        }
    }
    fn look_angles(&self, at: [f64; 3]) -> (Option<f32>, Option<f32>) {
        let p = self.motion.position;
        let (xd, yd, zd) = (at[0] - p.x, at[1] - self.eye_y(), at[2] - p.z);
        let sd = (xd * xd + zd * zd).sqrt();
        let x = (yd.abs() > EPSILON || sd.abs() > EPSILON).then(|| -trig::atan2_degrees(yd, sd));
        let y = (zd.abs() > EPSILON || xd.abs() > EPSILON).then(|| trig::atan2_degrees(zd, xd) - 90.0);
        (x, y)
    }
    pub fn jump_control_tick(&mut self) {
        self.ai.jumping = self.ai.jump;
        self.ai.jump = false;
    }
    /// `BodyRotationControl.clientTick` (`Mob.tickHeadTurn`).
    pub fn body_rotation_tick(&mut self) {
        let dx = self.motion.position.x - self.motion.previous.x;
        let dz = self.motion.position.z - self.motion.previous.z;
        let max = self.id.kind.max_head_y_rot;
        if dx * dx + dz * dz > ARRIVED {
            self.motion.body_yaw = self.motion.yaw;
            self.motion.head_yaw = rotate_if_necessary(self.motion.head_yaw, self.motion.body_yaw, max);
            self.ai.body.last_stable_head = self.motion.head_yaw;
            self.ai.body.head_stable_time = 0;
        } else if (self.motion.head_yaw - self.ai.body.last_stable_head).abs() > 15.0 {
            self.ai.body.head_stable_time = 0;
            self.ai.body.last_stable_head = self.motion.head_yaw;
            self.motion.body_yaw = rotate_if_necessary(self.motion.body_yaw, self.motion.head_yaw, max);
        } else {
            self.ai.body.head_stable_time += 1;
            if self.ai.body.head_stable_time > 10 {
                let t = self.ai.body.head_stable_time - 10;
                let fraction = (t as f32 / 10.0).clamp(0.0, 1.0);
                self.motion.body_yaw =
                    rotate_if_necessary(self.motion.body_yaw, self.motion.head_yaw, max * (1.0 - fraction));
            }
        }
    }
}
