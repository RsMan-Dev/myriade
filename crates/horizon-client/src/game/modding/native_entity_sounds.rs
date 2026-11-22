//! An entity's sounds, game events, entity events and spectator state for
//! native server mods, through an `EntityRef` (docs/modding/src/how-to/native-mods.md, "Entity
//! and level handles"): `Entity.playSound`, `gameEvent`,
//! `Level.broadcastEntityEvent`, `getSoundSource` and `isSpectator`. They
//! return None or false for a stale handle, on a client, and for an entity
//! that is not in the level in place (a player in a parked level).
use super::{Game, commands::Target};
use horizon_client::mobs::Species;
use mod_api::{EntityRef, SoundSource};

/// `Monster`'s sound source is hostile, and so are a few mobs that are not
/// `Monster`s: `Hoglin`, `Ghast`, `Phantom`, `Shulker` and the slimes
/// (`AbstractCubeMob`).
pub(super) fn hostile_species(species: Species) -> bool {
    horizon_client::mobs::ai::is_monster(species)
        || matches!(
            species,
            Species::Hoglin
                | Species::Ghast
                | Species::Phantom
                | Species::Shulker
                | Species::Slime
                | Species::MagmaCube
        )
}

impl Game<'_> {
    /// The level in place holds the entity: events and game events are
    /// queued in it.
    pub(super) fn native_entity_here(&self, entity: EntityRef) -> bool {
        self.entity_level(entity)
            .is_some_and(|level| level.dimension() == self.native_level_key())
    }
    /// `Entity.getSoundSource`: the group an entity's sounds play under.
    /// Players are `Players`; `Monster`s, the dragon, hoglins, ghasts,
    /// phantoms, shulkers, slimes and a killer bunny are `Hostile`; dropped
    /// items and experience orbs `Ambient`; every other entity `Neutral`.
    pub(super) fn entity_sound_source(&self, entity: EntityRef) -> Option<SoundSource> {
        Some(match self.native_entity_target(entity)? {
            Target::Player | Target::Guest(_) => SoundSource::Players,
            Target::Mob(id) if self.ecs.is_active() => {
                let species = self.ecs_mob(id)?.species?;
                // The killer bunny is the rabbit whose coat is `Rabbit.Variant.EVIL` (the ECS rabbit has no coat component yet: 0).
                let evil = species == Species::Rabbit
                    && self.ecs.variant_of(id) == Some(horizon_client::mobs::rabbit::EVIL);
                if evil || hostile_species(species) { SoundSource::Hostile } else { SoundSource::Neutral }
            }
            Target::Mob(id) => {
                let m = self.remote_entities.iter().find(|m| m.id == id)?;
                let evil = m.species() == Species::Rabbit
                    && m.extra.variant == horizon_client::mobs::rabbit::EVIL;
                if evil || hostile_species(m.species()) {
                    SoundSource::Hostile
                } else {
                    SoundSource::Neutral
                }
            }
            Target::Item(_) | Target::Orb(_) => SoundSource::Ambient,
            Target::Dragon(_) => SoundSource::Hostile,
            t => match self.target_type(t).as_str() {
                "minecraft:shulker_bullet" => SoundSource::Hostile,
                "minecraft:lightning_bolt" => SoundSource::Weather,
                _ => SoundSource::Neutral,
            },
        })
    }
    /// `Entity.playSound(sound, volume, pitch)`: plays sound event `sound` at
    /// the entity under its sound source, for the players within range. A
    /// silent mob plays nothing, as vanilla's check is `isSilent`; a player
    /// (`Player.playSound`) doesn't hear it themself, their own client plays
    /// it. True means queued, not heard; false for an unknown `minecraft:`
    /// event or a volume or pitch that isn't a finite non-negative number.
    pub(super) fn entity_play_sound(
        &mut self,
        entity: EntityRef,
        sound: &str,
        volume: f32,
        pitch: f32,
    ) -> bool {
        let Some(target) = self.native_entity_target(entity) else {
            return false;
        };
        let (Some(level), Some(at), Some(source)) = (
            self.entity_level(entity),
            self.target_position(target),
            self.entity_sound_source(entity),
        ) else {
            return false;
        };
        let except = match target {
            Target::Player => self.lan_acting,
            Target::Guest(conn) => Some(conn),
            _ => {
                if self.entity_is_silent(entity) == Some(true) {
                    return false;
                }
                None
            }
        };
        self.native_queue_sound(level, sound, at, source, volume, pitch, except)
            .is_some()
    }
    /// `Entity.gameEvent(event)`: game event `event` (`minecraft:step`...)
    /// at the entity's position with the entity as its source, which sculk
    /// sensors, shriekers, wardens and allays in range hear. A player or mob
    /// is the event's source entity; other entities retain their own id.
    /// False for an event vanilla doesn't have, a dead guest and a guest while
    /// another one acts.
    pub(super) fn entity_game_event(&mut self, entity: EntityRef, event: &str) -> bool {
        let Some(target) = self.native_entity_target(entity) else {
            return false;
        };
        let Some(at) = self.target_position(target) else {
            return false;
        };
        let Some(level) = self.level(self.dim.id()) else {
            return false;
        };
        self.level_game_event(
            level,
            event,
            at,
            mod_api::GameEventContext {
                source_entity: Some(entity),
                affected_state: None,
            },
        )
    }
    /// `Level.broadcastEntityEvent(entity, id)`: sends entity event `id` (the
    /// byte `Entity.handleEntityEvent` reads: `2` is a hurt flash, `3` a
    /// death, `35` a totem...) to the players tracking the entity and to the
    /// entity itself. True means queued; false for an entity outside the
    /// level in place.
    pub(super) fn entity_broadcast_event(&mut self, entity: EntityRef, id: u8) -> bool {
        let Some(target) = self.native_entity_target(entity) else {
            return false;
        };
        let Some(at) = self.target_position(target) else {
            return false;
        };
        if !self.native_entity_here(entity) {
            return false;
        }
        let Ok(entity_id) = i32::try_from(entity.id()) else {
            return false;
        };
        self.lan_entity_event(entity_id, id as i8, at);
        true
    }
    /// `Entity.isSpectator`: true for a player in spectator mode, false for
    /// every other entity.
    pub(super) fn entity_spectator(&self, entity: EntityRef) -> Option<bool> {
        Some(match self.native_entity_target(entity)? {
            Target::Player => self.spectator(),
            Target::Guest(conn) => self.command_guest(conn)?.game_mode == 3,
            _ => false,
        })
    }
}
