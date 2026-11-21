//! The ECS's entities as the trackers send them: the ones the world modified since the last tick, the ones it removed, and the
//! ones near a player who changed section. Each is read through the bridge's view; nothing walks the whole world.
use super::super::Game;
use bevy_ecs::entity::Entity;
use horizon_client::{fast_hash::FxHashSet, lan::entity::EntityState};
use horizon_ecs_proto::bridge::host::Select;

fn section(p: [f64; 3]) -> [i32; 3] {
    p.map(|v| (v.floor() as i32) >> 4)
}

impl Game<'_> {
    /// The tracker's state of `e`, under its network id (linked on its first follow). None when `e` sleeps: a sleeping entity
    /// stays tracked and sends nothing, and is not removed.
    fn ecs_state(&mut self, e: Entity, attributes: bool) -> Option<EntityState> {
        // The sleep test comes before the id is linked: a sleeping entity gets no id and no state.
        let mut state = EntityState::default();
        if !self.ecs.view_state(e, 0, &mut state) {
            return None;
        }
        let id = self.ecs.id_for(e, &mut || {
            self.next_entity += 1;
            self.next_entity
        });
        self.ecs.view_state(e, id as i32, &mut state);
        self.ecs.metadata(e, &mut state.meta);
        // The attribute instances only when they changed or the entity is being paired: a state without them leaves what was sent.
        if attributes {
            state.attributes = self.ecs_attributes(e, &state.kind);
        }
        if mod_api::__private::listened(mod_api::HookId::EffectParticleModification) {
            self.ecs_effect_mods(id, &mut state.meta);
        }
        Some(state)
    }

    /// `tick_lan` for an ECS level: the tracker's pass over the modified entities, the removed ids, and the entities near the
    /// players who changed section (their pairs are checked again). Nothing when no ECS runs or nothing changed.
    pub(in crate::game) fn tick_lan_ecs(&mut self) {
        if !self.ecs.is_active() {
            return;
        }
        let moved: Vec<([f64; 3], i32)> = match self.lan.as_ref() {
            Some(host) => host
                .hub
                .guests
                .iter()
                .filter(|g| host.hub.here(g) && section(g.position) != section(g.previous_position))
                .map(|g| (g.position, g.view_distance))
                .collect(),
            None => return,
        };
        let dirty = self.ecs.drain_attribute_changes();
        let pairing = !moved.is_empty();
        let mut changed = Vec::new();
        let mut done = FxHashSet::default();
        for e in self.ecs.drain_modified() {
            // `dirty` is sorted by `to_bits` (`drain_attribute_changes`).
            let with = pairing || dirty.binary_search_by_key(&e.to_bits(), |x| x.to_bits()).is_ok();
            done.insert(e);
            changed.extend(self.ecs_state(e, with));
        }
        for &e in dirty.iter().filter(|e| !done.contains(*e)) {
            changed.extend(self.ecs_state(e, true));
        }
        let mut seen: FxHashSet<i32> = changed.iter().map(|s| s.id).collect();
        for (at, view) in moved {
            // A client sees no further than its view distance (`sees`), so the entities within it are the ones to pair.
            let q = Select { kind: None, tag: None, near: Some((at, f64::from(view * 16))), limit: usize::MAX };
            for e in self.ecs.select(&q) {
                if let Some(s) = self.ecs_state(e, true)
                    && seen.insert(s.id)
                {
                    changed.push(s);
                }
            }
        }
        let gone = self.ecs.drain_removed();
        for id in &gone {
            self.ecs_effect_particles.remove(&(*id as u32));
        }
        if changed.is_empty() && gone.is_empty() {
            return;
        }
        if let Some(host) = self.lan.as_mut() {
            host.hub.track_changed(&changed, &gone);
        }
    }
}
