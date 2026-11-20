//! `RunAroundLikeCrazyGoal`: a mount that is not tame and carries a rider bolts to a spot 5 blocks about (`DefaultRandomPos(5, 4)`) at 1.2
//! of its speed, while the path lasts and the rider stays. Each tick it draws one in 50: the temper then decides whether it is
//! tamed by its rider (`nextInt(maxTemper) < temper`) or throws it off (+5 temper) and rears. What wakes it: a rider taking the seat
//! (`Events.mounted`); while it runs a system visits it each tick.
use super::*;
use crate::{cow::*, goals::*, mount::*};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub speed: f64,
    pub h: i32,
    pub v: i32,
}
impl Params {
    pub const DEFAULT: Self = Self { speed: 1.2, h: 5, v: 4 };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct RunAround {
    p: &'static Params,
    slot: u8,
    pick: Option<[f64; 3]>,
}
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Crazy;

pub static MODULE: Module = Module {
    name: "run_around_like_crazy",
    flags: MOVE,
    retry_on_end: true,
    equip,
    can_use,
    start,
    stop,
    systems,
    requires: &[Cap::Rideable, Cap::Tamable, Cap::Locomotion],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(RunAround { p: params::<Params>(p), slot: inst.slot, pick: None });
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems(mounted_events.in_set(slot));
    s.add_systems(crazy_run.in_set(slot));
}
fn mounted_events(ev: Res<Events>, q: Query<&RunAround>, mut reqs: ResMut<Requests>) {
    for e in ev.mounted.iter() {
        if let Ok(g) = q.get(e) {
            reqs.0.push(Req::Start(e, g.slot));
        }
    }
}
fn tame(w: &World, e: Entity) -> bool {
    w.get::<crate::tame::Owner>(e).is_some_and(|o| o.0.is_some())
}
fn can_use(w: &mut World, e: Entity, inst: Inst) -> bool {
    let Some(g) = w.get::<RunAround>(e).copied() else { return false };
    if tame(w, e) || rider_of(w, e).is_none() {
        return false;
    }
    let pick = crate::spots::pick_for(w, e, crate::spots::Pick::Default { h: g.p.h, v: g.p.v });
    w.get_mut::<RunAround>(e).unwrap().pick = pick;
    if pick.is_none() {
        // The selector asks again while a rider is on an untamed mount (no timer in `canUse`).
        arm(w, e, inst.slot, 2);
    }
    pick.is_some()
}
fn start(w: &mut World, e: Entity, _: Inst) {
    let g = *w.get::<RunAround>(e).unwrap();
    if let Some(at) = g.pick {
        go_to_at(w, e, at, g.p.speed, 0);
    }
    w.entity_mut(e).insert(Crazy);
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Crazy>();
    stop_nav(w, e);
}
fn crazy_run(q: Query<Entity, With<Crazy>>, mut reqs: ResMut<Requests>) {
    for e in &q {
        reqs.0.push(Req::Call(e, crazy_step));
    }
}
/// `canContinueToUse` then `tick` of one mount.
fn crazy_step(w: &mut World, e: Entity) {
    let Some(g) = w.get::<RunAround>(e).copied() else { return };
    let done = w.get::<Following>(e).is_none() && w.get::<NeedsPath>(e).is_none();
    if tame(w, e) || rider_of(w, e).is_none() || done {
        w.resource_mut::<Requests>().0.push(Req::End(e, g.slot));
        return;
    }
    if tame(w, e) || with_rng(w, e, |r| r.next_int(50)) != 0 {
        return;
    }
    let Some(m) = w.get::<Mount>(e).copied() else { return };
    if with_rng(w, e, |r| r.next_int(m.max_temper)) < m.temper {
        if let Some(who) = m.rider {
            crate::tame::set_owner(w, e, who);
        }
        return;
    }
    // `refuseRider`: +5 temper, the rider thrown off, the mount rears (`makeMad`).
    modify_temper(w, e, 5);
    dismount(w, e);
}
