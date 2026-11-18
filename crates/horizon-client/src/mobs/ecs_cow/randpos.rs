//! `DefaultRandomPos` and `LandRandomPos` of a cow (`mobs/goals.rs`): the spots a stroll or a panic picks.
use super::view::CowItem;
use crate::mobs::{MobWorld, goals::within_home};
use horizon_core::{jmath, random::RandomSource};

impl CowItem<'_, '_> {
    fn random_direction(&mut self, h: i32, v: i32) -> [i32; 3] {
        let x = self.random.next_int_bound(2 * h + 1).unwrap_or(0) - h;
        let y = self.random.next_int_bound(2 * v + 1).unwrap_or(0) - v;
        let z = self.random.next_int_bound(2 * h + 1).unwrap_or(0) - h;
        [x, y, z]
    }

    #[allow(clippy::too_many_arguments)]
    fn random_direction_within(
        &mut self,
        min: f64,
        max: f64,
        v: i32,
        fly_height: i32,
        xd: f64,
        zd: f64,
        max_rad: f64,
    ) -> Option<[i32; 3]> {
        let center =
            horizon_core::geometry::trig::atan2(zd, xd) - f64::from(std::f32::consts::FRAC_PI_2);
        let yrad = center + f64::from(2.0 * self.random.next_float() - 1.0) * max_rad;
        let dist = (min + self.random.next_double().sqrt() * (max - min))
            * f64::from(std::f32::consts::SQRT_2);
        let xt = -dist * jmath::sin(yrad);
        let zt = dist * jmath::cos(yrad);
        if xt.abs() > max || zt.abs() > max {
            return None;
        }
        let yt = self.random.next_int_bound(2 * v + 1).unwrap_or(0) - v + fly_height;
        Some([xt.floor() as i32, yt, zt.floor() as i32])
    }

    /// RandomPos.generateRandomPosTowardDirection: a mob with a home leans
    /// half the distance back toward it.
    fn toward(&mut self, h: f64, d: [i32; 3]) -> [i32; 3] {
        let (mut xt, mut zt) = (f64::from(d[0]), f64::from(d[2]));
        if let Some((c, _)) = self.ai.home
            && h > 1.0
        {
            if self.motion.position.x > f64::from(c[0]) {
                xt -= self.random.next_double() * h / 2.0;
            } else {
                xt += self.random.next_double() * h / 2.0;
            }
            if self.motion.position.z > f64::from(c[2]) {
                zt -= self.random.next_double() * h / 2.0;
            } else {
                zt += self.random.next_double() * h / 2.0;
            }
        }
        [
            (xt + self.motion.position.x).floor() as i32,
            (f64::from(d[1]) + self.motion.position.y).floor() as i32,
            (zt + self.motion.position.z).floor() as i32,
        ]
    }

    /// `GoalUtils.isRestricted(mobRestricted(mob, h), mob, pos)`: a mob
    /// near its home keeps the positions it picks within it.
    pub fn restricted(&self, h: f64, p: [i32; 3]) -> bool {
        let Some((c, r)) = self.ai.home else {
            return false;
        };
        let near = {
            let d = |i: usize, v: f64| f64::from(c[i]) + 0.5 - v;
            let (x, y, z) = (
                d(0, self.motion.position.x),
                d(1, self.motion.position.y),
                d(2, self.motion.position.z),
            );
            let reach = f64::from(r) + h + 1.0;
            x * x + y * y + z * z < reach * reach
        };
        near && !within_home(c, r, p)
    }

    fn default_check(&self, w: &MobWorld, h: f64, p: [i32; 3]) -> bool {
        p[1] >= w.world.bottom_y()
            && p[1] < w.world.top_y()
            && !self.restricted(h, p)
            && self.stable_destination(w, p)
            && self.malus(w.path_type_static(p[0], p[1], p[2])) == 0.0
    }

    fn pick_best(
        &mut self,
        w: &MobWorld,
        mut supplier: impl FnMut(&mut Self) -> Option<[i32; 3]>,
    ) -> Option<[f64; 3]> {
        let mut best: Option<[i32; 3]> = None;
        let mut best_weight = f64::NEG_INFINITY;
        for _ in 0..10 {
            if let Some(p) = supplier(self) {
                let v = f64::from(self.walk_target_value(w, p));
                if v > best_weight {
                    best_weight = v;
                    best = Some(p);
                }
            }
        }
        best.map(|p| {
            [
                f64::from(p[0]) + 0.5,
                f64::from(p[1]),
                f64::from(p[2]) + 0.5,
            ]
        })
    }

    /// DefaultRandomPos.getPos
    pub fn default_random_pos(&mut self, w: &MobWorld, h: i32, v: i32) -> Option<[f64; 3]> {
        self.pick_best(w, |m| {
            let d = m.random_direction(h, v);
            let p = m.toward(f64::from(h), d);
            m.default_check(w, f64::from(h), p).then_some(p)
        })
    }

    fn land_up_out_of_solid(&self, w: &MobWorld, mut p: [i32; 3]) -> Option<[i32; 3]> {
        if w.solid(p[0], p[1], p[2]) {
            p[1] += 1;
            while p[1] < w.world.top_y() && w.solid(p[0], p[1], p[2]) {
                p[1] += 1;
            }
        }
        let water = w.fluid(p[0], p[1], p[2]) == crate::blocks::Fluid::Water;
        (!water && self.malus(w.path_type_static(p[0], p[1], p[2])) == 0.0).then_some(p)
    }

    /// LandRandomPos.getPos
    pub fn land_random_pos(&mut self, w: &MobWorld, h: i32, v: i32) -> Option<[f64; 3]> {
        self.pick_best(w, |m| {
            let d = m.random_direction(h, v);
            let p = m.toward(f64::from(h), d);
            if p[1] < w.world.bottom_y()
                || p[1] >= w.world.top_y()
                || m.restricted(f64::from(h), p)
                || !m.stable_destination(w, p)
            {
                return None;
            }
            m.land_up_out_of_solid(w, p)
        })
    }


    /// `PathfinderMob.getWalkTargetValue` of an animal (`ai::walk_target_value`): grass is the best ground.
    pub fn walk_target_value(&self, w: &MobWorld, p: [i32; 3]) -> f32 {
        if w.block_name(p[0], p[1] - 1, p[2]) == "minecraft:grass_block" {
            return 10.0;
        }
        w.brightness(p[0], p[1], p[2]) - 0.5
    }
}
