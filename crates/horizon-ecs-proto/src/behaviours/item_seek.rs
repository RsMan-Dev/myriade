//! A sulfur cube with no block turns toward the item it searches for (`SulfurCube.SearchForItemsGoal`, `tryToSearch`): the hop's
//! direction takes the bearing of the point, turned by at most `turn` degrees per tick. The goal runs while the cube searches
//! and may search (`swallow_hold::may_search`); it re-arms each tick while the point is set.
//! DOUTE : la portée de recherche et le choix de l'objet viennent de l'événement qui écrit `Seek.point` (socle, « objet apparu »).
//! DOUTE : la vitesse de rotation (10 degrés par tick) et la recherche par tag ne sont pas relues dans la source ici.
use super::swallow_hold::may_search;
use super::*;
use crate::cow::*;
use crate::goals::LOOK;
use crate::locomotion::hop::Hop;
use crate::rules::item_seek::{aim_degrees, rotlerp};
use horizon_core::geometry::Vec3;

/// The point the cube aims at (the item it searches for).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Seek {
    pub point: Vec3,
}

/// The turn per tick toward the point, in degrees (species).
#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub turn: f32,
}
impl Params {
    pub const DEFAULT: Self = Self { turn: 10.0 };
}

pub static MODULE: Module = Module {
    name: "item_seek",
    flags: LOOK,
    equip,
    arm,
    can_use,
    start,
    stop,
    requires: &[Cap::Body, Cap::Goals],
    ..Module::DEFAULT
};

fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(TurnRate(params::<Params>(p).turn));
}
/// The turn per tick, kept on the cube.
#[derive(Component, Clone, Copy, Debug)]
pub struct TurnRate(pub f32);

fn arm(w: &mut World, e: Entity, inst: Inst) {
    crate::goals::arm(w, e, inst.slot, 1);
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    w.get::<Seek>(e).is_some() && may_search(w, e)
}
/// Turns the hop's direction toward the point by at most the turn rate (`rotlerp`).
fn start(w: &mut World, e: Entity, inst: Inst) {
    let (Some(seek), Some(pos), Some(rate)) = (w.get::<Seek>(e).copied(), w.get::<Pos>(e).map(|p| p.0), w.get::<TurnRate>(e).map(|r| r.0)) else {
        return;
    };
    let aim = aim_degrees(seek.point, pos);
    if let Some(mut hop) = w.get_mut::<Hop>(e) {
        hop.dir = rotlerp(hop.dir, aim, rate);
    }
    arm(w, e, inst);
}
fn stop(_: &mut World, _: Entity, _: Inst) {}
