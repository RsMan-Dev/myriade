//! Opt-in client targeting. Vanilla reach attributes and server attack validation stay intact.
use super::{Game, Hit};
use mod_api::ClientTargeting;

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

