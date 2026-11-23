//! Mobs and the guests of an open world. The mob code sees one player
//! (`MobWorld.player`, `Who::Player`); with guests on, each mob sees the
//! player vanilla's `Level.getNearestPlayer` would give it, and keeps the
//! guest it targets until it lets go. What it then does to `Who::Player`
//! (hits, pushes, fire, its projectiles) runs with that guest swapped in.
use super::Game;
use horizon_client::mobs::{MobEvent, PlayerView, Who};
use horizon_core::geometry::{Aabb, Vec3};
use horizon_protocol::packets::play::{self, Clientbound as C};

/// The events of a mob's tick that act on the player it sees.
pub(super) fn acts_on_player(e: &MobEvent) -> bool {
    matches!(
        e,
        MobEvent::AttackPlayer(..)
            | MobEvent::CubeHurtPlayer { .. }
            | MobEvent::AttackPlayerWithEffect { .. }
            | MobEvent::Sting {
                target: Who::Player,
                ..
            }
            | MobEvent::DamagePlayer { .. }
            | MobEvent::SonicBoom {
                target: Who::Player,
                ..
            }
            | MobEvent::Push {
                target: Who::Player,
                ..
            }
            | MobEvent::Knockback {
                target: Who::Player,
                ..
            }
            | MobEvent::StabDismount {
                target: Who::Player,
            }
            | MobEvent::Ignite(Who::Player, _)
            | MobEvent::ZombieIgnite(Who::Player, _)
            | MobEvent::IronGolemHit {
                target: Who::Player,
                ..
            }
            | MobEvent::EffectPlayer { .. }
            | MobEvent::AxolotlSupportPlayer { .. }
            | MobEvent::ParrotToShoulder { .. }
    )
}

impl Game<'_> {
    /// The guest `conn`'s real identity — the uuid its lent view carries and
    /// its entity id — when the guest is a player the tick can name: a fake
    /// guest resolves as none, as the write handle refuses it (arismoko/horizon#1077).
    pub(super) fn guest_identity(&self, conn: u32) -> Option<([i32; 4], u32)> {
        let guest = self
            .lan
            .as_ref()?
            .hub
            .guests
            .iter()
            .find(|g| g.conn == conn)?;
        if guest.is_fake() {
            return None;
        }
        let uuid = horizon_client::lan::playerdata::uuid_ints(guest.uuid);
        let id = u32::try_from(guest.entity_id).ok()?;
        Some((uuid, id))
    }

    /// [`Self::each_guest_hit`] leaving out the guest `except` (the one a
    /// firework rocket is attached to, which it hurts on its own).
    pub(super) fn each_guest_hit_except(
        &mut self,
        except: Option<u32>,
        mut f: impl FnMut(&mut Self),
    ) {
        let Some(host) = self.lan.as_ref() else {
            return;
        };
        let conns: Vec<u32> = host
            .hub
            .guests
            .iter()
            .filter(|g| host.hub.here(g) && g.loaded && !g.dead && Some(g.conn) != except)
            .map(|g| g.conn)
            .collect();
        for conn in conns {
            self.guest_hit_by(conn, &mut f);
        }
    }

    /// Mob events aimed at a guest, run as that guest (`guest_hit_by`).
    pub(super) fn lan_guest_mob_events(&mut self, events: &mut Vec<(u32, u32, MobEvent)>) {
        if events.is_empty() {
            return;
        }
        for (conn, mob, e) in super::creatures::RemainingActions::new(events) {
            self.guest_hit_by(conn, |g| g.guest_mob_event(mob, e));
        }
    }

    /// One mob event, with the guest swapped in. A hit knows the mob that
    /// swung, so nothing guesses it from where the hit came from.
    pub(super) fn guest_mob_event(&mut self, mob: u32, e: MobEvent) {
        if matches!(
            e,
            MobEvent::AttackPlayer(..)
                | MobEvent::AttackPlayerWithEffect { .. }
                | MobEvent::Sting {
                    target: Who::Player,
                    ..
                }
        ) {
            self.combat.melee_mob = Some(mob);
        }
        let _ = self.handle_mob_events(vec![e]);
        // Used up by the hit; a blow that hurt nobody leaves it to no other.
        self.combat.melee_mob = None;
    }

    /// The guests' boxes for the blocks entities trigger (pressure plates,
    /// tripwires, detector rails): `Entity.checkInsideBlocks` and
    /// `Level.getEntities` see every player in the level, but not spectators
    /// (`Entity.isIgnoringBlockTriggers`).
    pub(super) fn lan_block_trigger_boxes(&self) -> Vec<([f64; 3], [f64; 3])> {
        let Some(host) = self.lan.as_ref() else {
            return Vec::new();
        };
        host.hub
            .guests
            .simulation_iter()
            .filter(|g| host.hub.here(g) && g.loaded && !g.dead && !g.spectator())
            .map(|g| {
                let b = g.bounds();
                let (lo, hi) = (b.min_position(), b.max_position());
                ([lo.x, lo.y, lo.z], [hi.x, hi.y, hi.z])
            })
            .collect()
    }
    /// The same guests' feet and whether they stand on the ground, for the
    /// blocks that also ask that (`BigDripleafBlock.canEntityTilt`).
    pub(super) fn lan_guest_feet(&self) -> Vec<([f64; 3], bool)> {
        let Some(host) = self.lan.as_ref() else {
            return Vec::new();
        };
        host.hub
            .guests
            .iter()
            .filter(|g| host.hub.here(g) && g.loaded && !g.dead && !g.spectator())
            .map(|g| (g.position, g.on_ground))
            .collect()
    }

    /// The guests a projectile can hit, under their entity ids.
    pub(super) fn lan_projectile_targets(&self) -> Vec<(Who, Aabb)> {
        let Some(host) = self.lan.as_ref() else {
            return Vec::new();
        };
        host.hub
            .guests
            .iter()
            .filter(|g| host.hub.here(g) && g.loaded && !g.dead && !g.spectator())
            .map(|g| (Who::Mob(g.entity_id as u32), guest_bounds(g)))
            .collect()
    }

}

impl Game<'_> {
    /// Hurt a guest: `f` runs as them, and the knockback it gave reaches
    /// their client. A hurt marks the velocity (Entity.markHurt), which goes
    /// out once the guest's server copy has travelled (`guest_server_tick`);
    /// a push with no hurt goes out at once.
    pub(super) fn guest_hit_by<R>(
        &mut self,
        conn: u32,
        f: impl FnOnce(&mut Self) -> R,
    ) -> Option<R> {
        let (result, pushed) = self.as_guest(conn, |g| {
            let before = g.player.as_ref().map(|p| p.velocity);
            let result = f(g);
            let pushed = g
                .player
                .as_ref()
                .filter(|p| !p.health.sync_velocity && Some(p.velocity) != before)
                .map(|p| p.velocity);
            (result, pushed)
        })?;
        self.send_guest_motion(conn, pushed);
        Some(result)
    }

    /// A player's melee hit on a guest (Player.causeExtraKnockback on a
    /// ServerPlayer): a hurt that marked the velocity sends it at once, and
    /// the server's copy goes back to the velocity it had before the hit.
    pub(super) fn guest_struck_by<R>(
        &mut self,
        conn: u32,
        f: impl FnOnce(&mut Self) -> R,
    ) -> Option<R> {
        let (result, sent) = self.as_guest(conn, |g| {
            let before = g.player.as_ref().map(|p| p.velocity);
            let result = f(g);
            let sent = g.player.as_mut().zip(before).and_then(|(p, before)| {
                std::mem::take(&mut p.health.sync_velocity)
                    .then(|| std::mem::replace(&mut p.velocity, before))
            });
            (result, sent)
        })?;
        self.send_guest_motion(conn, sent);
        Some(result)
    }

    /// ClientboundSetEntityMotionPacket to a guest about themselves.
    pub(super) fn send_guest_motion(&self, conn: u32, velocity: Option<Vec3>) {
        if let (Some(v), Some(g)) = (
            velocity,
            self.lan.as_ref().and_then(|host| host.hub.guest(conn)),
        ) {
            g.send(&C::SetEntityMotion(play::SetEntityMotion {
                id: g.entity_id,
                velocity: [v.x, v.y, v.z],
            }));
        }
    }
}

/// A guest as the blocks that look for players see them
/// (`PlayerDetector`, `level.getPlayers`: the dead ones too).
pub(super) struct Present {
    pub uuid: [i32; 4],
    pub position: [f64; 3],
    pub eye: [f64; 3],
    pub height: f32,
    pub creative: bool,
    pub spectator: bool,
    pub alive: bool,
    /// Trial Omen, else Bad Omen, among the guest's effects.
    pub omen: Option<horizon_client::mobs::trial_spawner::Omen>,
}

impl Game<'_> {
    /// Every guest in the world (`ServerLevel.getPlayers` past the host).
    pub(super) fn lan_present(&self) -> Vec<Present> {
        let guests = self
            .lan
            .iter()
            .flat_map(|h| h.hub.guests.simulation_iter().filter(|g| h.hub.here(g)));
        guests
            .filter(|g| g.loaded)
            .map(|g| {
                let [x, y, z] = g.position;
                // The acting guest's effects are on the game while their turn runs.
                let effects = if self.lan_acting == Some(g.conn) {
                    &self.potions.effects
                } else {
                    &g.effects
                };
                Present {
                    uuid: horizon_client::lan::playerdata::uuid_ints(g.uuid),
                    position: g.position,
                    eye: [x, y + g.eye_height(), z],
                    height: g.dimensions().height,
                    creative: g.creative(),
                    spectator: g.spectator(),
                    alive: !g.dead,
                    omen: omen_of(effects),
                }
            })
            .collect()
    }
    /// The UUID of the player acting: the guest `as_guest` runs, else the host.
    pub(super) fn acting_uuid(&self) -> [i32; 4] {
        self.lan_acting_uuid
            .map(horizon_client::lan::playerdata::uuid_ints)
            .or(self.level.player_uuid)
            .unwrap_or_default()
    }
}

/// `TrialSpawner.findPlayerWithOminousEffect`: Trial Omen first, then Bad Omen.
pub(super) fn omen_of(
    effects: &horizon_client::effects::PlayerEffects,
) -> Option<horizon_client::mobs::trial_spawner::Omen> {
    use horizon_client::mobs::trial_spawner::Omen;
    if effects.get("trial_omen").is_some() {
        Some(Omen::Trial)
    } else {
        effects.get("bad_omen").map(|_| Omen::Bad)
    }
}

/// A guest's bounding box, by its pose (`Avatar.POSES`).
pub(super) fn guest_bounds(g: &horizon_client::lan::guest::Guest) -> Aabb {
    g.bounds()
}
