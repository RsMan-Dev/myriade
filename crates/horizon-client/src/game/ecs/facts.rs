//! The players as facts for the ECS: place and items (`Online`), life and targetability (`PlayerFacts`). A fact is written only
//! when it differs from the one the world holds; the cost follows the players, never the entities.
use super::super::{Game, Result};
use horizon_client::command::args::GameMode;
use horizon_client::items::ItemStack;
use horizon_client::lan::playerdata::uuid_ints;
use bevy_ecs::entity::Entity;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{
    bridge::{
        host::PlayerFact,
        players::PlayerFacts,
    },
    goals::{Online, Player},
    items::item_id,
};

/// Same place and same items: the test `Online` applies before it keeps the old player.
fn same_spot(a: &Player, b: &Player) -> bool {
    (a.x, a.y, a.z, a.main, a.off, a.head, a.chest, a.legs, a.feet) == (b.x, b.y, b.z, b.main, b.off, b.head, b.chest, b.legs, b.feet)
}

fn item(stack: Option<&ItemStack>) -> u32 {
    stack.map_or(0, |s| item_id(&s.item))
}

impl Game<'_> {
    /// Writes the facts of every player present, and removes the players that left. Reads the world's own copies to compare.
    pub(in crate::game) fn sync_player_facts(&mut self) -> Result<()> {
        let mut present = Vec::new();
        let conns: Vec<u32> = self.lan.as_ref().map_or_else(Vec::new, |h| h.hub.online_players().map(|g| g.conn).collect());
        if let Some(id) = self.player_entity_id_by_connection(None) {
            if let Some((p, fact)) = self.player_fact(id as u32) {
                let uuid = self.level.ensure_player_uuid();
                present.push(p.id);
                self.announce_player(p.id, uuid);
                self.set_fact_if_changed(p, fact);
            }
        }
        for conn in conns {
            let snap = self.as_guest(conn, |g| g.player_entity_id_by_connection(Some(conn)).and_then(|id| g.player_fact(id as u32)));
            if let Some(Some((p, fact))) = snap {
                present.push(p.id);
                if let Some(uuid) = self.lan.as_ref().and_then(|h| h.hub.guest(conn)).map(|g| uuid_ints(g.uuid)) {
                    self.announce_player(p.id, uuid);
                }
                self.set_fact_if_changed(p, fact);
            }
        }
        let gone: Vec<u32> = self
            .ecs
            .world()
            .and_then(|w| w.get_resource::<PlayerFacts>())
            .map_or_else(Vec::new, |f| f.0.keys().copied().filter(|k| !present.contains(k)).collect());
        for id in gone {
            self.ecs.remove_player(id);
            self.ecs.player_left(id);
        }
        Ok(())
    }

    /// Announces a player the ECS does not hold yet, with its uuid. Once its facts are written, it is known.
    fn announce_player(&mut self, id: u32, uuid: [i32; 4]) {
        let known = self.ecs.world().and_then(|w| w.get_resource::<PlayerFacts>()).is_some_and(|f| f.0.contains_key(&id));
        if !known {
            self.ecs.player_joined(id, uuid);
        }
    }

    /// The place, items and facts of the player `id` as this `Game` holds them (the host, or a guest swapped in).
    fn player_fact(&mut self, id: u32) -> Option<(Player, PlayerFact)> {
        let at: Vec3 = self.player.as_ref()?.position;
        let p = Player {
            id,
            x: at.x,
            y: at.y,
            z: at.z,
            main: item(self.held()),
            off: item(self.offhand.as_ref()),
            head: item(self.armor[0].as_ref()),
            chest: item(self.armor[1].as_ref()),
            legs: item(self.armor[2].as_ref()),
            feet: item(self.armor[3].as_ref()),
        };
        // A player who is dead or in creative or spectator is not a target. Its team is the board's (`Game::push_team_event`).
        let alive = !self.player.as_ref()?.health.dead_or_dying();
        let targetable = !matches!(self.mode, GameMode::Creative | GameMode::Spectator);
        Some((p, PlayerFact { alive, targetable, hero: self.hero_amplifier() }))
    }

    /// What a death reads of the player `player` that hurts `e`: its luck attribute and the stack in its main hand (Looting). Told with
    /// the hit, not with the facts of every synchronisation; the loot stack is made at the death.
    pub(in crate::game) fn ecs_note_killer(&mut self, e: Entity, player: u32) {
        let luck = self.player.as_mut().and_then(|pl| pl.attribute("minecraft:luck").ok()).unwrap_or(0.0) as f32;
        let weapon = self.held().cloned();
        self.ecs.note_killer(e, player, luck, weapon);
    }

    /// Calls the ECS only for a place or a fact that changed since the world last held it.
    fn set_fact_if_changed(&mut self, p: Player, fact: PlayerFact) {
        let Some(w) = self.ecs.world() else { return };
        let moved = w.get_resource::<Online>().and_then(|o| o.get(p.id)).is_none_or(|o| !same_spot(&o, &p));
        let changed = w.get_resource::<PlayerFacts>().and_then(|f| f.0.get(&p.id)) != Some(&fact);
        if moved || changed {
            self.ecs.set_player(p, fact);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::game::native_entity_refs::tests::with_game;
    use horizon_client::motion::PlayerMotion;
    use horizon_core::geometry::Vec3;
    use horizon_ecs_proto::bridge::{host::HostSetup, players::PlayerFacts};

    #[test]
    #[ignore = "daily"]
    fn a_present_player_is_announced_once_and_left_when_gone() {
        with_game(|game| {
            game.ecs = {
                game.test_ecs_level()
            };
            game.player = Some(PlayerMotion::new(game.motion, Vec3::new(0.5, 64.0, 0.5)).unwrap());
            let facts = |game: &crate::game::Game<'_>| game.ecs.world().and_then(|w| w.get_resource::<PlayerFacts>()).map_or(0, |f| f.0.len());
            game.sync_player_facts().unwrap();
            assert_eq!(facts(game), 1);
            game.sync_player_facts().unwrap();
            assert_eq!(facts(game), 1, "a second sync keeps the one player");
            game.player = None;
            game.sync_player_facts().unwrap();
            assert_eq!(facts(game), 0, "the player left");
        });
    }
}
