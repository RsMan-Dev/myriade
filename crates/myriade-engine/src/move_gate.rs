//! The client door of the move gate (arismoko/horizon#2414): the one function that arms a
//! physical entity move, and the only code allowed to reach the collide
//! entries. A family that resolves its own movement goes through a [`MoveGate`]
//! here; `clippy.toml` bans calling the `resolve*` entries anywhere else, so a
//! forgotten family cannot compile around the gate.
//!
//! [`arm`] is where a physical move is armed, on the authoritative side.
use crate::collisions::{EntityCollisions, SceneCollisions};
use horizon_core::geometry::{Aabb, Vec3, border::BorderGeometry};
use horizon_world::block_collision_scan::BlockCollisionWorld;
use horizon_world::collision_context::CollisionContext;
use horizon_world::entity_move::MoverType;
use horizon_world::move_gate::MoveGate;
use horizon_world::world_movement::EntityMovementWorld;
use std::io;

/// Arm one physical `Entity.move` invocation: the entity is about to move by
/// `delta` for `cause`'s sake. The requested delta is recorded unchanged.
#[inline]
pub fn arm(gate: &mut MoveGate, cause: MoverType, delta: Vec3) {
    gate.arm(cause, delta);
}

/// [`arm`] for a fresh gate, in one line at a move site.
#[inline]
pub fn armed(cause: MoverType, delta: Vec3) -> MoveGate {
    let mut gate = MoveGate::new();
    arm(&mut gate, cause, delta);
    gate
}

/// The collide entries: the move gate's own (arismoko/horizon#2414). The `clippy.toml` ban
/// targets their callers; these wrappers are the one caller-side exception,
/// and everything else resolves through them.
#[allow(clippy::disallowed_methods)]
mod collide {
    use super::*;
    /// `SceneCollisions::resolve` through the gate: the armed move's
    /// collision, stopped by the blocks (and powder snow by the scene's
    /// `subject`).
    pub fn scene(
        gate: &mut MoveGate,
        scene: &mut SceneCollisions<'_, '_>,
        bounds: Aabb,
        movement: Vec3,
        max_step: f32,
        on_ground: bool,
    ) -> crate::Result<Vec3> {
        let _ = gate;
        scene.resolve(bounds, movement, max_step, on_ground)
    }
    /// `SceneCollisions::resolve_with_context` through the gate.
    pub fn scene_with_context(
        gate: &mut MoveGate,
        scene: &mut SceneCollisions<'_, '_>,
        bounds: Aabb,
        movement: Vec3,
        max_step: f32,
        on_ground: bool,
        context: CollisionContext<'_>,
    ) -> crate::Result<Vec3> {
        let _ = gate;
        scene.resolve_with_context(bounds, movement, max_step, on_ground, context)
    }
    /// `SceneCollisions::resolve_in_border_with` through the gate: also
    /// stopped by the world border's walls and the selected `entities` boxes.
    /// The scene is consumed, as the entry it wraps consumes it.
    pub fn scene_in_border_with(
        gate: &mut MoveGate,
        scene: SceneCollisions<'_, '_>,
        bounds: Aabb,
        movement: Vec3,
        max_step: f32,
        on_ground: bool,
        border: Option<(&BorderGeometry, [f64; 2])>,
        entities: &[Aabb],
    ) -> crate::Result<Vec3> {
        let _ = gate;
        scene.resolve_in_border_with(bounds, movement, max_step, on_ground, border, entities)
    }
    /// `EntityCollisions::resolve` through the gate.
    pub fn entities<W>(
        gate: &mut MoveGate,
        collisions: &mut EntityCollisions<'_, W>,
        bounds: Aabb,
        movement: Vec3,
        max_step: f32,
        on_ground: bool,
    ) -> crate::Result<Vec3>
    where
        W: BlockCollisionWorld<Error = io::Error> + EntityMovementWorld<Error = io::Error>,
    {
        let _ = gate;
        collisions.resolve(bounds, movement, max_step, on_ground)
    }
    /// `EntityCollisions::resolve_with_context` through the gate.
    pub fn entities_with_context<W>(
        gate: &mut MoveGate,
        collisions: &mut EntityCollisions<'_, W>,
        bounds: Aabb,
        movement: Vec3,
        max_step: f32,
        on_ground: bool,
        context: CollisionContext<'_>,
    ) -> crate::Result<Vec3>
    where
        W: BlockCollisionWorld<Error = io::Error> + EntityMovementWorld<Error = io::Error>,
    {
        let _ = gate;
        collisions.resolve_with_context(bounds, movement, max_step, on_ground, context)
    }
}
pub use collide::{
    entities as entities_resolve, entities_with_context as entities_resolve_with_context,
    scene as scene_resolve, scene_in_border_with as scene_resolve_in_border_with,
    scene_with_context as scene_resolve_with_context,
};
