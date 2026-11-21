//! The archetype's physics of a sulfur cube (`SulfurCube` attributes `minecraft:bounciness`, `minecraft:friction_modifier`,
//! `minecraft:air_drag_modifier`, `sulfur_cube.rs` 297-335): while it holds a block, its horizontal speed is multiplied by the
//! friction modifier on the ground and by the air drag modifier in the air, and a landing gives back the bounciness part of its fall.
//! The tick is the cube's own; the modifiers are the archetype's, read each tick (no state kept).
//! DOUTE : LA FORMULE N'EST PAS VÉRIFIÉE : la source décompilée de vanille n'est pas dans le worktree (ni `.local/`), le cadre n'a pas
//! encore de traînée de vie (seul `cow.rs` 2094-2160 traite les objets). Ici : multiplicateurs bruts, sans le 0,91 ni le 0,6 vanilles.
//! DOUTE : le rebond est pris comme `-vy * bounciness` au premier tick au sol après une chute ; le seuil vanille n'est pas connu ici.
use super::swallow_hold::Held;
use super::*;
use crate::cow::*;
use crate::goals::*;

#[derive(Clone, Copy, Debug)]
pub struct Params;
impl Params {
    pub const DEFAULT: Self = Self;
}

/// Whether the cube was in the air at the last tick (a landing gives back the bounce).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct WasAirborne;

pub static MODULE: Module = Module {
    name: "bounce_drag",
    flags: 0,
    equip,
    arm,
    can_use,
    start,
    stop,
    tick: Some(tick),
    requires: &[Cap::Body, Cap::Goals],
    ..Module::DEFAULT
};

fn equip(_: &mut World, _: Entity, _: Inst, _: &'static (dyn std::any::Any + Sync)) {}
fn arm(w: &mut World, e: Entity, inst: Inst) {
    crate::goals::arm(w, e, inst.slot, 1);
}
fn can_use(_: &mut World, _: Entity, _: Inst) -> bool {
    false
}
fn start(_: &mut World, _: Entity, _: Inst) {}
fn stop(_: &mut World, _: Entity, _: Inst) {}

/// The tick of the module: its system, run by `arbitrate` while an animal carries it.
fn tick(w: &mut World) {
    crate::goals::tick_state(w, physics);
}
/// Drag and bounce of every cube that holds a block.
fn physics(
    mut commands: Commands,
    mut q: Query<(Entity, &Held, &Contact, &mut Vel, Option<&WasAirborne>)>,
) {
    for (e, held, contact, mut vel, airborne) in &mut q {
        let a = held.archetype;
        let factor = if contact.on_ground { a.friction_modifier } else { a.air_drag_modifier };
        vel.0.x *= f64::from(factor);
        vel.0.z *= f64::from(factor);
        if contact.on_ground && airborne.is_some() && vel.0.y < 0.0 {
            vel.0.y = -vel.0.y * f64::from(a.bounciness);
        }
        if contact.on_ground {
            commands.entity(e).remove::<WasAirborne>();
        } else {
            commands.entity(e).insert(WasAirborne);
        }
    }
}
