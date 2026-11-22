//! `AdmireSeen` (the Mob's `AdmireSeen`, `Memories::admire_seen`): a wanted item on the ground, in sight within 14 blocks, and an empty
//! off hand make the animal walk to it for `duration` ticks; it gives up when the item is too far, after `tired` ticks of trying
//! (then no goal for `walk_off` ticks), or when the off hand is filled. Woken by `arm` and by a dropped item (`item_dropped`).
use super::*;
use crate::rules::admire_seen::{self, Step, SPEED};
use crate::{cow::*, goals::*, item};
use horizon_core::geometry::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub items: &'static [&'static str],
    /// Ticks of admiration (119), squared distance of give-up (81), ticks of trying (200), `walk_off` (200), creation's `disabled` (0).
    pub duration: i32,
    pub too_far_sqr: f64,
    pub tired: i32,
    pub walk_off: i32,
    pub disabled: i32,
}
impl Params {
    pub const DEFAULT: Self = Self { items: &[], duration: 119, too_far_sqr: 81.0, tired: 200, walk_off: 200, disabled: 0 };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct SeenParams(pub &'static Params);
/// The goal runs until `until` (own ticks); `reach` counts the ticks of trying.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct SeenAdmire {
    pub until: u64,
    pub reach: Option<u32>,
}
/// No goal before `until` (own ticks): the `walk_off` after giving up, or the creation's `disabled`.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct WalkOff {
    pub until: u64,
}

pub static MODULE: Module = Module {
    name: "admire_seen",
    flags: MOVE | LOOK,
    equip,
    arm,
    can_use,
    start,
    stop,
    tick: Some(tick),
    after_change: Some(after_change),
    requires: &[Cap::Body, Cap::Goals, Cap::Head, Cap::Locomotion],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = params::<Params>(p);
    w.entity_mut(e).insert(SeenParams(p));
    if let Some(until) = admire_seen::disabled_until(now_of(w, e), p.disabled) {
        w.entity_mut(e).insert(WalkOff { until });
    }
}
/// Asked when the off hand changes: a running goal ends with a filled hand, an idle one is asked a tick later.
fn arm(w: &mut World, e: Entity, _: Inst) {
    if w.get::<SeenParams>(e).is_none() {
        return;
    }
    if w.get::<SeenAdmire>(e).is_none() {
        arm_module(w, e, &MODULE, 1);
    } else if equipped(w, e, Slot::Off) != 0 {
        end(w, e, &MODULE);
    }
}
/// The goal ended while a wanted item still lies there: it is asked again (after the `walk_off` if one began), as the Mob's selector does every tick.
fn after_change(w: &mut World, e: Entity, c: Change) {
    if c != Change::Ended || w.get::<SeenParams>(e).is_none() {
        return;
    }
    let now = now_of(w, e);
    let wait = admire_seen::wait_after_end(now, w.get::<WalkOff>(e).map(|x| x.until));
    arm_module(w, e, &MODULE, wait);
}
/// The nearest wanted item in reach and in sight, with its squared distance.
fn nearest_wanted(w: &World, e: Entity, p: &Params) -> Option<(Entity, f64)> {
    let at = w.get::<Pos>(e)?.0;
    let eye = admire_seen::eye_of(at);
    item::items_in(w, admire_seen::search_area(at))
        .into_iter()
        .filter(|&it| item::pickable(w, it) && item::kind_of(w, it).is_some_and(|k| p.items.contains(&k)))
        .filter_map(|it| {
            let q = w.get::<Pos>(it)?.0;
            let d = q.distance_squared(at);
            (admire_seen::in_reach(d) && crate::dragon::fight::line_of_sight(w, eye, q)).then_some((it, d))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
}
/// `canUse`: a wanted item is seen, the off hand is empty, and no walk-off is under way.
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let p = w.get::<SeenParams>(e).unwrap().0;
    let now = now_of(w, e);
    admire_seen::can_start(equipped(w, e, Slot::Off) == 0, now, w.get::<WalkOff>(e).map(|x| x.until)) && nearest_wanted(w, e, p).is_some()
}
fn start(w: &mut World, e: Entity, _: Inst) {
    let p = w.get::<SeenParams>(e).unwrap().0;
    let until = admire_seen::until(now_of(w, e), p.duration);
    w.entity_mut(e).insert(SeenAdmire { until, reach: None });
    if let Some((it, _)) = nearest_wanted(w, e, p) {
        let at = w.get::<Pos>(it).unwrap().0;
        go_to_at(w, e, [at.x, at.y, at.z], SPEED, 1);
    }
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    stop_nav(w, e);
    w.entity_mut(e).remove::<SeenAdmire>();
}
/// The tick, over the animals that run the goal and those an item fell near (only while some animal carries the module).
fn tick(w: &mut World) {
    let dropped: Vec<Vec3> = w.resource::<Events>().item_dropped.iter().filter_map(|i| w.get::<Pos>(i).map(|p| p.0)).collect();
    if !dropped.is_empty() {
        let mut q = w.query_filtered::<(Entity, &Pos), (With<SeenParams>, Without<SeenAdmire>)>();
        let near: Vec<Entity> = q.iter(w).filter(|(_, p)| dropped.iter().any(|s| admire_seen::in_reach(s.distance_squared(p.0)))).map(|(e, _)| e).collect();
        for e in near {
            arm_module(w, e, &MODULE, 1);
        }
    }
    let running: Vec<Entity> = w.query::<(Entity, &SeenAdmire)>().iter(w).map(|(e, _)| e).collect();
    for e in running {
        step(w, e);
    }
}
/// One tick of a running goal: it ends when the time is over, the item is gone or too far, or the animal tired of trying;
/// otherwise it walks to the item when its path is done.
fn step(w: &mut World, e: Entity) {
    let s = *w.get::<SeenAdmire>(e).unwrap();
    let p = w.get::<SeenParams>(e).unwrap().0;
    let now = now_of(w, e);
    let near = nearest_wanted(w, e, p);
    let reach = s.reach.unwrap_or(0);
    match admire_seen::step(now, s.until, reach, p.tired, p.walk_off, near.map(|(_, d)| d), p.too_far_sqr) {
        Step::End => end(w, e, &MODULE),
        Step::GiveUp { walk_off_until } => {
            w.entity_mut(e).insert(WalkOff { until: walk_off_until });
            end(w, e, &MODULE)
        }
        Step::Go => {
            let Some((it, _)) = near else { return };
            if let Some(mut r) = w.get_mut::<SeenAdmire>(e) {
                r.reach = Some(reach + 1);
            }
            if w.get::<Nav>(e).and_then(|n| n.path.as_ref()).is_none_or(|path| path.is_done()) {
                let at = w.get::<Pos>(it).unwrap().0;
                go_to_at(w, e, [at.x, at.y, at.z], SPEED, 1);
            }
        }
    }
}
