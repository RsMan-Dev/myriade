//! The slots of a mob loop's snapshot by species, so a search for one species reads its few
//! entries instead of every entry near it (vanilla's `getEntitiesOfClass` reads per class).
use super::{Seen, Species};

/// Slots of the snapshot grouped by species, ascending within each.
#[derive(Default)]
pub struct SpeciesSlots {
    order: Vec<u32>,
    starts: Vec<u32>,
}

impl SpeciesSlots {
    /// Counting sort of the snapshot's slots by species.
    pub fn new(snapshot: &[Seen]) -> Self {
        let kinds = snapshot
            .iter()
            .map(|s| s.species as usize + 1)
            .max()
            .unwrap_or(0);
        let mut starts = vec![0u32; kinds + 1];
        for s in snapshot {
            starts[s.species as usize + 1] += 1;
        }
        for i in 0..kinds {
            starts[i + 1] += starts[i];
        }
        let mut next = starts.clone();
        let mut order = vec![0u32; snapshot.len()];
        for (slot, s) in snapshot.iter().enumerate() {
            let at = &mut next[s.species as usize];
            order[*at as usize] = slot as u32;
            *at += 1;
        }
        Self { order, starts }
    }
    /// The slots holding `species`, ascending.
    #[inline]
    pub fn of(&self, species: Species) -> &[u32] {
        let i = species as usize;
        match (self.starts.get(i), self.starts.get(i + 1)) {
            (Some(&a), Some(&b)) => &self.order[a as usize..b as usize],
            _ => &[],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobs::{seen_grid::SeenGrid};
    use horizon_core::geometry::{Aabb, Vec3};
    use std::sync::Arc;

    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self, n: u64) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (self.0 >> 33) % n
        }
        fn unit(&mut self) -> f64 {
            self.next(1 << 20) as f64 / (1 << 20) as f64
        }
    }

    fn around(p: Vec3) -> Aabb {
        Aabb::new(
            Vec3::new(p.x - 0.3, p.y, p.z - 0.3),
            Vec3::new(p.x + 0.3, p.y + 1.8, p.z + 0.3),
        )
    }

}
