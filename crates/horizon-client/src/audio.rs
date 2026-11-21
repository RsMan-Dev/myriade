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
