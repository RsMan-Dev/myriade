//! The entity overload of level sounds, shared by mob and player call sites.
use super::Game;
use horizon_core::nbt::Tag;

fn saved_silent(extra: Option<&Tag>) -> bool {
    horizon_client::save::number(extra.and_then(|tag| tag.get("Silent"))).unwrap_or(0) != 0
}

impl Game<'_> {
    /// `Entity.isSilent` for the player this window plays: its saved `Silent`,
    /// which its sounds already read.
    pub(super) fn player_silent(&self) -> bool {
        saved_silent(self.level.player_extra.as_ref())
    }
    /// `Level.playSound(except, mob, ...)`: the category and actor exclusion
    /// belong to the call site, not to the event's name.
    pub(super) fn play_mob_bound_sound(
        &mut self,
        id: u32,
        event: &str,
        source: usize,
        volume: f32,
        pitch: f32,
        except_actor: bool,
    ) {
        self.ecs_mob_sound(id, event, source, volume, pitch, except_actor);
    }

    fn sound_player_id(&self) -> u32 {
        self.native_acting_entity()
    }

    /// The player in place can be a guest while `as_guest` runs. Its entity
    /// id and saved metadata travel with that action, even while the hub is out.
    pub(super) fn play_player_bound_sound(
        &mut self,
        event: &str,
        source: usize,
        volume: f32,
        pitch: f32,
        except_actor: bool,
    ) {
        let id = self.sound_player_id();
        let Some(position) = self.sound_entity_position(id) else {
            return;
        };
        self.audio.play_bound(
            event,
            source,
            volume,
            pitch,
            id,
            position,
            except_actor.then_some(id),
        );
    }

    pub(super) fn sound_entity_position(&self, id: u32) -> Option<[f64; 3]> {
        if id == self.sound_player_id()
            && let Some(player) = self.player.as_ref()
        {
            return Some([player.position.x, player.position.y, player.position.z]);
        }
        if let Some(guest) = self.lan.as_ref().and_then(|host| {
            host.hub
                .guests
                .iter()
                .find(|g| g.entity_id as u32 == id && g.placed && g.dimension == self.dim.id())
        }) {
            return Some(guest.position);
        }
        if let Some(p) = self.ecs.position(id) {
            return Some(p);
        }
        None
    }
}
