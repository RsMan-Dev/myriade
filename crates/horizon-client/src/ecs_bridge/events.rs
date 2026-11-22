//! The ECS host events as the server's `MobEvent`s, which `handle_mob_events_reusable` applies with the effect the mobs' own events have.
//! `None`: no `MobEvent` of the same meaning, or an entity without a network id or a position; the caller keeps the event.
//! Variant to action: SetBlock, Sound, Particles, EntityEvent, Darkness, Xp, LevelEvent, ChestOpen, ChestSlot, ParrotToShoulder
//! and PullEntity (a `Push` of the creature) map to their `MobEvent`, and TargetChanged with a target to `SetTarget`; BossBar, PushPlayer,
//! Cloud, DisplayUpdate, GameEvent, Pearl and the lot-46 outputs (Death, ConversionAsk, Spawned,
//! Tamed, HitDealt, Unleashed, TotemAsk) and the lot variants (one per lot, routed by `game/ecs/host_events.rs`) have none and stay with the host (`None`).
use super::level::EcsLevel;
use crate::mobs::{MobEvent, Who};
use bevy_ecs::world::World;
use horizon_ecs_proto::bridge::{ids::id_of, view};
use horizon_ecs_proto::host_out::HostEvent;

impl EcsLevel {
    /// A conversion waits for the host while a mod hears it (`set_conversion_watched`); nothing when inert.
    pub fn set_conversion_watched(&mut self, on: bool) {
        if let Some(w) = self.ecs_mut() {
            w.set_conversion_watched(on);
        }
    }

    /// Walking entities emit their movement game events while `on`; nothing when inert.
    pub fn set_move_events(&mut self, on: bool) {
        if let Some(w) = self.ecs_mut() {
            w.set_move_events(on);
        }
    }

    /// The host's table of what a stack on the ground resists, set once at the level's creation (no-op when inert).
    pub fn set_item_resists_table(&mut self, f: horizon_ecs_proto::item::ItemResistsFn) {
        if let Some(w) = self.ecs_mut() {
            w.set_item_resists_table(f);
        }
    }

    /// The host's table of the attribute modifiers an item grants in a slot, set at the level's creation and again when the world's tables
    /// are staged (no-op when inert).
    pub fn set_item_modifiers_table(&mut self, f: horizon_ecs_proto::equip_attrs::ItemModifiersFn) {
        if let Some(w) = self.ecs_mut() {
            w.set_item_modifiers_table(f);
        }
    }

    /// The answer to a `ConversionAsk` of the entity `e`: the new entity (the old one stays until `finish_conversion`).
    pub fn resolve_conversion(&mut self, e: bevy_ecs::entity::Entity, cancel: bool) -> Option<bevy_ecs::entity::Entity> {
        self.ecs_mut()?.resolve_conversion(e, cancel)
    }

    /// The old entity of a resolved conversion leaves.
    pub fn finish_conversion(&mut self, e: bevy_ecs::entity::Entity) {
        if let Some(w) = self.ecs_mut() {
            w.finish_conversion(e);
        }
    }

    /// A lethal hit leaves the entity `Dying` until `resolve_death` (set while a mod hears `living_death_post`); nothing when inert.
    pub fn set_death_vetoable(&mut self, on: bool) {
        if let Some(w) = self.ecs_mut() {
            w.set_death_vetoable(on);
        }
    }

    /// A lethal hit on a holder of a totem waits for the host while a mod hears `living_use_totem`; nothing when inert.
    pub fn set_totem_watched(&mut self, on: bool) {
        if let Some(w) = self.ecs_mut() {
            w.set_totem_watched(on);
        }
    }

    /// `HitDealt` is emitted only while `on` (a mod hears `hurt_enemy`); nothing when inert.
    pub fn set_hit_watched(&mut self, on: bool) {
        if let Some(w) = self.ecs_mut() {
            w.set_hit_watched(on);
        }
    }

    /// `SpecialDates.isHalloween` of the host's day, for the carved pumpkin of finalized undead; nothing when inert.
    pub fn set_halloween(&mut self, on: bool) {
        if let Some(w) = self.ecs_mut() {
            w.set_halloween(on);
        }
    }

    /// Rereads the calendar at most once a minute (`today` is (month, day)) and tells P `is_halloween` only when the date has changed
    /// since (the first read included); nothing when inert.
    pub fn refresh_date(&mut self, today: impl FnOnce() -> [u32; 2], is_halloween: impl FnOnce([u32; 2]) -> bool) {
        if !self.is_active() || self.date.is_some_and(|(at, _)| at.elapsed() < std::time::Duration::from_secs(60)) {
            return;
        }
        let now = today();
        let changed = self.date.is_none_or(|(_, last)| last != now);
        self.date = Some((std::time::Instant::now(), now));
        if changed {
            self.set_halloween(is_halloween(now));
        }
    }

    /// The answer to a `TotemAsk` of `e`: `cancel` lets it die, else the totem is used up. True when it was saved.
    pub fn resolve_totem(&mut self, mw: &crate::mobs::MobWorld<'_>, e: bevy_ecs::entity::Entity, cancel: bool) -> bool {
        self.ecs_mut().is_some_and(|w| w.resolve_totem(mw, e, cancel))
    }

    /// The end of a lethal hit held by the death policy, of the entity with network id `id` (`None`: the death goes on; `Some(v)`:
    /// the entity lives on with health `v`). Nothing when the id is unknown.
    pub fn loot_dying(&mut self, mw: &crate::mobs::MobWorld<'_>, id: u32) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.loot_dying(mw, e))
    }

    pub fn resolve_death(&mut self, mw: &crate::mobs::MobWorld<'_>, id: u32, veto: Option<f32>) {
        let Some(e) = self.entity_of(id) else { return };
        if let Some(w) = self.ecs_mut() {
            w.resolve_death(mw, e, veto);
        }
    }
}

/// A `targeting::Who` as the `MobEvent`'s: a player (the id is the server's own, not carried by `MobEvent`) or a mob by network id.
fn who_of(world: &World, w: horizon_ecs_proto::targeting::Who) -> Option<Who> {
    Some(match w {
        horizon_ecs_proto::targeting::Who::Player(_) => Who::Player,
        horizon_ecs_proto::targeting::Who::Entity(e) => Who::Mob(id_of(world, e)?),
    })
}

pub fn to_mob_event(world: &World, ev: HostEvent) -> Option<MobEvent> {
    Some(match ev {
        HostEvent::SetBlock { pos, state } => MobEvent::SetBlock { pos, state },
        HostEvent::Sound { name, at, volume, pitch } => MobEvent::Sound(name, at, volume, pitch),
        HostEvent::Particles { kind, at, spread, speed, count } => MobEvent::LevelParticles { kind, at, spread, speed, count },
        HostEvent::EntityEvent { entity, code } => MobEvent::EntityEvent { mob: id_of(world, entity)?, event: code as i8, at: view::position(world, entity)? },
        HostEvent::Darkness { at, radius } => MobEvent::Darkness { at, radius },
        HostEvent::Xp { at, amount } => MobEvent::Experience { at, amount },
        HostEvent::LevelEvent { event, at, data } => MobEvent::LevelEvent { id: event, pos: at, data },
        HostEvent::ChestOpen { chest, open, entity } => MobEvent::ChestOpen { chest, open, mob: id_of(world, entity)? },
        HostEvent::ChestSlot { chest, slot, stack } => MobEvent::ChestSlot { chest, slot, stack },
        HostEvent::ParrotToShoulder { parrot, .. } => MobEvent::ParrotToShoulder { mob: id_of(world, parrot)? },
        HostEvent::PullEntity { entity, velocity } => MobEvent::Push { target: Who::Mob(id_of(world, entity)?), velocity },
        HostEvent::TargetChanged { entity, target: Some(t), .. } => MobEvent::SetTarget { mob: id_of(world, entity)?, target: who_of(world, t)? },
        HostEvent::BossBar { .. }
        | HostEvent::PushPlayer { .. }
        | HostEvent::Cloud { .. }
        | HostEvent::CloudHit { .. }
        | HostEvent::DisplayUpdate { .. }
        | HostEvent::GameEvent { .. }
        | HostEvent::GameEventBy { .. }
        | HostEvent::Pearl { .. }
        | HostEvent::Death { .. }
        | HostEvent::ConversionAsk { .. }
        | HostEvent::TargetChanged { target: None, .. }
        | HostEvent::Spawned { .. }
        | HostEvent::Tamed { .. }
        | HostEvent::HitDealt { .. }
        | HostEvent::Unleashed { .. }
        | HostEvent::TotemAsk { .. }
        | HostEvent::Swing { .. }
        | HostEvent::Hurt { .. }
        | HostEvent::DeathAnimation { .. }
        | HostEvent::DeathSmoke { .. }
        | HostEvent::HeldConsume { .. }
        | HostEvent::ComposterWork { .. }
        | HostEvent::RaiderDied { .. }
        | HostEvent::RaiderJoined { .. }
        | HostEvent::RaiderLeads { .. }
        | HostEvent::RaiderHurt { .. }
        | HostEvent::HeldUsed { .. }
        | HostEvent::HeldSwap { .. }
        | HostEvent::PlayerGameEvent { .. }
        | HostEvent::HeldWorn { .. }
        | HostEvent::HeldGiven { .. }
        | HostEvent::BecomeBlock { .. }
        | HostEvent::Arrow(_)
        | HostEvent::BoatEffect(_)
        | HostEvent::BoatHurt(_)
        | HostEvent::Boat(_)
        | HostEvent::CartMove(_)
        | HostEvent::Cart(_)
        | HostEvent::Equipment(_)
        | HostEvent::Falling(_)
        | HostEvent::Firework(_)
        | HostEvent::Frame(_)
        | HostEvent::General(_)
        | HostEvent::Hook(_)
        | HostEvent::Inside(_)
        | HostEvent::Item(_)
        | HostEvent::Misc(_)
        | HostEvent::Seat(_)
        | HostEvent::Tnt(_)
        | HostEvent::Trident(_)
        | HostEvent::Using(_)
        | HostEvent::VehiclePush(_) => return None,
    })
}

/// Hands the host events that have a `MobEvent` to `run` (the server's `handle_mob_events_reusable`, called by the game with
/// its own access), and returns the others, unchanged, for the caller to keep.
pub(crate) fn apply(
    world: &World,
    evs: Vec<HostEvent>,
    mut run: impl FnMut(&mut Vec<MobEvent>) -> horizon_client::Result<()>,
) -> horizon_client::Result<Vec<HostEvent>> {
    let mut mob = Vec::new();
    let mut kept = Vec::new();
    for ev in evs {
        match to_mob_event(world, ev.clone()) {
            Some(m) => mob.push(m),
            None => kept.push(ev),
        }
    }
    run(&mut mob)?;
    Ok(kept)
}
