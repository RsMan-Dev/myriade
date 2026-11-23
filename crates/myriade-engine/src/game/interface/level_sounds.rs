//! Level sound requests on each logical side: the mods' `PlayLevelSoundEvent`,
//! then server broadcast or client playback.

use std::borrow::Cow;

use mod_api::HookId;
use myriade_types::Identifier;
use myriade_types::SoundSource;

use super::Game;
use super::native_hooks::level_sound_playable;

/// A sound the server's level is about to play, as vanilla asked for it.
pub(super) struct LevelSound<'a> {
    pub name: &'a str,
    pub at: [f64; 3],
    pub volume: f32,
    pub pitch: f32,
    pub source: SoundSource,
    /// The entity vanilla binds the sound to (`playSound(except, sourceEntity, ...)`);
    /// none for a sound at a position, as `Entity.playSound`'s.
    pub emitter: Option<u32>,
}

impl<'a> LevelSound<'a> {
    /// A sound at `at` in the source its event name belongs to.
    pub fn new(name: &'a str, at: [f64; 3], volume: f32, pitch: f32) -> Self {
        let source = SoundSource::ALL
            .get(myriade_data::sound_source::sound_source(name))
            .copied()
            .unwrap_or(SoundSource::Master);
        Self {
            name,
            at,
            volume,
            pitch,
            source,
            emitter: None,
        }
    }

    pub fn in_source(self, source: SoundSource) -> Self {
        Self { source, ..self }
    }
}

/// A sound bound to an entity (`level.playSound(except, entity, ...)`),
/// waiting for the end of the tick to be asked of the mods and sent.
pub(super) struct BoundSound {
    pub name: String,
    pub at: [f64; 3],
    pub volume: f32,
    pub pitch: f32,
    pub source: SoundSource,
    pub emitter: u32,
    /// The connection of the player vanilla leaves out, who played it already.
    pub except: Option<u32>,
}

/// A sound after the mods: what goes in the packet.
pub(super) struct HeardSound<'a> {
    pub name: Cow<'a, str>,
    pub source: SoundSource,
    pub volume: f32,
    pub pitch: f32,
}

/// Whether a mod listens to either level sound hook.
fn listened() -> bool {
    false
        || false
}

impl Game<'_> {

    /// `ServerLevel.playSeededSound`'s entity overload, `playSound(except,
    /// emitter, ...)`: while the level's sounds are captured for others, `sound`
    /// joins the tick's bound sounds. `lan_broadcast_effects` asks
    /// them of the mods (`play_level_sound_at_entity`) after the tick's
    /// other sounds, and sends them to the players around but the one on
    /// connection `except`.
    pub(super) fn play_bound_sound(
        &mut self,
        sound: LevelSound<'_>,
        emitter: u32,
        except: Option<u32>,
    ) {
        // Kept while the level's sounds are captured for others, as
        // `Audio.play` keeps the rest.
        if self.audio.capture.is_some() {
            self.bound_sounds.push(BoundSound {
                name: sound.name.to_owned(),
                at: sound.at,
                volume: sound.volume,
                pitch: sound.pitch,
                source: sound.source,
                emitter,
                except,
            });
        }
    }

    /// The tick's bound sounds, in the order they played, for the players.
    pub(super) fn send_bound_sounds(&mut self) {
        for s in std::mem::take(&mut self.bound_sounds) {
            let sound = LevelSound {
                name: &s.name,
                at: s.at,
                volume: s.volume,
                pitch: s.pitch,
                source: s.source,
                emitter: Some(s.emitter),
            };
            self.send_level_sound(sound, s.except);
        }
    }

    /// `Level.playSound`'s packet for the players near the sound's position in
    /// the level in place, but the player on connection `except`, once the
    /// mods allowed it.
    pub(super) fn send_level_sound(&mut self, sound: LevelSound<'_>, except: Option<u32>) {
        let at = sound.at;
        let dimension = self.dim.id();
        let Some(heard) = self.ask_level_sound(dimension, sound) else {
            return;
        };
        if let Some(host) = self.lan.as_ref() {
            host.hub.sound(
                &heard.name,
                at,
                heard.volume,
                heard.pitch,
                heard.source as i32,
                except,
            );
        }
    }

    /// `ServerLevel.playSeededSound`'s `EventHooks.onPlaySoundAtEntity` or
    /// `onPlaySoundAtPosition`: `play_level_sound_at_entity` when the sound's
    /// emitter is an entity a mod can name, `play_level_sound_at_position`
    /// otherwise. None when a mod cancels it or leaves it unplayable (a volume
    /// or pitch that isn't finite and at least 0). With no listener, the sound
    /// as asked, borrowed.
    pub(super) fn ask_level_sound<'a>(
        &mut self,
        dimension: &str,
        sound: LevelSound<'a>,
    ) -> Option<HeardSound<'a>> {
        let LevelSound {
            name,
            at,
            volume,
            pitch,
            source,
            emitter,
        } = sound;
        let asked = HeardSound {
            name: Cow::Borrowed(name),
            source,
            volume,
            pitch,
        };
        if !listened() {
            return Some(asked);
        }
        // The mods see a namespaced id (`minecraft:entity.pig.ambient`).
        let Ok(mut id) = Identifier::parse(name) else {
            return Some(asked);
        };
        let (mut source_now, mut volume_now, mut pitch_now, mut cancel) =
            (source, volume, pitch, false);
        let entity = emitter.and_then(|id| self.entity(id));
        match entity {
            Some(entity) => self.native_play_level_sound_at_entity(
                mod_api::__private::PlayLevelSoundAtEntityCall {
                    entity,
                    sound: &mut id,
                    source: &mut source_now,
                    volume: &mut volume_now,
                    pitch: &mut pitch_now,
                    original_volume: volume,
                    original_pitch: pitch,
                    cancel: &mut cancel,
                },
            ),
            None => {
                let level = self.level(dimension)?;
                self.native_play_level_sound_at_position(
                    mod_api::__private::PlayLevelSoundAtPositionCall {
                        level,
                        position: at,
                        sound: &mut id,
                        source: &mut source_now,
                        volume: &mut volume_now,
                        pitch: &mut pitch_now,
                        original_volume: volume,
                        original_pitch: pitch,
                        cancel: &mut cancel,
                    },
                );
            }
        }
        if cancel || !level_sound_playable(at, volume_now, pitch_now) {
            return None;
        }
        Some(HeardSound {
            name: Cow::Owned(id.to_string()),
            source: source_now,
            volume: volume_now,
            pitch: pitch_now,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::lan::LanSettings;
    use super::super::native_entity_refs::tests::with_game;
    use super::super::native_sound_refs::tests::{guest, packets};
    use myriade_engine::lan::{NetConfig, hub::HubConfig};
    use horizon_protocol::packets::play::Clientbound;
    use mod_api::HostSide;
    use myriade_types::SoundSource;
    use std::sync::{Arc, Mutex};

    /// Quietens and moves the locked chest, cancels the pig, and turns a
    /// sound bound to an entity into a cow's; logs what it was asked.
    struct Sounds(Arc<Mutex<Vec<(&'static str, String, Option<u32>)>>>);
    #[mod_api::__subscriptions]
    impl mod_api::Hooks for Sounds {
        fn play_level_sound_at_position(
            &self,
            host: HostSide<'_>,
            e: &mut mod_api::PlayLevelSoundAtPositionEvent,
        ) -> mod_api::Flow {
            assert!(matches!(host, HostSide::Server(_)));
            self.0
                .lock()
                .unwrap()
                .push(("position", e.sound.to_string(), None));
            match e.sound.to_string().as_str() {
                "minecraft:block.chest.locked" => {
                    assert_eq!((e.original_volume, e.original_pitch), (1.0, 1.0));
                    (e.volume, e.pitch, *e.source) = (0.25, 2.0, SoundSource::Ambient);
                    mod_api::Flow::Continue
                }
                "minecraft:entity.pig.ambient" => mod_api::Flow::Cancel,
                _ => mod_api::Flow::Continue,
            }
        }
        fn play_level_sound_at_entity(
            &self,
            host: HostSide<'_>,
            e: &mut mod_api::PlayLevelSoundAtEntityEvent,
        ) -> mod_api::Flow {
            assert!(matches!(host, HostSide::Server(_)));
            self.0
                .lock()
                .unwrap()
                .push(("entity", e.sound.to_string(), Some(e.entity.id())));
            *e.sound = myriade_types::Identifier::parse("minecraft:entity.cow.ambient").unwrap();
            mod_api::Flow::Continue
        }
    }

    /// Opens the world to the network in place, with no one joined yet.
    fn host(game: &mut super::Game<'_>) {
        let net = NetConfig {
            listen: false,
            announce: false,
            lod_distance: 0,
            ..NetConfig::lan(0, String::new(), 3)
        };
        let config = HubConfig {
            default_game_mode: 0,
            guest_commands: false,
            force_game_mode: false,
            hardcore: false,
            view_distance: 2,
            simulation_distance: 2,
            dimension: game.dim.id().to_owned(),
            allow_flight: true,
            seed: 0,
            max_players: 3,
            levels: game.level_shapes,
            level_seeds: game.levels.runtime.seeds(),
        };
        let settings = LanSettings {
            port: 0,
            guest_commands: false,
            force_game_mode: false,
        };
        game.lan_host(settings, net, config).unwrap();
    }
}
