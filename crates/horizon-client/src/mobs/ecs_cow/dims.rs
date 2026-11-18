//! A cow's size, box, pose and eye height (`Mob::dims` and friends, cow only: no pose but standing,
//! no scale attribute, the baby box of the species).
use super::view::CowItem;
use crate::mobs::{Species, species};
use horizon_core::geometry::{Aabb, Vec3};

impl CowItem<'_, '_> {
    pub fn species(&self) -> Species {
        self.id.kind.species
    }
    pub fn alive(&self) -> bool {
        self.vitals.health > 0.0
    }
    pub fn is_baby(&self) -> bool {
        self.extra.age < 0 && species::can_be_a_baby(self.id.kind.species)
    }
    /// `Mob::dims_in_pose` of a standing cow with no `minecraft:scale`.
    pub fn dims(&self) -> (f64, f64, f64) {
        let k = &self.id.kind;
        let (mut w, mut h, mut e) = (k.width as f32, k.height as f32, k.eye_height as f32);
        if self.is_baby() {
            match species::def(k.species).baby {
                species::Baby::Abs(bw, bh, be) => (w, h, e) = (bw as f32, bh as f32, be as f32),
                species::Baby::Scale(s, eye) => {
                    let s = s as f32;
                    (w, h, e) = (w * s, h * s, eye.map_or(e * s, |x| x as f32))
                }
                species::Baby::None => {}
            }
        }
        let scale = 1.0_f32;
        (f64::from(w * scale), f64::from(h * scale), f64::from(e * scale))
    }
    pub fn eye_height(&self) -> f64 {
        self.dims().2
    }
    pub fn eye_y(&self) -> f64 {
        self.motion.position.y + self.eye_height()
    }
    pub fn width(&self) -> f32 {
        self.dims().0 as f32
    }
    pub fn height(&self) -> f32 {
        self.dims().1 as f32
    }
    pub fn bounds(&self) -> Aabb {
        self.bounds_at(self.motion.position)
    }
    pub fn bounds_at(&self, position: Vec3) -> Aabb {
        let (width, height, _) = self.dims();
        let w = f64::from(width as f32) / 2.0;
        Aabb::new(
            Vec3::new(position.x - w, position.y, position.z - w),
            Vec3::new(position.x + w, position.y + f64::from(height as f32), position.z + w),
        )
    }
    pub fn hitbox(&self) -> Aabb {
        self.bounds()
    }
    pub fn is_invulnerable(&self) -> bool {
        self.vitals.invulnerable_flag || self.vitals.invulnerable_time > 0
    }
    pub fn distance_sqr(&self, p: Vec3) -> f64 {
        self.motion.position.distance_squared(p)
    }
}
