//! Aller à un point d'attache et poser un bloc (`TurtleLayEggGoal`, forme générique). An animal that carries a load (`Carrying`, given
//! by `load`) and stands within `radius` of its anchor walks to a spot with the floor `floor` and air above (`MoveToBlock`, `range`),
//! digs for `dig_ticks` ticks on the spot, then sets one of `states` (drawn at random) above it through `Events.host` and drops the load.
//! The search cadence is the turtle's: a countdown drawn at each search, the next `canUse` chained every 2 ticks.
use super::*;
use crate::move_to_block::MoveToBlock;
use crate::{blocks::resolve, cow::*, goals::*};
use horizon_client_lib::mobs::MobWorld;
use horizon_core::geometry::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub speed: f64,
    pub range: i32,
    pub vertical: i32,
    /// `closerToCenterThan(anchor, radius)`.
    pub radius: f64,
    /// Ticks of digging on the spot before the block is set (`counter > dig_ticks`).
    pub dig_ticks: i32,
    /// The tag of the floor the spot must have.
    pub floor: &'static str,
    /// The block set above the spot, and the properties of each of its possible states (one drawn at random).
    pub name: &'static str,
    pub states: &'static [&'static [(&'static str, &'static str)]],
    /// The sound of the laying and its volume.
    pub sound: Option<(&'static str, f32)>,
}
impl Params {
    pub const DEFAULT: Self = Self { speed: 1.0, range: 16, vertical: 1, radius: 9.0, dig_ticks: 200, floor: "", name: "", states: &[], sound: None };
}
/// The load the animal carries; `load` gives it one, the laying takes it away.
#[derive(Component, Clone, Copy, Debug)]
pub struct Carrying;
/// Where the animal goes back to: its position at creation (block), moved by a rule through `set_anchor`.
#[derive(Component, Clone, Copy, Debug)]
pub struct Anchor(pub [i32; 3]);
/// The goal's state: its parameters, its walk, the countdown to the next search, whether a `canUse` deadline is armed, the ticks dug.
#[derive(Component, Clone, Debug)]
pub struct Lay {
    pub def: Params,
    pub walk: MoveToBlock,
    pub next_start: i32,
    pub armed: bool,
    pub slot: u8,
    pub dug: i32,
}
/// The goal runs: its system visits it.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Digging;

pub static MODULE: Module = Module {
    name: "lay_egg",
    flags: MOVE | JUMP,
    equip,
    fired,
    can_use,
    start,
    stop,
    tick: Some(tick),
    requires: &[Cap::Body, Cap::Goals, Cap::Locomotion],
    ..Module::DEFAULT
};
/// Gives the animal a load: the goal's first search comes at once.
pub fn load(w: &mut World, e: Entity) {
    w.entity_mut(e).insert(Carrying);
    if let Some(slot) = w.get::<Lay>(e).map(|l| l.slot) {
        chain_slot(w, e, slot, 2);
    }
}
/// Moves the anchor (a rule that changes where the animal goes back to).
pub fn set_anchor(w: &mut World, e: Entity, at: [i32; 3]) {
    w.entity_mut(e).insert(Anchor(at));
}
fn equip(w: &mut World, e: Entity, inst: Inst, p: &(dyn std::any::Any + Sync)) {
    let def = *params::<Params>(p);
    let walk = MoveToBlock::new(def.speed, def.range, def.vertical);
    w.entity_mut(e).insert(Lay { def, walk, next_start: 0, armed: false, slot: inst.slot, dug: 0 });
    if w.get::<Anchor>(e).is_none() {
        let p = w.get::<Pos>(e).map_or(Vec3::ZERO, |p| p.0);
        set_anchor(w, e, [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32]);
    }
}
/// Arms the next `canUse` request, once: one chain per animal.
fn chain(w: &mut World, e: Entity, wait: i32) {
    let Some(slot) = w.get::<Lay>(e).map(|l| l.slot) else { return };
    chain_slot(w, e, slot, wait);
}
fn chain_slot(w: &mut World, e: Entity, slot: u8, wait: i32) {
    {
        let Some(mut l) = w.get_mut::<Lay>(e) else { return };
        if l.armed {
            return;
        }
        l.armed = true;
    }
    crate::goals::arm(w, e, slot, wait);
}
fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    if let Some(mut l) = w.get_mut::<Lay>(e) {
        l.armed = false;
    }
    w.get::<Digging>(e).is_none()
}
/// The first test of `canUse`: a load, and the anchor within `radius`.
fn wants_laying(w: &World, e: Entity) -> bool {
    let (Some(l), Some(a), Some(p)) = (w.get::<Lay>(e), w.get::<Anchor>(e), w.get::<Pos>(e)) else { return false };
    w.get::<Carrying>(e).is_some() && near(a.0, [p.0.x, p.0.y, p.0.z], l.def.radius)
}
/// The spot: air above it, the floor tag under it.
fn floor_ok(tag: &'static str) -> impl Fn(&MobWorld, [i32; 3]) -> bool {
    move |mw, p| mw.facts.is_air(mw.world.block(p[0], p[1] + 1, p[2])) && mw.has_tag(tag, mw.block_name(p[0], p[1], p[2]))
}
fn feet(w: &World, e: Entity) -> [i32; 3] {
    let p = w.get::<Pos>(e).unwrap().0;
    [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32]
}
/// `closerToCenterThan`: the centre of the anchor block is nearer than `d`.
fn near(a: [i32; 3], p: [f64; 3], d: f64) -> bool {
    let v = [f64::from(a[0]) + 0.5 - p[0], f64::from(a[1]) + 0.5 - p[1], f64::from(a[2]) + 0.5 - p[2]];
    v[0] * v[0] + v[1] * v[1] + v[2] * v[2] < d * d
}
fn reduced(n: i32) -> i32 {
    (n + 1) / 2
}
/// `canUse`: skip the countdown, or draw the next one and search; a search that finds nothing keeps the chain going.
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    if !wants_laying(w, e) {
        return false;
    }
    let left = w.get::<Lay>(e).unwrap().next_start;
    if left > 0 {
        // The `Mob` counts its countdown down once per ask (every 2 ticks): the search comes `left` asks later, in one deadline.
        w.get_mut::<Lay>(e).unwrap().next_start = 0;
        chain(w, e, 2 * left);
        return false;
    }
    let countdown = with_rng(w, e, |r| reduced(200 + r.next_int(200)));
    let base = feet(w, e);
    let tag = w.get::<Lay>(e).unwrap().def.floor;
    let mut l = w.get::<Lay>(e).unwrap().clone();
    let found = {
        let mw = w.resource::<Level>().mw.0;
        l.walk.may_find(base, mw, |s| mw.has_tag(tag, &mw.facts.get(s).name)) && l.walk.find_nearest(base, mw, floor_ok(tag))
    };
    l.next_start = countdown;
    *w.get_mut::<Lay>(e).unwrap() = l;
    if !found {
        chain(w, e, 2);
    }
    found
}
fn start(w: &mut World, e: Entity, _: Inst) {
    let mut l = w.get::<Lay>(e).unwrap().clone();
    with_rng(w, e, |r| l.walk.start(r));
    go_to_at(w, e, l.walk.target(), l.def.speed, 1);
    w.entity_mut(e).insert(Digging);
    *w.get_mut::<Lay>(e).unwrap() = l;
}
/// The end of the goal: a condition that holds asks again at once (the next +2 request).
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Digging>();
    chain(w, e, 2);
}
/// The tick of the module, run while some animal carries it (no system in the schedule).
fn tick(w: &mut World) {
    tick_state(w, dig_run);
}
/// The walk and the digging of each tick of the goal: the goal ends when its block is lost or the animal no longer qualifies;
/// the path is asked again every 40 ticks of walking.
fn dig_run(
    mut q: Query<(Entity, &mut Lay, &mut Rng, &Pos, &FluidState, &Anchor, Option<&Carrying>), With<Digging>>,
    level: Res<Level>,
    mut events: ResMut<Events>,
    mut reqs: ResMut<Requests>,
    mut cmd: Commands,
) {
    let mw = level.mw.0;
    for (e, mut l, mut rng, p, f, anchor, load) in &mut q {
        let pos = [p.0.x, p.0.y, p.0.z];
        let tag = l.def.floor;
        if !(l.walk.can_continue(mw, floor_ok(tag)) && load.is_some() && near(anchor.0, pos, l.def.radius)) {
            reqs.0.push(Req::End(e, l.slot));
            continue;
        }
        if l.walk.tick(pos, 1.0) {
            reqs.0.push(Req::Call(e, walk_again));
        }
        if f.in_water || !l.walk.reached {
            continue;
        }
        if l.dug < 1 {
            l.dug = 1;
        } else if l.dug > l.def.dig_ticks {
            let b = l.walk.block;
            let props = l.def.states.get(rng.next_int(l.def.states.len().max(1) as _) as usize).copied().unwrap_or(&[]);
            if let Some(id) = resolve(mw, l.def.name, props) {
                events.host.set_block([b[0], b[1] + 1, b[2]], id);
            }
            if let Some((sound, volume)) = l.def.sound {
                events.host.sound(sound, pos, volume, 1.0);
            }
            cmd.entity(e).remove::<Carrying>();
            l.dug = 0;
        }
        if l.dug > 0 {
            l.dug += 1;
        }
    }
}
fn walk_again(w: &mut World, e: Entity) {
    let Some(l) = w.get::<Lay>(e).cloned() else { return };
    go_to_at(w, e, l.walk.target(), l.def.speed, 1);
}
