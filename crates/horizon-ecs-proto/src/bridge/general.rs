//! The general entities and the displays and interactions as the host reaches them: a general entity holds its payload in `General`,
//! a click on an interaction is recorded by `interaction` and reported as `GeneralEvent::Clicked` in `GeneralEvents`. The registry's
//! slot (`kind`) is resolved by the host, which owns the mods' table; P only stores it.
use crate::bridge::host::EcsWorld;
use crate::display;
use crate::display_tick;
use crate::general::{self, General};
use crate::host_out_general::GeneralEvent;
use crate::interaction;
use crate::passive::Interact;
use bevy_ecs::entity::Entity;
use horizon_core::geometry::Vec3;
use horizon_core::nbt::{Compound, Tag};

impl EcsWorld {
    /// A general entity of registry slot `kind` at `at`, with its UUID and payload, from the summon or the load.
    pub fn spawn_general(&mut self, kind: u16, at: [f64; 3], uuid: [i32; 4], payload: Vec<u8>) -> Entity {
        let g = General { kind, payload_version: 0, payload, uuid, rotation: [0.0; 2] };
        general::spawn(self.world_mut(), Vec3::new(at[0], at[1], at[2]), g)
    }

    /// The payload of general entity `e`, as the mod wrote it; None when `e` is not one.
    pub fn general_payload(&self, e: Entity) -> Option<&[u8]> {
        self.world().get::<General>(e).map(|g| g.payload.as_slice())
    }

    /// Writes the payload of general entity `e`; `Changed` is emitted when it differs.
    pub fn set_general(&mut self, e: Entity, payload: Vec<u8>) {
        general::set_payload(self.world_mut(), e, payload);
    }

    /// Subscribes general entity `e` to its tick (`on`), or unsubscribes it; false when `e` is not one.
    pub fn subscribe_general(&mut self, e: Entity, on: bool) -> bool {
        general::subscribe(self.world_mut(), e, on)
    }

    /// Asks general entity `e` for one `ModTick` `ticks` ticks from now (a deadline).
    pub fn request_general_tick(&mut self, e: Entity, ticks: i32) {
        general::request_tick(self.world_mut(), e, ticks);
    }

    /// An entity of a type nothing here runs, kept as `c`; None when `c` has no string `id`.
    pub fn spawn_unknown(&mut self, c: Compound) -> Option<Entity> {
        general::spawn_unknown(self.world_mut(), c)
    }

    /// A display of `kind` at `at` from its saved compound (see `display::spawn_display`).
    pub fn spawn_display(&mut self, kind: &'static str, at: [f64; 3], tag: Tag) -> Entity {
        display::spawn_display(self.world_mut(), kind, Vec3::new(at[0], at[1], at[2]), tag)
    }

    /// Starts the interpolation of display `e` (`display_tick::start_interpolation`); false when `e` is not a display.
    pub fn start_interpolation(&mut self, e: Entity, delay: i32, duration: i32) -> bool {
        display_tick::start_interpolation(self.world_mut(), e, delay, duration)
    }

    /// An interaction box at `at` of `width` and `height`, `response` or not (`interaction::spawn`).
    pub fn spawn_interaction(&mut self, at: [f64; 3], width: f32, height: f32, response: bool) -> Entity {
        interaction::spawn(self.world_mut(), Vec3::new(at[0], at[1], at[2]), width, height, response)
    }

    /// An attack by a player (`uuid`) on interaction `e` at `tick`. True when the blow is skipped; `Clicked` (with `player`) is emitted
    /// for an interaction only, and nothing happens to any other entity.
    pub fn attack_interaction(&mut self, e: Entity, player: u32, uuid: [i32; 4], tick: i64) -> bool {
        if self.world().get::<Interact>(e).is_none() {
            return false;
        }
        let skipped = interaction::attack(self.world_mut(), e, uuid, tick);
        general::emit(self.world_mut(), GeneralEvent::Clicked { entity: e, player, attack: true });
        skipped
    }

    /// A use (right click) by a player (`uuid`) on interaction `e` at `tick`. `Clicked` (with `player`) is emitted for an interaction only.
    pub fn click_interaction(&mut self, e: Entity, player: u32, uuid: [i32; 4], tick: i64) {
        if self.world().get::<Interact>(e).is_none() {
            return;
        }
        interaction::click(self.world_mut(), e, uuid, tick);
        general::emit(self.world_mut(), GeneralEvent::Clicked { entity: e, player, attack: false });
    }
}
