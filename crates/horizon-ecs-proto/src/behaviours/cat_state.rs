//! The state of a tame cat, shared by its three goals (`CatRelaxOnOwnerGoal` fiche 62, `CatLieOnBlockGoal` 63, `CatSitOnBlockGoal` 64).
//! `CatPose` is the `Mob`'s `extra.sitting_pose`, `extra.flag` (lying) and `extra.relax_one`. This file is not a module: only the three
//! goals read or write it. `OwnerAsleep` stands for the sleep of the owner, which the frame does not give yet (BESOIN DU CADRE).
use super::*;
use crate::cow::{Pos, Level};

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct CatPose {
    pub sitting_pose: bool,
    pub lying: bool,
    pub relax: bool,
}

/// The owner of this cat is asleep. BESOIN DU CADRE : `PlayerState.sleeping` (the sleep of each player). Until the frame gives it,
/// nothing sets this marker in the game, so the relax goal never starts (the fallback of the fiche); the tests set it.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct OwnerAsleep;

pub fn owner_asleep(w: &World, e: Entity) -> bool {
    w.get::<OwnerAsleep>(e).is_some()
}
/// The cat has an owner (`Mob`: `extra.tame`).
pub fn tamed(w: &World, e: Entity) -> bool {
    w.get::<crate::tame::Owner>(e).is_some_and(|o| o.0.is_some())
}
/// Creates the pose of the cat once (each of its goals calls it at creation).
pub fn ensure(w: &mut World, e: Entity) {
    if w.get::<CatPose>(e).is_none() {
        w.entity_mut(e).insert(CatPose::default());
    }
}
pub fn pose(w: &World, e: Entity) -> CatPose {
    w.get::<CatPose>(e).copied().unwrap_or_default()
}
pub fn set(w: &mut World, e: Entity, f: impl FnOnce(&mut CatPose)) {
    if let Some(mut p) = w.get_mut::<CatPose>(e) {
        f(&mut p);
    }
}
pub use crate::rules::cat_state::{empty_above, reduced};
/// The block the entity stands in.
pub fn feet(w: &World, e: Entity) -> [i32; 3] {
    let p = w.get::<Pos>(e).unwrap().0;
    [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32]
}
/// The level's world (the blocks, their tags and properties).
pub fn level(w: &World) -> &myriade_engine_lib::mobs::MobWorld {
    w.resource::<Level>().mw.0
}

/// The ocelot trusts players (`Ocelot.mobInteract`, the `Trusting` tag); the game sets it when one feeds it. Read by `ocelot:trusting`.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Trusting;
