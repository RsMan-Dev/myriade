//! The scoreboard's teams as alliances for the ECS (greffe lot 38). The board sends a team's members when the team changes
//! (`push_team_event`), and the level keeps them; nothing reads the board per tick.
use super::super::Game;
use horizon_ecs_proto::targeting::Who;

impl Game<'_> {
    /// Sends the members of the board's team `team` to the ECS: the players by id, the other entities by their ECS entity (an entity
    /// that has no id yet is not a member). A team that is gone clears its alliance.
    pub(in crate::game) fn push_team_event(&mut self, team: &str) {
        let mut players: Vec<(u32, String)> = Vec::new();
        if let (Some(id), Some(name)) = (self.player_entity_id_by_connection(None), self.lan_acting_name.clone()) {
            players.push((id as u32, name));
        }
        let conns: Vec<u32> = self.lan.as_ref().map_or_else(Vec::new, |h| h.hub.online_players().map(|g| g.conn).collect());
        for conn in conns {
            let guest = self.as_guest(conn, |g| Some((g.player_entity_id_by_connection(Some(conn))? as u32, g.lan_acting_name.clone()?)));
            players.extend(guest.flatten());
        }
        let Some(t) = self.score.board.team(team) else {
            self.ecs.clear_team(team);
            return;
        };
        let mut members: Vec<Who> = players.into_iter().filter(|(_, name)| t.players.contains(name)).map(|(id, _)| Who::Player(id)).collect();
        for (id, uuid) in self.persist.uuids.iter() {
            if t.players.contains(&self.scoreboard_name(*id, *uuid)) {
                members.extend(self.ecs.entity_of(*id).map(Who::Entity));
            }
        }
        self.ecs.set_team(team, &members);
    }
}
