//! The level sounds the server keeps for its network broadcaster.

/// A level sound retained for the network broadcaster, in playback order.
#[derive(Clone, Debug)]
pub struct CapturedSound {
    pub event: String,
    pub position: [f64; 3],
    pub volume: f32,
    pub pitch: f32,
    pub source: usize,
    pub entity: Option<u32>,
    pub except_entity: Option<u32>,
}

/// The server's level sound channel: every positioned sound is kept in `capture` for the
/// network broadcaster. Nothing is played here.
#[derive(Default)]
pub struct Audio {
    pub capture: Option<Vec<CapturedSound>>,
    source_override: Option<usize>,
}

impl Audio {
    pub fn play(&mut self, event: &str, at: Option<[f64; 3]>, volume: f32, pitch: f32) {
        let (Some(capture), Some(position)) = (self.capture.as_mut(), at) else {
            return;
        };
        let source = self.source_override.unwrap_or_else(|| {
            myriade_data::sound_source::sound_source(
                &myriade_level::sound_events::normalize(event),
            )
        });
        capture.push(CapturedSound {
            event: event.to_owned(),
            position,
            volume,
            pitch,
            source,
            entity: None,
            except_entity: None,
        });
    }
    /// `play` on the sound source `source` instead of the event's own.
    pub fn play_on(&mut self, source: usize, event: &str, at: Option<[f64; 3]>, volume: f32, pitch: f32) {
        self.source_override = Some(source);
        self.play(event, at, volume, pitch);
        self.source_override = None;
    }
    /// A sound that follows the entity `entity`, heard by all but `except_entity`.
    #[allow(clippy::too_many_arguments)]
    pub fn play_bound(
        &mut self,
        event: &str,
        source: usize,
        volume: f32,
        pitch: f32,
        entity: u32,
        position: [f64; 3],
        except_entity: Option<u32>,
    ) {
        if let Some(capture) = self.capture.as_mut() {
            capture.push(CapturedSound {
                event: event.to_owned(),
                position,
                volume,
                pitch,
                source,
                entity: Some(entity),
                except_entity,
            });
        }
    }
}
