//! `explosion_detonate`: the blocks and entities a server explosion is about to
//! reach (NeoForge's `ExplosionEvent.Detonate`), and the list of them the
//! listeners leave behind.
use super::Game;
use crate::game::entity::EntityHandle;
use mod_api::EntityRef;
use std::collections::HashSet;

impl Game<'_> {
    /// `ServerExplosion.explode` posting `onExplosionDetonate` before any block or
    /// entity is hit. The entities are `level.getEntities(source, box)`, the box
    /// the centre ± (2 × radius + 1) floored, without the exploding source; every
    /// kind is in it: the entity-list walk, then the item frames, paintings,
    /// cushions and end crystals the walk misses. The list they leave in
    /// `combat.blast_hit_list` is what the blast's walks then use: what a
    /// listener leaves in it is eligible for the normal hurt/push checks.
    /// Order controls only the ordinary-storage walk, not the separate crystal,
    /// dragon, player and decoration passes. Without a listener nothing is listed, and the walks
    /// enumerate the storages as before.
    pub(super) fn explosion_detonate(
        &mut self,
        center: [f64; 3],
        radius: f32,
        blocks: &mut Vec<[i32; 3]>,
    ) {
        // `ServerExplosion.hurtEntities` fires it only for a radius of at least 0.00001.
        if radius < 1.0e-5 || !false {
            return;
        }
        let reach = f64::from(radius) * 2.0 + 1.0;
        let min = center.map(|c| (c - reach).floor());
        let max = center.map(|c| (c + reach).floor());
        let source = self
            .combat
            .explosion_direct
            .or(self.combat.explosion_source);
        let mut entities = self
            .level(self.dim.id())
            .and_then(|level| self.level_entities_in(level, min, max))
            .unwrap_or_default();
        entities.retain(|entity| Some(entity.id()) != source);
        self.append_blast_specials(&mut entities, source, min, max);
        let dimension = self.dim.id();
        self.native_explosion_detonate(dimension, center, blocks, &mut entities);
        // The walks follow the list: one a listener added is walked as any
        // other, so a handle that names nothing real is refused here, and a
        // repeated id is taken once (`getEntities` lists an entity once).
        entities.retain(|entity| self.native_entity_target(*entity).is_some());
        let mut seen = HashSet::new();
        entities.retain(|entity| seen.insert(entity.id()));
        self.combat.blast_hit_ids = seen;
        self.combat.blast_hit_list = Some(entities.iter().map(|entity| entity.id()).collect());
    }

    /// The four entity kinds the entity-list walk misses: item frames,
    /// paintings, cushions and end crystals. Appended after it, in their
    /// storage order, those whose box meets the blast's box
    /// (`Level.getEntities`), the exploding source excluded with the rest.
    fn append_blast_specials(
        &self,
        entities: &mut Vec<EntityRef>,
        source: Option<u32>,
        min: [f64; 3],
        max: [f64; 3],
    ) {
        let Some(epoch) = self.native_lifecycle.epoch() else {
            return;
        };
        let in_box = |lo: [f64; 3], hi: [f64; 3]| (0..3).all(|k| lo[k] < max[k] && hi[k] > min[k]);
        let listed = |id: u32, lo: [f64; 3], hi: [f64; 3], entities: &mut Vec<EntityRef>| {
            if Some(id) != source && in_box(lo, hi) {
                entities.push(EntityRef::new(epoch, id));
            }
        };
        for frame in self.ecs_frames().into_iter() {
            let (lo, hi) = frame.bounds();
            listed(frame.id, lo, hi, entities);
        }
        for painting in self.ecs_paintings().into_iter() {
            let (lo, hi) = painting.bounds();
            listed(painting.id, lo, hi, entities);
        }
        for cushion in self.cushion_list() {
            let (lo, hi) = cushion.bounds();
            listed(cushion.id, lo, hi, entities);
        }
        for (id, _) in self.ecs.crystal_views() {
            let Some(bounds) = self.ecs.bounds(id) else {
                continue;
            };
            let lo = [
                bounds.min(horizon_core::geometry::Axis::X),
                bounds.min(horizon_core::geometry::Axis::Y),
                bounds.min(horizon_core::geometry::Axis::Z),
            ];
            let hi = [
                bounds.max(horizon_core::geometry::Axis::X),
                bounds.max(horizon_core::geometry::Axis::Y),
                bounds.max(horizon_core::geometry::Axis::Z),
            ];
            listed(id, lo, hi, entities);
        }
    }

    /// The handle of the storaged entity `id` names, for the listed walk. No
    /// storage holds mobs any more (the ECS hurts them in `explode_ecs`), so
    /// every id answers None.
    pub(in crate::game) fn blast_handle(&self, _id: u32) -> Option<EntityHandle> {
        None
    }
}
