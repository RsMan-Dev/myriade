//! `SetWalkTargetAwayFrom.entity(memory, speed, desired, false)` (`piglin.rs:1054-1140`, `WalkAway::step`): while the entity
//! named by `memory` is nearer than `desired`, the walk goes to a spot away from it (`LandRandomPos.getPosAway(16, 7)`). A walk
//! under way is not interrupted: a new spot is asked only when the path is done. Generic: the memory is a parameter.
use crate::brain::*;
use crate::cow::Pos;
use crate::rules::walk_away::is_nearer_than;
use crate::spots::Pick;
use crate::targeting::info;

/// `MemoryModuleType.AVOID_TARGET`: who the entity keeps away from (set by the hit and the broadcasts, with a time to live).
pub static AVOID_TARGET: MemKey = MemKey::new("minecraft:avoid_target", Shape::Who);

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:walk_away_from",
    mems: &[&WALK_TARGET],
    requires: &[Cap::Brain, Cap::Locomotion, Cap::Targets],
    start,
    tick,
    ..Behaviour::DEFAULT
};

pub struct Params {
    pub speed: f32,
    pub desired: f64,
    pub memory: &'static MemKey,
}

fn start(w: &mut World, e: Entity, c: &Ctx) {
    aim(w, e, c.get::<Params>());
}

/// Runs while the remembered entity is near; the walk is re-aimed when the path is done.
fn tick(w: &mut World, e: Entity, c: &Ctx) -> bool {
    let p = c.get::<Params>();
    if !near(w, e, p) {
        return false;
    }
    if nav_done(w, e) {
        aim(w, e, p);
    }
    true
}

fn near(w: &World, e: Entity, p: &Params) -> bool {
    let (Some(from), Some(me)) = (source(w, e, p), w.get::<Pos>(e)) else { return false };
    is_nearer_than(from, [me.0.x, me.0.y, me.0.z], p.desired)
}

fn source(w: &World, e: Entity, p: &Params) -> Option<[f64; 3]> {
    match get(w, e, p.memory) {
        Some(MemValue::Who(who)) => info(w, *who).map(|i| [i.pos.x, i.pos.y, i.pos.z]),
        // `SetWalkTargetAwayFrom.pos`: a block, from its bottom centre (`Vec3.atBottomCenterOf`).
        Some(MemValue::Block(b)) => Some([f64::from(b[0]) + 0.5, f64::from(b[1]), f64::from(b[2]) + 0.5]),
        _ => None,
    }
}

fn aim(w: &mut World, e: Entity, p: &Params) {
    if !near(w, e, p) {
        return;
    }
    let Some(from) = source(w, e, p) else { return };
    if let Some(at) = pick(w, e, Pick::LandAway { min_h: 0.0, max_h: 16.0, v: 7, from }) {
        let walk = WalkTarget { target: Tracker::Point(at), speed: p.speed, close: 0 };
        set(w, e, &WALK_TARGET, MemValue::Walk(walk), None);
    }
}
