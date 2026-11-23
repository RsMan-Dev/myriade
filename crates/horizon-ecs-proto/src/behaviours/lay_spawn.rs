//! `LaySpawn` (`mobs/frog.rs:338-476`: `TryFindLandNearLiquid` then `TryLaySpawnOnFluidNearLand`): a frog that `Carrying` (breed with
//! `carries`) walks to land beside water and lays `minecraft:frogspawn` on the water or the block that supports it. The goal has no flag and
//! runs while the frog carries; it is asked for by the `carrying` event (no scan for pregnancies). While it runs, one search per 40 own
//! ticks (`next_ok`) and one try to lay per tick, as the Mob does (the polling is only for a pregnant frog).
use super::breed::Carrying;
use super::*;
use crate::cow::*;
use crate::goals::*;
use myriade_engine_lib::blocks::Fluid;
use myriade_engine_lib::mobs::MobWorld;
use crate::rules::lay_spawn::manhattan_cells;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// Search range of `withinManhattan`.
    pub range: i32,
    /// Own ticks between two searches.
    pub search_every: u64,
    pub speed: f64,
}
impl Params {
    pub const DEFAULT: Self = Self { range: 8, search_every: 40, speed: 1.0 };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Layer {
    pub slot: u8,
    pub p: &'static Params,
    pub next_ok: u64,
    pub target: Option<[i32; 3]>,
}
pub static MODULE: Module = Module {
    name: "lay_spawn",
    flags: 0,
    equip,
    arm,
    can_use,
    start,
    stop,
    systems,
    after_change: Some(after_change),
    requires: &[Cap::Body, Cap::Goals, Cap::Locomotion],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Layer { slot: inst.slot, p: params::<Params>(p), next_ok: 0, target: None });
}
/// A frog that is already pregnant when the goals are armed is asked for at once.
fn arm(w: &mut World, e: Entity, inst: Inst) {
    if w.get::<Carrying>(e).is_some() {
        crate::goals::arm(w, e, inst.slot, 1);
    }
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    w.get::<Carrying>(e).is_some()
}
fn start(w: &mut World, e: Entity, _: Inst) {
    step(w, e);
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    if let Some(mut l) = w.get_mut::<Layer>(e) {
        l.target = None;
    }
}
/// The goal that went off took its place: nothing to do (the carrying event asks again).
fn after_change(w: &mut World, e: Entity, c: Change) {
    if c == Change::Ended && w.get::<Carrying>(e).is_some() {
        arm_module(w, e, &MODULE, 2);
    }
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems(events_run.in_set(slot));
}
/// The pregnancy begins: the goal is asked for at the next selector tick; and each tick the running goal acts (search, lay).
fn events_run(ev: Res<Events>, mut reqs: ResMut<Requests>, q: Query<&Layer>, running: Query<(Entity, &Layer), With<Carrying>>) {
    for e in ev.carrying.iter() {
        if q.get(e).is_ok() {
            reqs.0.push(Req::Call(e, ask));
        }
    }
    for (e, _) in &running {
        reqs.0.push(Req::Call(e, step));
    }
}
fn ask(w: &mut World, e: Entity) {
    arm_module(w, e, &MODULE, 1);
}
fn block_of(w: &World, e: Entity) -> [i32; 3] {
    let p = w.get::<Pos>(e).map_or_else(Default::default, |p| p.0);
    [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32]
}
fn nav_done(w: &World, e: Entity) -> bool {
    w.get::<Following>(e).is_none() && w.get::<NeedsPath>(e).is_none()
}
fn empty(mw: &MobWorld, p: [i32; 3]) -> bool {
    mw.collision_bounds(p).is_none()
}
fn is_air(mw: &MobWorld, p: [i32; 3]) -> bool {
    mw.facts.is_air(mw.world.block(p[0], p[1], p[2]))
}
/// North, east, south, west.
const HORIZONTAL: [[i32; 2]; 4] = [[0, -1], [1, 0], [0, 1], [-1, 0]];
/// `TryFindLandNearLiquid`'s test: an empty cell on solid ground with air that has water below beside it.
fn land_near_water(mw: &MobWorld, p: [i32; 3]) -> bool {
    if !empty(mw, p) || empty(mw, [p[0], p[1] - 1, p[2]]) {
        return false;
    }
    HORIZONTAL.iter().any(|d| {
        let n = [p[0] + d[0], p[1], p[2] + d[1]];
        is_air(mw, n) && mw.fluid(n[0], n[1] - 1, n[2]) == Fluid::Water
    })
}
/// One brain turn of the Mob's `step`: forget the target when the walk is over, search (rate-limited), then try to lay.
fn step(w: &mut World, e: Entity) {
    let Some(mut l) = w.get::<Layer>(e).copied() else { return };
    if w.get::<Carrying>(e).is_none() {
        return;
    }
    let done = nav_done(w, e);
    if l.target.is_some() && done {
        l.target = None;
    }
    let at = block_of(w, e);
    let now = now_of(w, e);
    let mw = w.resource::<Level>().mw.0;
    if l.target.is_none() && done && mw.fluid(at[0], at[1], at[2]) != Fluid::Water && now >= l.next_ok {
        l.next_ok = now + l.p.search_every;
        let found = manhattan_cells(at, l.p.range).into_iter().find(|p| (p[0] != at[0] || p[2] != at[2]) && land_near_water(mw, *p));
        if let Some(p) = found {
            l.target = Some(p);
            *w.get_mut::<Layer>(e).unwrap() = l;
            go_to_at(w, e, [f64::from(p[0]) + 0.5, f64::from(p[1]), f64::from(p[2]) + 0.5], l.p.speed, 0);
        }
    }
    *w.get_mut::<Layer>(e).unwrap() = l;
    if l.target.is_some() {
        lay(w, e);
    }
}
/// `TryLaySpawnOnFluidNearLand`: on the ground out of the water, a neighbour of the block below whose top is open and that holds water or a
/// supporting block, with air above: frogspawn there.
fn lay(w: &mut World, e: Entity) {
    let on_ground = w.get::<Contact>(e).is_some_and(|c| c.on_ground);
    let in_water = w.get::<FluidState>(e).is_some_and(|f| f.in_water);
    if in_water || !on_ground {
        return;
    }
    let b = block_of(w, e);
    let below = [b[0], b[1] - 1, b[2]];
    let mw = w.resource::<Level>().mw.0;
    for d in HORIZONTAL {
        let rel = [below[0] + d[0], below[1], below[2] + d[1]];
        let open_top = mw.collision_bounds(rel).is_none_or(|bb| bb.max_position().y < 1.0);
        let supports = mw.fluid(rel[0], rel[1], rel[2]) == Fluid::Water || mw.has_tag("minecraft:supports_frogspawn", mw.block_name(rel[0], rel[1], rel[2]));
        let spawn = [rel[0], rel[1] + 1, rel[2]];
        if open_top && supports && is_air(mw, spawn) {
            w.resource_mut::<StateChanges>().0.push((spawn[0], spawn[1], spawn[2], "minecraft:frogspawn", &[]));
            w.entity_mut(e).remove::<Carrying>();
            return;
        }
    }
}
