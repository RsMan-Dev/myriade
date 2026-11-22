//! The dragon's breath clouds on the host. The ECS holds each one (`minecraft:area_effect_cloud` with a `breath::Cloud`); the host reads
//! its place, radius, age and duration through these, and removes it by its network id.
use super::*;
use horizon_ecs_proto::{cow::now_of, dragon::breath::Cloud};

/// A breath cloud as the host reads it: its network id, its floor, its radius, its total age and its duration.
#[derive(Clone, Copy, Debug)]
pub(in crate::game) struct CloudView {
    pub id: u32,
    pub at: [f64; 3],
    pub radius: f32,
    pub age: i32,
    pub duration: i32,
}

impl CloudView {
    /// `AreaEffectCloud.DATA_WAITING`: the cloud has not started its radius and its hits yet.
    pub(in crate::game) fn waiting(&self) -> bool {
        self.age < horizon_ecs_proto::cloud_tick::WAIT
    }
}

impl Game<'_> {
    /// The breath cloud of network id `id`; None for an unknown id or a cloud that is not a breath's.
    pub(in crate::game) fn breath_cloud(&self, id: u32) -> Option<CloudView> {
        let e = self.ecs.entity_of(id)?;
        let w = self.ecs.world()?;
        let c = w.get::<Cloud>(e)?;
        let live = w.get::<horizon_ecs_proto::breath_cloud::Live>(e).is_some();
        let age = if live { c.age } else { c.age + now_of(w, e) as i32 };
        Some(CloudView { id, at: c.at, radius: c.radius, age, duration: c.duration })
    }

    /// The breath clouds of the level, in id order.
    pub(in crate::game) fn breath_clouds(&self) -> Vec<CloudView> {
        self.ecs
            .species_ids(horizon_ecs_proto::species::lists::AREA_EFFECT_CLOUD)
            .into_iter()
            .filter_map(|id| self.breath_cloud(id))
            .collect()
    }

    /// Removes every breath cloud of the level (the world is closed).
    pub(in crate::game) fn clear_breath_clouds(&mut self) {
        for c in self.breath_clouds() {
            self.remove_breath_cloud(c.id);
        }
    }

    /// Removes the breath cloud of network id `id` (its end, a command, a portal), with its stored `noPhysics` write.
    pub(in crate::game) fn remove_breath_cloud(&mut self, id: u32) {
        if let Some(e) = self.ecs.entity_of(id) {
            self.ecs.despawn(e);
        }
        self.forget_entity(id);
    }
}
