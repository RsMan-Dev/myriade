//! The game layer's door of the move gate (arismoko/horizon#2414): who is moving, and the one
//! `Game::entity_move` the pushers and family ticks arm a move through. PR 1a
//! carries no hook — every decision is `Run` and the tick cost is one branch;
//! PR 1b fires the `entity_move` hook inside the client `move_gate::arm` this
//! door calls, with a revision recheck before the kind's physics runs.
use super::Game;
use super::entity::EntityHandle;
use horizon_core::geometry::Vec3;
use horizon_world::entity_move::MoverType;
use horizon_world::move_gate::MoveGate;

/// Which body a move is for: the local player window, a LAN guest, or a listed
/// entity. The piston and shulker-box pushes and the family ticks name their
/// mover with it.
#[derive(Clone, Copy, Hash, PartialEq, Eq)]
pub(in crate::game) enum MoveWho {
    Player,
    Guest(u32),
    Entity(EntityHandle),
    /// An entity of P's ECS, by its network id.
    Ecs(u32),
}

impl Game<'_> {
    /// Arm one physical `Entity.move` invocation (arismoko/horizon#2414) for `who` with `cause`
    /// and hand back its gate: the caller then runs the kind's physics with
    /// the movement the gate decided (`delta`), and skips it when the gate
    /// says so. A pusher arms before its own restrictions — the piston limit
    /// included — so a cancelled move spends none of them.
    pub(in crate::game) fn entity_move(&mut self, _who: MoveWho, cause: MoverType, delta: Vec3) -> MoveGate {
        myriade_engine::move_gate::armed(cause, delta)
    }
}
