//! Flight from repellents (`PiglinAvoidRepellent`, `mobs/piglin.rs:1640-1690`): a block of the list, or a creature of the
//! list, within `horizontal` blocks (`vertical` up and down) makes the animal remember that spot as its `Avoid` point, and
//! hands the walk to `walk_away`. The search runs when the selector asks this goal, and `walk_away` is started by request.
use crate::goals;
use super::walk_away::{Avoid, MODULE as WALK_AWAY};
use super::*;
use crate::{cow::*, goals::*};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The blocks and the species that repel, by name.
    pub blocks: &'static [&'static str],
    pub species: &'static [&'static str],
    pub horizontal: f64,
    pub vertical: i32,
    /// Ticks the spot stays remembered (the avoid ttl).
    pub ttl: u64,
}
impl Params {
    pub const DEFAULT: Self = Self { blocks: &[], species: &[], horizontal: 8.0, vertical: 4, ttl: 120 };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Flee(pub Params);

pub static MODULE: Module = Module {
    name: "flee_repellent",
    flags: 0,
    equip,
    arm,
    can_use,
    start,
    tick: Some(tick),
    requires: &[Cap::Body, Cap::Goals],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, _: Inst, p: &(dyn std::any::Any + Sync)) {
    let f = *params::<Params>(p);
    if !f.blocks.is_empty() {
        w.init_resource::<crate::block_watch::BlockWatch>();
        w.resource_scope(|w, mut bw: Mut<crate::block_watch::BlockWatch>| bw.watch(w.resource::<Level>(), f.blocks));
    }
    w.entity_mut(e).insert(Flee(f));
}
/// Nothing that repels exists (no block of the list in the level, no species of the list): the goal has nothing to ask until one
/// appears (a block placed: `tick`; a species: only the goals with a species list ask, they never park).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
struct Parked;
fn nothing_to_flee(w: &World, f: &Params) -> bool {
    f.species.is_empty() && !w.get_resource::<crate::block_watch::BlockWatch>().is_some_and(|b| b.any(f.blocks))
}
/// The blocks that appeared wake the goals parked for them.
fn tick(w: &mut World) {
    let appeared = match w.get_resource::<crate::block_watch::BlockWatch>() {
        Some(b) if !b.appeared.is_empty() => b.appeared.clone(),
        _ => return,
    };
    let es: Vec<Entity> = w.query_filtered::<(Entity, &Flee), With<Parked>>().iter(w).filter(|(_, f)| f.0.blocks.iter().any(|b| appeared.contains(b))).map(|x| x.0).collect();
    for e in es {
        w.entity_mut(e).remove::<Parked>();
        arm_module(w, e, &MODULE, 1);
    }
}
/// The sensor of the repellents runs every 20 ticks (`PiglinSpecificSensor`): the goal is asked at that cadence, from a phase
/// drawn at creation as the jar does (`Sensor.<init>`: `timeToTick = random.nextInt(scanRate)`, then `--timeToTick <= 0`).
fn arm(w: &mut World, e: Entity, _: Inst) {
    let f = w.get::<Flee>(e).unwrap().0;
    if nothing_to_flee(w, &f) {
        w.entity_mut(e).insert(Parked);
    } else {
        let phase = with_rng(w, e, |r| r.next_int(20)).max(1);
        arm_module(w, e, &MODULE, phase);
    }
}
/// The next look is 20 ticks later; none at all while nothing that repels exists.
fn ask_later(w: &mut World, e: Entity) {
    let f = w.get::<Flee>(e).unwrap().0;
    if nothing_to_flee(w, &f) {
        w.entity_mut(e).insert(Parked);
    } else {
        arm_module(w, e, &MODULE, 20);
    }
}
/// `canUse`: a repellent within reach; none: the next look is 20 ticks later.
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let f = w.get::<Flee>(e).unwrap().0;
    let found = repellent_near(w, e, &f).is_some();
    if !found {
        // The scan finds none: the memory is erased (`brain.eraseMemory(NEAREST_REPELLENT)`).
        w.entity_mut(e).remove::<super::walk_away::Avoid>();
        ask_later(w, e);
    }
    found
}
/// The spot to keep away from is remembered, the walk away is requested, and this goal ends.
fn start(w: &mut World, e: Entity, inst: Inst) {
    let f = w.get::<Flee>(e).unwrap().0;
    let Some(at) = repellent_near(w, e, &f) else { return };
    let until = now_of(w, e) + f.ttl;
    w.entity_mut(e).insert(Avoid { at, until });
    if let Some(slot) = w.get::<GoalSet>(e).and_then(|g| g.slot_of(&WALK_AWAY)) {
        w.resource_mut::<Requests>().0.push(Req::Start(e, slot));
    }
    w.resource_mut::<Requests>().0.push(Req::End(e, inst.slot));
    ask_later(w, e);
}
/// The point of the nearest repellent (a block of the list, or a creature of the list) within reach.
/// Socle absent: `goals::repellent_near` reads the blocks of the level and the species of the creatures.
fn repellent_near(w: &World, e: Entity, f: &Params) -> Option<[f64; 3]> {
    // The blocks come from the set kept by events (`BlockWatch`), not from a walk over the cube around the animal.
    let at = w.get::<Pos>(e)?.0;
    let mut best: Option<[f64; 3]> = None;
    let mut d_best = f64::MAX;
    // On a tie the first of the walk x, z, y wins (as the scan of the cube did): the key is (distance, x, z, y).
    let mut key_best = [i32::MAX; 3];
    if let Some(bw) = w.get_resource::<crate::block_watch::BlockWatch>() {
        bw.near(f.blocks, [at.x, at.y, at.z], f.horizontal, f.vertical, |p| {
            let c = [f64::from(p[0]) + 0.5, f64::from(p[1]) + 0.5, f64::from(p[2]) + 0.5];
            let d = (c[0] - at.x).powi(2) + (c[1] - at.y).powi(2) + (c[2] - at.z).powi(2);
            // The spot kept is the bottom centre of the block (`Vec3.atBottomCenterOf`), the choice goes by the centre.
            let c = [c[0], f64::from(p[1]), c[2]];
            let key = [p[0], p[2], p[1]];
            if d < d_best || (d == d_best && key < key_best) {
                d_best = d;
                key_best = key;
                best = Some(c);
            }
        });
    }
    if !f.species.is_empty()
        && let Some(o) = goals::repellent_near(w, e, &[], f.species, f.horizontal, f.vertical)
    {
        let d = (o[0] - at.x).powi(2) + (o[1] - at.y).powi(2) + (o[2] - at.z).powi(2);
        if d < d_best {
            best = Some(o);
        }
    }
    best
}
