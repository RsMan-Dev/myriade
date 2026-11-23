//! Block particles the server sends: the transform's level event.
use super::*;

impl Game<'_> {
    /// `BlockTransformer.TransformParticle.send`: its level event.
    pub(super) fn transform_particle(&mut self, particle: &str, pos: [i32; 3]) {
        let source = self
            .lan_acting_uuid
            .map(horizon_world::level::LevelEventPlayer);
        match particle {
            "scrape" => self.level_event_from(source, 3005, pos, 0),
            "wax_on" => self.level_event_from(source, 3003, pos, 0),
            "wax_off" => self.level_event_from(source, 3004, pos, 0),
            _ => {}
        }
    }
}
