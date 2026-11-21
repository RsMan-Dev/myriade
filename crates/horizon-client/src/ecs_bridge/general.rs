//! The general entities, displays and interactions as the server's level reaches them: an entity is bound to its network id by `alloc`
//! when it is made, and every call finds it by that id. The registry slot (`kind`) and the key table are the caller's. An inert level
//! answers as an empty world: none, false, nothing. A click counts only when its attacker is a player (`Who::Player`).
use super::EcsLevel;
use horizon_core::nbt::{Compound, Tag};
use horizon_ecs_proto::targeting::Who;

impl EcsLevel {
    /// Summons a general entity of registry slot `kind` at `at` and returns its network id, bound by `alloc`; None when inert.
    pub fn spawn_general(&mut self, kind: u16, at: [f64; 3], uuid: [i32; 4], payload: Vec<u8>, alloc: &mut dyn FnMut() -> u32) -> Option<u32> {
        let w = self.ecs_mut()?;
        let e = w.spawn_general(kind, at, uuid, payload);
        Some(w.id_for(e, alloc))
    }

    /// The payload of the general entity `id`; None when inert or when `id` is not one.
    pub fn general_payload(&self, id: u32) -> Option<Vec<u8>> {
        let w = self.ecs()?;
        w.general_payload(w.entity_of(id)?).map(<[u8]>::to_vec)
    }

    /// Writes the payload of the general entity `id`; nothing when inert or when `id` is not one.
    pub fn set_general(&mut self, id: u32, payload: Vec<u8>) {
        let Some(w) = self.ecs_mut() else { return };
        if let Some(e) = w.entity_of(id) {
            w.set_general(e, payload);
        }
    }

    /// Subscribes the general entity `id` to its tick (`on`); false when inert or when `id` is not one.
    pub fn subscribe_general(&mut self, id: u32, on: bool) -> bool {
        let Some(w) = self.ecs_mut() else { return false };
        w.entity_of(id).is_some_and(|e| w.subscribe_general(e, on))
    }

    /// Asks the general entity `id` for one `ModTick` `ticks` ticks from now; nothing when inert or when `id` is not one.
    pub fn request_general_tick(&mut self, id: u32, ticks: i32) {
        let Some(w) = self.ecs_mut() else { return };
        if let Some(e) = w.entity_of(id) {
            w.request_general_tick(e, ticks);
        }
    }

    /// Summons an entity of a type nothing here runs, kept as `c`, and returns its network id; None when inert or when `c` has no `id`.
    pub fn spawn_unknown(&mut self, c: Compound, alloc: &mut dyn FnMut() -> u32) -> Option<u32> {
        let w = self.ecs_mut()?;
        let e = w.spawn_unknown(c)?;
        Some(w.id_for(e, alloc))
    }

    /// Summons a display of `kind` from its saved compound at `at` and returns its network id; None when inert.
    pub fn spawn_display(&mut self, kind: &'static str, at: [f64; 3], tag: Tag, alloc: &mut dyn FnMut() -> u32) -> Option<u32> {
        let w = self.ecs_mut()?;
        let e = w.spawn_display(kind, at, tag);
        Some(w.id_for(e, alloc))
    }

    /// Starts the interpolation of the display `id`; false when inert or when `id` is not one.
    pub fn start_interpolation(&mut self, id: u32, delay: i32, duration: i32) -> bool {
        let Some(w) = self.ecs_mut() else { return false };
        w.entity_of(id).is_some_and(|e| w.start_interpolation(e, delay, duration))
    }

    /// Summons an interaction box of `width` and `height` at `at` and returns its network id; None when inert.
    pub fn spawn_interaction(&mut self, at: [f64; 3], width: f32, height: f32, response: bool, alloc: &mut dyn FnMut() -> u32) -> Option<u32> {
        let w = self.ecs_mut()?;
        let e = w.spawn_interaction(at, width, height, response);
        Some(w.id_for(e, alloc))
    }

    /// A blow by `who` on the interaction `id` at `tick`: true when it is skipped. False when inert, when `id` is not an interaction, or
    /// when `who` is not a player.
    pub fn attack_interaction(&mut self, id: u32, who: Who, uuid: [i32; 4], tick: i64) -> bool {
        let Some(w) = self.ecs_mut() else { return false };
        let (Some(e), Who::Player(p)) = (w.entity_of(id), who) else { return false };
        w.attack_interaction(e, p, uuid, tick)
    }

    /// A use by `who` on the interaction `id` at `tick`. Nothing when inert, when `id` is not an interaction, or when `who` is not a player.
    pub fn click_interaction(&mut self, id: u32, who: Who, uuid: [i32; 4], tick: i64) {
        let Some(w) = self.ecs_mut() else { return };
        let (Some(e), Who::Player(p)) = (w.entity_of(id), who) else { return };
        w.click_interaction(e, p, uuid, tick);
    }
}
