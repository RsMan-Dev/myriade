//! Opt-in client targeting. Vanilla reach attributes and server attack validation stay intact.
use super::{Game, Hit};

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
}

