//! Opt-in client targeting. Vanilla reach attributes and server attack validation stay intact.
use super::{Game, Hit, commands::Target};
use mod_api::{ClientTargeting, CrosshairTarget, EntityRef};

fn target_point(eye: [f64; 3], bounds: [[f64; 3]; 2], reach: f64) -> Option<([f64; 3], f64)> {
    if !reach.is_finite()
        || reach < 0.0
        || (0..3).any(|i| {
            !eye[i].is_finite()
                || !bounds[0][i].is_finite()
                || !bounds[1][i].is_finite()
                || bounds[0][i] > bounds[1][i]
        })
    {
        return None;
    }
    let point = std::array::from_fn(|i| eye[i].clamp(bounds[0][i], bounds[1][i]));
    let distance = (0..3)
        .map(|i| (point[i] - eye[i]).powi(2))
        .sum::<f64>()
        .sqrt();
    (distance <= reach).then_some((point, distance))
}

impl Game<'_> {
    /// The same traversal without `filterHitResult`: the block hit that
    /// bounds the entity ray in `GameRenderer.pick`, out to the larger
    /// interaction range — a wall between the two still hides the entity.
    pub(super) fn pick_unfiltered(&self) -> Option<Hit> {
        if let Some(hit) = self.forced_hit {
            return Some(hit);
        }
        let (origin, dir) = self.aim()?;
        self.clip_ray(origin, dir, self.block_reach.max(self.entity_reach))
    }

    pub(super) fn targeting(&self) -> ClientTargeting {
        let mut options = self.client_targeting.get();
        options.entity = options.entity.filter(|e| self.entity(e.id()) == Some(*e));
        options
    }
    pub(super) fn clip_targeting_ray(
        &self,
        origin: [f64; 3],
        dir: [f64; 3],
        reach: f64,
    ) -> Option<Hit> {
        if !self.targeting().pick_liquids {
            return self.clip_ray(origin, dir, reach);
        }
        use horizon_core::geometry::Vec3;
        use horizon_world::world_ray::{RayBlockMode, RayFluidMode, clip_world};
        // Boat placement already supplies the block outlines and real fluid shapes.
        let mut rays = super::boats::OutlineFluidRay(self);
        let start = Vec3::new(origin[0], origin[1], origin[2]);
        let end = Vec3::new(
            origin[0] + dir[0] * reach,
            origin[1] + dir[1] * reach,
            origin[2] + dir[2] * reach,
        );
        let result = clip_world(
            &mut rays,
            start,
            end,
            RayBlockMode::Outline,
            RayFluidMode::Any,
            4096,
        )
        .ok()?;
        if result.miss {
            return None;
        }
        let hit = result.hit;
        Some(Hit {
            pos: [hit.block.x, hit.block.y, hit.block.z],
            face: hit.direction.normal(),
            point: [hit.location.x, hit.location.y, hit.location.z],
            inside: hit.inside,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_range_uses_nearest_box_point_and_rejects_invalid_geometry() {
        let bounds = [[2.0, -1.0, -1.0], [4.0, 1.0, 1.0]];
        assert_eq!(
            target_point([0.0; 3], bounds, 2.0),
            Some(([2.0, 0.0, 0.0], 2.0))
        );
        assert_eq!(target_point([0.0; 3], bounds, 1.999), None);
        assert_eq!(
            target_point([3.0, 0.0, 0.0], bounds, 0.0),
            Some(([3.0, 0.0, 0.0], 0.0))
        );
        for reach in [-1.0, f64::NAN, f64::INFINITY] {
            assert_eq!(target_point([0.0; 3], bounds, reach), None);
        }
        assert_eq!(target_point([f64::NAN; 3], bounds, 3.0), None);
        assert_eq!(target_point([0.0; 3], [bounds[1], bounds[0]], 3.0), None);
    }
}
