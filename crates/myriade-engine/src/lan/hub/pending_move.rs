//! Commits a validated movement report to the player's state.

use super::{HostEvent, Hub};

/// How a validated report is committed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hearing {
    /// A report from the player's own movement.
    Heard,
    /// The confirmation of a teleport: validated and committed in place. The last reported
    /// location stays the teleport's destination.
    Confirmation,
}

/// A movement report that passed the server's own checks (`handlePlayerPositionChange`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Accepted {
    pub to: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub flags: u8,
    /// The box rests on something at `to`.
    pub stands: bool,
    /// `clientIsFloating`, from this report.
    pub floating: bool,
    /// The packet's own on-ground flag, as the client sent it: `flags` may have lost the bit to
    /// the anticheat's ground check, this keeps the claim.
    pub on_ground: bool,
    /// The packet carried a position (`MovePlayerPos`, `MovePlayerPosRot`).
    pub has_position: bool,
    /// The packet carried a rotation (`MovePlayerRot`, `MovePlayerPosRot`).
    pub has_rotation: bool,
}

impl Hub {
    /// Commits the validated report `m` of guest `i`. A passenger never gets here (its vehicle
    /// carries it).
    pub(super) fn accept_move(
        &mut self,
        i: usize,
        m: Accepted,
        hearing: Hearing,
        events: &mut Vec<HostEvent>,
    ) {
        self.commit_move(i, m, events);
        // Committed in place: the baseline follows it, except on a teleport's confirmation, which
        // leaves it at the teleport's destination.
        if hearing != Hearing::Confirmation {
            let seen = (m.to, [m.yaw, m.pitch.clamp(-90.0, 90.0)]);
            self.guests.update(i, |g| g.last_reported = seen);
        }
    }

    /// `handlePlayerPositionChange`'s commit: the report becomes the player's state.
    fn commit_move(&mut self, i: usize, m: Accepted, events: &mut Vec<HostEvent>) {
        let Accepted {
            to,
            yaw,
            pitch,
            flags,
            stands,
            floating,
            ..
        } = m;
        self.guests.update(i, |g| {
            g.client_is_floating = floating;
            // Entity.move's verticalCollisionBelow, which the check above reads of the last move.
            g.rests_on_ground = stands;
            g.accept_move(to);
            // ServerChunkCache.move: its pairs are checked again.
            g.moved = true;
            let moved = [0, 1, 2].map(|k| g.position[k] - g.previous_position[k]);
            if g.moves.is_empty() {
                g.moves_from_ground = g.on_ground;
            }
            let on_ground = flags & 1 != 0;
            if g.on_ground && !on_ground && moved[1] > 0.0 && !g.spectator() {
                events.push(HostEvent::Jump {
                    conn: g.conn,
                    from: g.previous_position,
                    yaw: g.yaw,
                });
            }
            g.moves.push((moved, on_ground));
            g.known_movement = moved;
            g.yaw = yaw;
            g.pitch = pitch.clamp(-90.0, 90.0);
            g.head_yaw = g.yaw;
            g.on_ground = flags & 1 != 0;
            g.ticks = g.ticks.max(1);
        });
    }
}
