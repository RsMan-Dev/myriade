//! The fuse of a charged sulfur cube (`SulfurCube.tickFuse`, `ignite`): a lit cube counts up to its maximum with the explosion's fuse
//! and blows up with the blast of the creeper family (`explosion::Fuse`). Lit by a burning arrow (`ignite`), the direction is +1 whatever
//! the target does. The constants (`max`, `radius`) come from the species.
//! DOUTE : la fin de l'entité après l'explosion (retrait) et l'allumage par flèche ne sont pas câblés ici (socle : tir, `Blast`).
//! DOUTE : `armed` reçoit l'instant de l'allumage ; sa sémantique exacte dans `explosion.rs` n'a pas été relue.
use crate::cow::*;
use crate::explosion::{set_dir, Fuse};
use bevy_ecs::prelude::*;

/// The fuse's constants (species): the maximum countdown and the blast radius.
#[derive(Component, Clone, Copy, Debug)]
pub struct Params {
    pub max: i32,
    pub radius: f32,
}

/// Lights the fuse of `e` (`ignite`): the countdown runs toward the blast.
pub fn ignite(w: &mut World, e: Entity, p: Params) {
    let now = now_of(w, e);
    let fuse = w.get::<Fuse>(e).copied().unwrap_or(Fuse { value: 0, at: now, dir: 1, max: p.max, radius: p.radius, armed: now, ignited: false });
    w.entity_mut(e).insert(Fuse { ignited: true, ..fuse });
    set_dir(w, e, 1);
}
