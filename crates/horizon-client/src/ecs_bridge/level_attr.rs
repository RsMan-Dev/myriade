//! Absorption and attribute modifiers of a living entity, by network id (lot 75). Reads answer None and writes false when the level is
//! inert, the id is unknown, or the entity is gone.
use super::level::EcsLevel;
use horizon_ecs_proto::attr_mod::Modifier;

impl EcsLevel {
    /// The absorption of `id`; None when inert or unknown.
    pub fn absorption(&self, id: u32) -> Option<f32> {
        let e = self.entity_of(id)?;
        Some(self.ecs()?.absorption(e))
    }
    /// Writes the absorption of `id`; false when inert or the entity does not exist.
    pub fn set_absorption(&mut self, id: u32, amount: f32) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        let Some(w) = self.ecs_mut().filter(|w| w.world().get_entity(e).is_ok()) else { return false };
        w.set_absorption(e, amount);
        true
    }
    /// Adds a modifier to attribute `name` of `id`; false when inert, unknown, or the id is already applied.
    pub fn add_attribute_modifier(&mut self, id: u32, name: &str, m: Modifier) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.add_attribute_modifier(e, name, m))
    }
    /// Removes the modifier `mod_id` from attribute `name` of `id`; false when inert, unknown, or absent.
    pub fn remove_attribute_modifier(&mut self, id: u32, name: &str, mod_id: &str) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.remove_attribute_modifier(e, name, mod_id))
    }
    /// Writes the base of attribute `name` of `id` over the species' one; false when inert or unknown.
    pub fn set_attribute_base(&mut self, id: u32, name: &str, base: f64) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.set_attribute_base(e, name, base))
    }
    /// Gives attribute `name` of `id` the species' own base again; false when inert or unknown.
    pub fn reset_attribute_base(&mut self, id: u32, name: &str) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.reset_attribute_base(e, name))
    }
    /// The entities whose attribute instances changed since the last call; empty when inert.
    pub fn drain_attribute_changes(&mut self) -> Vec<bevy_ecs::entity::Entity> {
        self.ecs_mut().map_or_else(Vec::new, |w| w.drain_attribute_changes())
    }
    /// `EcsWorld::attribute_instances` of `e`: (name, base written over the species', modifiers).
    pub fn attribute_instances(&self, e: bevy_ecs::entity::Entity) -> Vec<(String, Option<f64>, Vec<Modifier>)> {
        self.ecs().map_or_else(Vec::new, |w| w.attribute_instances(e))
    }
    /// The entities whose attribute instances changed since the last call; empty when inert.
    pub fn drain_attribute_changes(&mut self) -> Vec<bevy_ecs::entity::Entity> {
        self.ecs_mut().map_or_else(Vec::new, |w| w.drain_attribute_changes())
    }
    /// `EcsWorld::attribute_instances` of `e`: (name, base written over the species', modifiers).
    pub fn attribute_instances(&self, e: bevy_ecs::entity::Entity) -> Vec<(String, Option<f64>, Vec<Modifier>)> {
        self.ecs().map_or_else(Vec::new, |w| w.attribute_instances(e))
    }
}
