//! An end crystal on the host. The ECS holds each one (`minecraft:end_crystal`); the fight reads its place, beam and shield through
//! these, and its `alive_crystals` count follows the placements and removals.
use super::*;

/// A crystal as the fight reads it: its network id, its feet, `ShowBottom`, its beam and its shield.
#[derive(Clone, Copy, Debug)]
pub(in crate::game) struct CrystalView {
    pub id: u32,
    pub position: Vec3,
    pub show_bottom: bool,
    pub beam: Option<[i32; 3]>,
    pub invulnerable: bool,
    pub join: horizon_client::join_seq::JoinSeq,
}

/// `updateCrystalCount`'s test for a crystal at `at` on the spikes of `seed`.
pub(in crate::game) fn spike_top(seed: i64, at: Vec3) -> bool {
    horizon_world::worldgen::end_spike::spikes_for_seed(seed).iter().any(|s| {
        (at.x - f64::from(s.center_x) - 0.5).abs() <= f64::from(s.radius)
            && (at.z - f64::from(s.center_z) - 0.5).abs() <= f64::from(s.radius)
            && at.y >= f64::from(s.height)
    })
}

/// `EntityType.END_CRYSTAL`: 2 x 2 from its feet, up 2.
pub(in crate::game) fn crystal_box(p: Vec3) -> Aabb {
    Aabb::new(Vec3::new(p.x - 1.0, p.y, p.z - 1.0), Vec3::new(p.x + 1.0, p.y + 2.0, p.z + 1.0))
}

impl Game<'_> {
    /// The view of crystal `id`; None for an id that is no crystal.
    pub(in crate::game) fn crystal(&self, id: u32) -> Option<CrystalView> {
        let (show_bottom, beam, invulnerable) = self.ecs.crystal_state(id)?;
        let p = self.ecs.position(id)?;
        let e = self.ecs.entity_of(id)?;
        let join = self.ecs.ecs().and_then(|w| w.world().get::<horizon_ecs_proto::hanging::JoinedAt>(e)).map_or_else(Default::default, |j| j.0);
        Some(CrystalView { id, position: Vec3::new(p[0], p[1], p[2]), show_bottom, beam, invulnerable, join })
    }

    /// Every crystal of the level, in id order.
    pub(in crate::game) fn crystals(&self) -> Vec<CrystalView> {
        self.ecs.crystal_ids().into_iter().filter_map(|id| self.crystal(id)).collect()
    }

    /// The boxes of the level's crystals.
    pub(in crate::game) fn crystal_boxes(&self) -> Vec<Aabb> {
        self.crystals().into_iter().map(|c| crystal_box(c.position)).collect()
    }

    /// `updateCrystalCount`'s test: a crystal at `at` stands on a spike top.
    pub(in crate::game) fn on_spike_top(&self, at: Vec3) -> bool {
        spike_top(self.level.seed, at)
    }

    /// An end crystal at `at` (its feet) in the ECS; its id.
    /// It counts in `alive_crystals` on a spike top (`EnderDragonFight.updateCrystalCount`, kept by events).
    pub(in crate::game) fn spawn_crystal(&mut self, at: Vec3, show_bottom: bool) -> u32 {
        let mut next = self.next_entity;
        let id = self.ecs_with(|ecs, mw| {
            ecs.spawn_crystal(mw, [at.x, at.y, at.z], show_bottom, &mut || {
                next += 1;
                next
            })
        });
        self.next_entity = next;
        if self.on_spike_top(at) {
            self.fight.alive_crystals += 1;
        }
        id
    }

    /// A crystal that enters the level by its saved data (`EndCrystal` loaded with its chunk, or summoned): it counts on a spike top.
    pub(in crate::game) fn count_loaded_crystal(&mut self, id: u32) {
        if let Some(c) = self.crystal(id)
            && self.on_spike_top(c.position)
        {
            self.fight.alive_crystals += 1;
        }
    }

    /// Takes crystal `id` out of the ECS, with no blast.
    pub(in crate::game) fn remove_crystal(&mut self, id: u32) {
        let Some(c) = self.crystal(id) else {
            return;
        };
        if self.on_spike_top(c.position) {
            self.fight.alive_crystals -= 1;
        }
        if let Some(e) = self.ecs.entity_of(id) {
            self.ecs.despawn(e);
        }
    }

    /// Takes every crystal of the level out, with no blast.
    pub(in crate::game) fn clear_crystals(&mut self) {
        for c in self.crystals() {
            self.remove_crystal(c.id);
        }
    }

    /// `setBeamTarget` on crystal `id`.
    pub(in crate::game) fn set_crystal_beam(&mut self, id: u32, to: Option<[i32; 3]>) {
        self.ecs.set_crystal_beam(id, to);
    }

    /// `setPermanentlyInvulnerable` on crystal `id`.
    pub(in crate::game) fn set_crystal_shield(&mut self, id: u32, on: bool) {
        self.ecs.set_crystal_shield(id, on);
    }
}
