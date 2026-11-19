//! `Ravager.aiStep` (leaves, on a horizontal bump) and `Entity.checkInsideBlocks` -> `CropBlock.entityInside` (crops, when the
//! ravager enters their cell), both only with `mobGriefing`. A broken block drops its loot, named for the ravager (`breaker`) when a
//! crop, else `by` for a leaf. Jump: a bump that broke nothing jumps; one that broke leaves jumps only when the game kept them all.
use super::*;
use crate::{cow::Pos, goals::*};
use crate::level::{asks_living_destroy_block, block_has_class, break_block, can_grief, cells_in, is_on_ground, jump, jump_if_kept};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// Block classes that a bump breaks (`LeavesBlock`), and those that a walk through breaks (`CropBlock`, `PitcherCropBlock`).
    pub bumped: &'static [&'static str],
    pub walked: &'static [&'static str],
}
impl Params {
    pub const DEFAULT: Self = Self { bumped: &["LeavesBlock"], walked: &["CropBlock", "PitcherCropBlock"] };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Trample(pub Params);

pub static MODULE: Module = Module {
    name: "trample",
    flags: 0,
    equip,
    requires: &[Cap::Body, Cap::Solid],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, _: Inst, p: &(dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Trample(*params::<Params>(p)));
}
/// The bump of the walk on a wall (`horizontalCollision`), with the bounds inflated by 0.2 as vanilla.
pub fn on_bump(w: &mut World, e: Entity) {
    let Some(t) = w.get::<Trample>(e).map(|t| t.0) else { return };
    if !can_grief(w, e) {
        return;
    }
    let mut destroyed = false;
    for p in cells_in(w, e, 0.2) {
        if block_has_class(w, p, t.bumped) {
            break_block(w, p, true, Some(e), None);
            destroyed = true;
        }
    }
    if !is_on_ground(w, e) {
        return;
    }
    if !destroyed {
        jump(w, e);
    } else if asks_living_destroy_block(w) {
        // The jump follows only when the game kept every leaf: `IfBroke` in vanilla (declared in the notes).
        jump_if_kept(w, e);
    }
}
/// The ravager entered a cell: a crop there is broken, named for the ravager (`breaker`), when mobs may grief.
pub fn on_enter_cell(w: &mut World, e: Entity, p: [i32; 3]) {
    let Some(t) = w.get::<Trample>(e).map(|t| t.0) else { return };
    if block_has_class(w, p, t.walked) && can_grief(w, e) {
        break_block(w, p, true, None, Some(e));
    }
}
