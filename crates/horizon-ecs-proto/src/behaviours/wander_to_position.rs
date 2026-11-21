//! `WanderingTrader.WanderToPositionGoal` (`mobs/trader.rs`): a creature that was sent to a place (`send_to`: the `WanderPoint` the
//! spawner of its visit lays) walks to it, in hops of 10 blocks while it is farther than that, and the goal ends within `stop` blocks of
//! the place; however it ends (arrival, or another goal that took the flag), the place is forgotten. Nothing is polled: the placing
//! wakes the goal, and only the running ones are visited (a hop is asked when the previous path is over). Move flag.
use super::door_interact::closer_to_center;
use super::*;
use crate::{cow::*, goals::*};
use horizon_core::geometry::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The distance from the place at which the goal ends (and does not start).
    pub stop: f64,
    pub speed: f64,
}
impl Params {
    pub const DEFAULT: Self = Self { stop: 2.0, speed: 0.35 };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Wanderer {
    p: Params,
    slot: u8,
}
/// Where the creature was sent (`Mob.extra.point`).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct WanderPoint(pub [f64; 3]);
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Wandering;

pub static MODULE: Module = Module { name: "wander_to_position", flags: MOVE, equip, can_use, start, stop, tick: Some(tick), requires: &[Cap::Body, Cap::Goals, Cap::Locomotion], ..Module::DEFAULT };
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Wanderer { p: *params::<Params>(p), slot: inst.slot });
}
/// Sends the creature to `point` (the visit's first place) and asks the goal.
pub fn send_to(w: &mut World, e: Entity, point: [f64; 3]) {
    w.entity_mut(e).insert(WanderPoint(point));
    if let Some(slot) = slot_of(w, e, &MODULE) {
        w.resource_mut::<Requests>().0.push(Req::Start(e, slot));
    }
}
fn floor3(p: [f64; 3]) -> [i32; 3] {
    p.map(|v| v.floor() as i32)
}
fn too_far(w: &World, e: Entity, p: [f64; 3], d: f64) -> bool {
    w.get::<Pos>(e).is_some_and(|o| !closer_to_center(floor3(p), o.0, d))
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let Some(p) = w.get::<WanderPoint>(e).map(|p| p.0) else { return false };
    let stop = w.get::<Wanderer>(e).unwrap().p.stop;
    too_far(w, e, p, stop)
}
fn start(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).insert(Wandering);
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<(Wandering, WanderPoint)>();
}
fn tick(w: &mut World) {
    crate::goals::tick_state(w, run);
}
fn run(q: Query<(Entity, Has<Following>, Has<NeedsPath>), With<Wandering>>, mut cmd: Commands) {
    for (e, following, asked) in &q {
        if following || asked {
            continue;
        }
        cmd.queue(move |w: &mut World| {
            let (Some(p), Some(g), Some(own)) = (w.get::<WanderPoint>(e).map(|p| p.0), w.get::<Wanderer>(e).copied(), w.get::<Pos>(e).map(|p| p.0)) else { return };
            if !too_far(w, e, p, g.p.stop) {
                w.resource_mut::<Requests>().0.push(Req::End(e, g.slot));
            } else if too_far(w, e, p, 10.0) {
                let d = Vec3::new(p[0].floor() - own.x, p[1].floor() - own.y, p[2].floor() - own.z);
                let len = (d.x * d.x + d.y * d.y + d.z * d.z).sqrt().max(1e-9);
                let at = [d.x / len * 10.0 + own.x, d.y / len * 10.0 + own.y, d.z / len * 10.0 + own.z];
                go_to_at(w, e, at, g.p.speed, 1);
            } else {
                go_to_at(w, e, [p[0].floor(), p[1].floor(), p[2].floor()], g.p.speed, 1);
            }
        });
    }
}
