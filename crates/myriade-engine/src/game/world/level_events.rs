//! Level events: the server's `levelEvent` and `globalLevelEvent` senders. Each event is queued
//! for the guests that can see it; the sound and particles of an event are the clients' own.
use crate::game::Game;
use horizon_core::position::BlockPos;
use horizon_world::level::LevelEvent;

impl Game<'_> {
    pub(in crate::game) fn level_event_at(&mut self, id: i32, pos: [i32; 3], data: i32) {
        self.level_event_from(None, id, pos, data);
    }

    pub(in crate::game) fn level_event_from(
        &mut self,
        source: Option<horizon_world::level::LevelEventPlayer>,
        id: i32,
        pos: [i32; 3],
        data: i32,
    ) {
        self.level_events.push(LevelEvent::LevelEvent {
            id,
            pos: BlockPos::new(pos[0], pos[1], pos[2]),
            data,
            source,
        });
        let _ = self.process_level_events();
    }

    pub(in crate::game) fn global_level_event(&mut self, id: i32, pos: [i32; 3], data: i32) {
        // ServerLevel.globalLevelEvent: without global_sound_events it is a
        // plain level event (`levelEvent(null, ...)`), sent to the players
        // near `pos`.
        if !self
            .session
            .rules
            .bool_at(myriade_engine::gamerules::vanilla::GLOBAL_SOUND_EVENTS)
        {
            self.level_event_at(id, pos, data);
            return;
        }
        self.lan_global_events.push((self.dim.id(), id, pos, data));
    }
}
