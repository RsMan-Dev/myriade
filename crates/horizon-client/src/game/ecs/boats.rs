//! The boat outputs from the ECS (`BoatEvent`, `BoatEffectEvent`, `BoatHurtEvent`, `SeatEvent`, merged into `HostEvent`): one receiver
//! per family. Each calls the host's effect (sound, bubble, damage message, riding state); the ECS has already decided the outcome.
use crate::game::{Game, commands::Target};
use horizon_ecs_proto::{
    host_out_boat_effects::BoatEffectEvent, host_out_boat_hurt::BoatHurtEvent,
    host_out_boats::BoatEvent, host_out_seat::SeatEvent, seat::Rider, targeting,
};

impl Game<'_> {
    /// A paddle stroke plays at the boat with the pitch the ECS drew, the water's or the land's sound as the ECS decided.
    pub(in crate::game) fn ecs_boat_event(&mut self, ev: BoatEvent) {
        match ev {
            BoatEvent::Paddle { entity, pitch, water, .. } => {
                let id = self.ecs_id_of(entity);
                let Some(at) = self.ecs.position(id) else {
                    return;
                };
                let sound = if water { "minecraft:entity.boat.paddle_water" } else { "minecraft:entity.boat.paddle_land" };
                self.audio.play(sound, Some(at), 1.0, pitch);
            }
        }
    }
    /// The bubble column's event at the boat (`71` down, `72` up), the lava hiss, the powder snow's flakes, and a discard.
    pub(in crate::game) fn ecs_boat_effect_event(&mut self, ev: BoatEffectEvent) {
        match ev {
            BoatEffectEvent::Bubble { entity, down } => {
                let id = self.ecs_id_of(entity);
                let event = if down {
                    horizon_gameplay::boats::BUBBLE_DOWN_EVENT
                } else {
                    horizon_gameplay::boats::BUBBLE_UP_EVENT
                };
                if let Some(at) = self.ecs.position(id) {
                    self.lan_entity_event(id as i32, event, at);
                }
            }
            BoatEffectEvent::LavaHurt { at } => self.lava_hurt_sound(at),
            BoatEffectEvent::Snowflakes { at, velocity } => self.boat_snowflakes(at, velocity),
            BoatEffectEvent::Discard { entity } => self.ecs.despawn(entity),
            BoatEffectEvent::Burn { .. } => {}
        }
    }
    /// A hit on a boat shows its damage message; a break shows the removal (the ECS removed the boat already).
    pub(in crate::game) fn ecs_boat_hurt_event(&mut self, ev: BoatHurtEvent) {
        match ev {
            BoatHurtEvent::Hit { entity, by, .. } => {
                let id = self.ecs_id_of(entity);
                if let Some(at) = self.ecs.position(id) {
                    let by = self.boat_hurt_by(by);
                    self.vehicle_damage_event(at, by);
                }
            }
            // The contents and the boat's item are dropped by the ECS.
            BoatHurtEvent::Broke { at, .. } => self.native_vehicle_destroy("boat", at),
        }
    }
    /// A rider taking or leaving a seat: a player's riding state follows it.
    pub(in crate::game) fn ecs_seat_event(&mut self, ev: SeatEvent) {
        match ev {
            SeatEvent::Boarded {
                rider: Rider::Player(_),
                vehicle,
            } => {
                let id = self.ecs_id_of(vehicle);
                self.note_started_riding(id);
            }
            SeatEvent::Left {
                rider: Rider::Player(_),
                vehicle,
            } => {
                let id = self.ecs_id_of(vehicle);
                self.note_stopped_riding(id);
            }
            SeatEvent::Boarded { .. } | SeatEvent::Left { .. } => {}
        }
    }
    /// The host's `Who` of a boat's attacker: the player, or the mob of its ECS entity (its network id, linked here when it has none).
    pub(in crate::game) fn boat_hurt_by(&mut self, by: Option<targeting::Who>) -> Option<Target> {
        match by? {
            targeting::Who::Player(_) => Some(Target::Player),
            targeting::Who::Entity(e) => Some(Target::Mob(self.ecs_id_of(e))),
        }
    }
    /// Whether the ECS boat `id` has a chest; None when it is not a boat.
    pub(in crate::game) fn boat_has_chest(&self, id: u32) -> Option<bool> {
        let e = self.ecs.entity_of(id)?;
        self.ecs.boat_wood(e).map(|(_, chest)| chest)
    }
    /// The powder snow's flakes at a boat, the snowflake particle of the host.
    pub(in crate::game) fn boat_snowflakes(&mut self, at: [f64; 3], velocity: [f64; 3]) {
        self.snowflake_particle(at, velocity);
    }
    /// A player left the boat `vehicle`: the host's riding state follows it when it was that boat.
    pub(in crate::game) fn note_stopped_riding(&mut self, vehicle: u32) {
        if self.riding == Some(vehicle) {
            self.riding = None;
        }
    }
}
