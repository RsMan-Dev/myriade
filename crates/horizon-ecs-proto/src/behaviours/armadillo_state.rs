//! L'état d'un animal à carapace, lu par deux buts (`rolled`, `baby_follow_adult`). Données seules : ce n'est pas un module
//! (pas de `MODULE`, pas d'entrée dans la table). L'état est écrit par `rolled::set_state`.
use bevy_ecs::prelude::*;

pub const IDLE: u8 = 0;
pub const ROLLING: u8 = 1;
pub const SCARED: u8 = 2;
pub const UNROLLING: u8 = 3;

/// L'état courant (`state` de l'animal, IDLE par défaut).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArmadilloState(pub u8);

/// L'état d'une entité : IDLE sans le composant.
pub fn of(w: &World, e: Entity) -> u8 {
    w.get::<ArmadilloState>(e).map_or(IDLE, |s| s.0)
}

/// Crée le composant s'il manque ; les deux buts le demandent à leur création.
pub fn ensure(w: &mut World, e: Entity) {
    if w.get::<ArmadilloState>(e).is_none() {
        w.entity_mut(e).insert(ArmadilloState(IDLE));
    }
}
