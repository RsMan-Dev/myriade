//! Level sound requests on each logical side: server broadcast or client playback.

use std::borrow::Cow;

use myriade_types::SoundSource;

use super::Game;

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
/// waiting for the end of the tick to be sent.
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

/// A sound as it goes in the packet.
pub(super) struct HeardSound<'a> {
    pub name: Cow<'a, str>,
    pub source: SoundSource,
    pub volume: f32,
    pub pitch: f32,
}

impl Game<'_> {
    /// `ServerLevel.playSeededSound`'s entity overload, `playSound(except,
    /// emitter, ...)`: while the level's sounds are captured for others, `sound`
    /// joins the tick's bound sounds, sent after the tick's other sounds to the
    /// players around but the one on connection `except`.
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
    /// the level in place, but the player on connection `except`.
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

    /// The sound as vanilla's `ServerLevel.playSeededSound` asks for it, borrowed.
    pub(super) fn ask_level_sound<'a>(
        &mut self,
        _dimension: &str,
        sound: LevelSound<'a>,
    ) -> Option<HeardSound<'a>> {
        let LevelSound {
            name,
            source,
            volume,
            pitch,
            ..
        } = sound;
        Some(HeardSound {
            name: Cow::Borrowed(name),
            source,
            volume,
            pitch,
        })
    }
}
