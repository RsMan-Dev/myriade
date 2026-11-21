//! `StayCloseToTarget(speed, start, stop)` (the allay's `stay_close_to_owner`, `Mob` l.~395-420): a tamed animal whose owner is `start`
//! blocks away or more walks to it until the Manhattan distance in blocks is at most `stop`. No sitting check: the allay does not sit.
//! What wakes it: the owner leaving the `start` radius (`PlayerWatch`); while it runs, a system visits it each tick.
use super::*;
use crate::{cow::*, goals::*, tame};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub speed: f64,
    pub start: f64,
    pub stop: i32,
}
impl Params {
    pub const DEFAULT: Self = Self { speed: 2.25, start: 16.0, stop: 4 };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct StayClose {
    p: &'static Params,
    slot: u8,
    recalc: i32,
    /// The tick the goal started in: its `tick` runs before any `canContinueToUse`.
    fresh: bool,
}
#[derive(Component)]
#[component(storage = "SparseSet")]
pub struct Closing;

pub static MODULE: Module = Module {
    name: "stay_close_to_owner",
    flags: MOVE,
    retry_on_end: true,
    equip,
    arm,
    can_use,
    start,
    stop,
    tick: Some(tick),
    requires: &[Cap::Tamable, Cap::Locomotion],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = params::<Params>(p);
    w.entity_mut(e).insert(StayClose { p, slot: inst.slot, recalc: 0, fresh: false });
    if w.get::<PlayerWatch>(e).is_none() {
        w.entity_mut(e).insert(PlayerWatch::new(p.start as f32));
    }
}
/// Entering the active zone: the owner may already be far (the radius only tells when it LEAVES).
fn arm(w: &mut World, e: Entity, inst: Inst) {
    w.resource_mut::<Requests>().0.push(Req::Start(e, inst.slot));
}
fn tick(w: &mut World) {
    tick_state(w, owner_left);
    tick_state(w, stay_run);
}
/// The owner went beyond the radius of the goal: it is asked at the next tick of the selector.
fn owner_left(ev: Res<Events>, q: Query<(&tame::Owner, &StayClose)>, mut reqs: ResMut<Requests>) {
    for (e, id) in ev.player_left.iter() {
        if let Ok((o, s)) = q.get(e)
            && o.0 == Some(crate::targeting::Who::Player(id))
        {
            reqs.0.push(Req::Start(e, s.slot));
        }
    }
}
/// `canUse`: an owner that is online and at `start` blocks or more.
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let Some(s) = w.get::<StayClose>(e).map(|s| s.p) else { return false };
    let Some(d) = tame::owner_distance_sqr(w, e) else { return false };
    d >= s.start * s.start
}
fn start(w: &mut World, e: Entity, _: Inst) {
    let mut s = w.get_mut::<StayClose>(e).unwrap();
    (s.recalc, s.fresh) = (0, true);
    w.entity_mut(e).insert(Closing);
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Closing>();
    stop_nav(w, e);
}
fn stay_run(q: Query<Entity, With<Closing>>, mut reqs: ResMut<Requests>) {
    for e in &q {
        reqs.0.push(Req::Call(e, stay_step));
    }
}
/// One tick of the goal of one animal. `canContinueToUse`: it ends when the owner is gone or within `stop` (Manhattan distance in
/// blocks), not when the path is done; else it makes a path to the owner every 10 ticks, or when the last path is finished.
fn stay_step(world: &mut World, e: Entity) {
    let Some(s) = world.get::<StayClose>(e).copied() else { return };
    let owner = tame::owner_player(world, e);
    let me = world.get::<Pos>(e).map(|p| p.0);
    let near = match (owner, me) {
        (Some(o), Some(m)) => (o.x.floor() - m.x.floor()).abs() + (o.y.floor() - m.y.floor()).abs() + (o.z.floor() - m.z.floor()).abs() <= f64::from(s.p.stop),
        _ => false,
    };
    if !s.fresh && (owner.is_none() || near) {
        world.resource_mut::<Requests>().0.push(Req::End(e, s.slot));
        return;
    }
    let Some(owner) = owner else { return };
    let busy = world.get::<Following>(e).is_some() || world.get::<NeedsPath>(e).is_some();
    let mut st = world.get_mut::<StayClose>(e).unwrap();
    st.fresh = false;
    if !busy {
        st.recalc = 0;
    }
    st.recalc -= 1;
    if st.recalc <= 0 {
        st.recalc = 10;
        go_to_at(world, e, [owner.x, owner.y, owner.z], s.p.speed, 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cow::{species, spawn_species, Herd};
    use crate::species::Config;

    /// One tamed creature whose only goal is this one, owned by player 1, who stands `dx` blocks away.
    fn world(dx: f64) -> (Herd, Entity) {
        let mut h = Herd::with_slab_awake(60, true, false);
        let mut sp = *species();
        let entries: &'static [Entry] = Box::leak(vec![Entry::new(&MODULE, 3, &Params::DEFAULT)].into_boxed_slice());
        sp.goals = Config { entries };
        let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 70);
        tame::set_owner(&mut h.world, e, crate::targeting::Who::Player(1));
        // The goal ends within `stop`, a tick later the body has moved on: the scene fixes the pace (a cow's 0.2), so the species of the bench (the frog's attribute is 1.0, over a block per tick) does not decide the result.
        // The smooth control on land (the frog) scales the speed by its `outside` factor: the scene keeps the pace of a cow's 0.2.
        let outside = h.world.get::<crate::cow::Drive>(e).and_then(|d| d.smooth_land).map_or(1.0, |(_, o)| f64::from(o));
        h.world.get_mut::<Body>(e).unwrap().speed = 0.2 / outside;
        h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5 + dx, y: 1.0, z: 0.5, main: 0, off: 0, ..Default::default() });
        (h, e)
    }
    /// The Manhattan distance in blocks between the creature and its owner (the measure of `stop`).
    fn gap(h: &Herd, e: Entity) -> f64 {
        let p = h.world.get::<Pos>(e).unwrap().0;
        let o = h.world.resource::<Online>().get(1).unwrap();
        (o.x.floor() - p.x.floor()).abs() + (o.y.floor() - p.y.floor()).abs() + (o.z.floor() - p.z.floor()).abs()
    }
    #[test]
    fn it_walks_to_an_owner_16_blocks_away_and_stops_within_4() {
        if crate::cow::skip_unless_walker("stay close to owner") || crate::cow::skip_unless_caps(&[Cap::Tamable], "stay close to owner") {
            return;
        }
        let (mut h, e) = world(20.0);
        for _ in 0..600 {
            h.tick();
        }
        assert!(gap(&h, e) <= 4.0, "gap {}", gap(&h, e));
        for _ in 0..20 {
            h.tick();
        }
        assert!(gap(&h, e) <= 4.0, "gap after 20 more ticks {}", gap(&h, e));
        assert!(h.world.get::<Closing>(e).is_none());
    }
    #[test]
    fn it_stays_put_when_the_owner_is_near() {
        if crate::cow::skip_unless_walker("stay close to owner") || crate::cow::skip_unless_caps(&[Cap::Tamable], "stay close to owner") {
            return;
        }
        let (mut h, e) = world(10.0);
        let x0 = h.world.get::<Pos>(e).unwrap().0.x;
        for _ in 0..100 {
            h.tick();
            assert!(h.world.get::<Closing>(e).is_none(), "started at 10 blocks");
        }
        let x = h.world.get::<Pos>(e).unwrap().0.x;
        assert!((x - x0).abs() < 0.5, "moved from {x0} to {x}");
    }
}
