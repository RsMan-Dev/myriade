//! `player_respawn_position` (NeoForge's `PlayerRespawnPositionEvent`): where
//! a respawning server player ends up. Vanilla finds the point inside
//! `PlayerList.respawn`, loading chunks as it goes. Horizon places the player
//! first and finds the point once its level has loaded around it (the respawn
//! and world-spawn trips of `server_travel.rs`), so the mods hear it there.
use super::respawn_anchor::RespawnConfig;
use super::server_travel::LevelTeleport;
use super::{Dim, Game};
use horizon_core::geometry::Vec3;

/// Where a respawn ends, as the server found it: a level, a point and the
/// player's facing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Respawn {
    pub dim: Dim,
    pub at: Vec3,
    pub yaw: f32,
    pub pitch: f32,
}

/// What the mods decided about a respawn.
#[derive(Clone, Copy, Debug)]
pub(super) struct RespawnDecision {
    /// Where the player goes: the found point, or the mods' own.
    pub to: Respawn,
    /// Whether that differs from the found point.
    pub moved: bool,
    /// Whether the fresh player keeps its respawn point
    /// (`copyOriginalSpawnPosition`).
    pub copy: bool,
}

impl Game<'_> {
    /// `PlayerRespawnPositionEvent`, once per respawn: guest `conn`
    /// respawns at `found`, final now. `missing` is the respawn point whose
    /// bed or anchor was gone (`TeleportTransition.missingRespawnBlock`).
    /// Every place that finishes a respawn asks here; None when no mod
    /// listens. A moved destination is not block-checked and doesn't fire
    /// again.
    pub(super) fn respawn_position(
        &mut self,
        _conn: u32,
        _found: Respawn,
        _missing: Option<RespawnConfig>,
    ) -> Option<RespawnDecision> {
        None
    }

    /// The deferred respawns' side of a decision: the found point's copy
    /// already happened (`copyRespawnPosition` for a found bed or anchor,
    /// the point forgotten for a missing one), so a mod's other choice
    /// forgets the point or gives the lost one back.
    pub(super) fn apply_respawn_copy(
        &mut self,
        conn: u32,
        decision: &RespawnDecision,
        missing: Option<RespawnConfig>,
    ) {
        if decision.copy == missing.is_none() {
            return;
        }
        let Some(host) = self.lan.as_mut() else {
            return;
        };
        let _ = host.hub.guests.update_connection(conn, |g| {
            g.spawn = None;
            g.respawn_elsewhere = missing.filter(|_| decision.copy).map(|c| c.saved());
        });
    }

    /// Sends a guest whose deferred respawn the mods moved into another
    /// level there: a level change, the one extra Respawn packet a moved
    /// respawn costs.
    pub(super) fn respawn_into_other_level(&mut self, conn: u32, to: Respawn) {
        self.level_teleports.push(LevelTeleport {
            conn,
            dim: to.dim,
            at: [to.at.x, to.at.y, to.at.z],
            rotation: Some([to.yaw, to.pitch]),
            vehicle: None,
        });
        self.run_level_teleports();
    }
}
