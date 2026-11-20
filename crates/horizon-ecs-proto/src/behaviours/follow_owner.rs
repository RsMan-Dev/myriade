//! `FollowOwnerGoal(speed, startDistance, stopDistance)`: a pet whose owner is `start` blocks away or more (and that is not
//! ordered to sit) walks to it until it is within `stop`; at 12 blocks it teleports to it instead (`TamableAnimal.tryToTeleportToOwner`).
//! What wakes it: the owner leaving the `start` radius (`PlayerWatch`), nothing else; while it runs a system visits it each tick.
use super::*;
use crate::{cow::*, goals::*, tame};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub speed: f64,
    pub start: f64,
    pub stop: f64,
}
impl Params {
    pub const DEFAULT: Self = Self { speed: 1.0, start: 10.0, stop: 2.0 };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct FollowOwner {
    p: &'static Params,
    slot: u8,
    recalc: i32,
    /// The tick the goal started in: its `tick` runs before any `canContinueToUse`.
    fresh: bool,
}
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Trailing;

pub static MODULE: Module = Module {
    name: "follow_owner",
    flags: MOVE | LOOK,
    retry_on_end: true,
    equip,
    arm,
    can_use,
    start,
    stop,
    systems,
    requires: &[Cap::Tamable, Cap::Locomotion, Cap::Head],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = params::<Params>(p);
    w.entity_mut(e).insert(FollowOwner { p, slot: inst.slot, recalc: 0, fresh: false });
    if w.get::<PlayerWatch>(e).is_none() {
        w.entity_mut(e).insert(PlayerWatch::new(p.start as f32));
    }
}
/// Entering the active zone: the owner may already be far (the radius only tells when it LEAVES).
fn arm(w: &mut World, e: Entity, inst: Inst) {
    w.resource_mut::<Requests>().0.push(Req::Start(e, inst.slot));
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems(owner_left.in_set(slot));
    s.add_systems(follow_run.in_set(slot));
}
/// The owner went beyond the radius of the goal: it is asked at the next tick of the selector.
fn owner_left(ev: Res<Events>, q: Query<(&tame::Owner, &FollowOwner)>, mut reqs: ResMut<Requests>) {
    for (e, id) in ev.player_left.iter() {
        if let Ok((o, f)) = q.get(e)
            && o.0 == Some(crate::targeting::Who::Player(id))
        {
            reqs.0.push(Req::Start(e, f.slot));
        }
    }
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let Some(f) = w.get::<FollowOwner>(e).map(|f| f.p) else { return false };
    let Some(d) = tame::owner_distance_sqr(w, e) else { return false };
    !tame::unable_to_move_to_owner(w, e) && d >= f.start * f.start
}
fn start(w: &mut World, e: Entity, _: Inst) {
    let mut f = w.get_mut::<FollowOwner>(e).unwrap();
    (f.recalc, f.fresh) = (0, true);
    w.entity_mut(e).insert(Trailing);
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Trailing>();
    stop_nav(w, e);
}
/// `FollowOwnerGoal.canContinueToUse` and `tick` (every tick): it ends when the path is done, the order to sit came, or the owner is
/// within the stop distance; else it looks at the owner, and every 5 ticks makes a path to it (or teleports when it is 12 away).
fn follow_run(q: Query<Entity, With<Trailing>>, mut reqs: ResMut<Requests>) {
    for e in &q {
        reqs.0.push(Req::Call(e, follow_step));
    }
}
/// One tick of the goal of one pet (asked at the selector's step, before the paths are made).
fn follow_step(world: &mut World, e: Entity) {
    {
        let Some(f) = world.get::<FollowOwner>(e).copied() else { return };
        let done = !matches!((world.get::<Following>(e), world.get::<NeedsPath>(e)), (Some(_), _) | (_, Some(_)));
        let d = tame::owner_distance_sqr(world, e);
        if !f.fresh && (done || tame::unable_to_move_to_owner(world, e) || !d.is_some_and(|d| d > f.p.stop * f.p.stop)) {
            world.resource_mut::<Requests>().0.push(Req::End(e, f.slot));
            return;
        }
        let (Some(owner), Some(d)) = (tame::owner_player(world, e), d) else { return };
        let far = d >= 144.0;
        if !far {
            crate::brain::look_at(world, e, [owner.x, owner.y + 1.62, owner.z]);
        }
        let mut st = world.get_mut::<FollowOwner>(e).unwrap();
        st.fresh = false;
        st.recalc -= 1;
        if st.recalc <= 0 {
            st.recalc = (10 + 1) / 2;
            if far {
                tame::teleport_to_owner(world, e);
            } else {
                go_to_at(world, e, [owner.x, owner.y, owner.z], f.p.speed, 0);
            }
        }
    }
}
