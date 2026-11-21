//! `Raider.PathfindToRaidGoal`: a raider with no target, in an active raid that is not over, and not in a village (the registry of the points of
//! interest: more than one section from one), walks toward the raid's centre: whenever its path is done it picks a spot within 15 blocks toward the
//! centre (a quarter turn either side) and goes at speed 1.0. The goal ends when the raid is over or the raider reaches the village.
//! `raids::enlist_raider` asks it to start for the raiders the host enlists. `recruitNearby`: every 20 ticks of its own the raiders within 16 blocks that have no raid join its raid (`raids::recruit_nearby`).
use super::*;
use crate::poi::PoiRegistry;
use crate::rules::move_towards_restriction::home_target;
use crate::raids::{raid_centre, Raider};
use crate::spots::{pick_for, Pick};
use crate::targeting::Target;
use crate::{cow::*, goals::*};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub speed: f64,
}
impl Params {
    pub const DEFAULT: Self = Self { speed: 1.0 };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct ToRaid {
    p: Params,
    pub slot: u8,
    /// `recruitmentTick`: the raider's own time before which it recruits no more.
    recruit_at: u64,
}
/// `RECRUITMENT_SEARCH_TICK_DELAY`.
const RECRUIT_EVERY: u64 = 20;
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Walking;

pub static MODULE: Module = Module { name: "pathfind_to_raid", flags: MOVE, equip, can_use, start, stop, tick: Some(tick), requires: &[Cap::Locomotion], ..Module::DEFAULT };
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(ToRaid { p: *params::<Params>(p), slot: inst.slot, recruit_at: 0 });
}
/// `valid`: an active raid, not over, and the raider is not in a village.
fn valid(w: &World, e: Entity) -> Option<[i32; 3]> {
    let c = raid_centre(w, e)?;
    let p = w.get::<Pos>(e)?.0;
    let here = [(p.x.floor() as i32) >> 4, (p.y.floor() as i32) >> 4, (p.z.floor() as i32) >> 4];
    (!w.resource::<PoiRegistry>().is_village_section(here)).then_some(c)
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    w.get::<Target>(e).is_none() && valid(w, e).is_some()
}
fn start(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).insert(Walking);
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Walking>();
    stop_nav(w, e);
}
fn tick(w: &mut World) {
    crate::goals::tick_state(w, run);
}
#[allow(clippy::type_complexity)]
fn run(q: Query<(Entity, &ToRaid, Has<Following>, Has<NeedsPath>), With<Walking>>, mut cmd: Commands) {
    for (e, t, following, asked) in &q {
        let slot = t.slot;
        let speed = t.p.speed;
        cmd.queue(move |w: &mut World| {
            let Some(c) = valid(w, e) else {
                w.resource_mut::<Requests>().0.push(Req::End(e, slot));
                return;
            };
            let now = crate::cow::now_of(w, e);
            if let Some(mut t) = w.get_mut::<ToRaid>(e)
                && now > t.recruit_at
            {
                t.recruit_at = now + RECRUIT_EVERY;
                crate::raids::recruit_nearby(w, e);
            }
            if following || asked {
                return;
            }
            let to = home_target(c);
            // A test gives the ECS the Mob's draws (`ImposedSpots`, absent in the normal state): the next one, or none yet.
            let at = match w.get_resource_mut::<super::turtle_go_home::ImposedSpots>() {
                Some(mut q) => q.0.pop_front(),
                None => pick_for(w, e, Pick::DefaultTowards { h: 15, v: 4, to, max_rad: std::f64::consts::FRAC_PI_2 }),
            };
            if let Some(at) = at {
                go_to_at(w, e, at, speed, 1);
            }
        });
    }
}
/// The raider is asked to start (a new wave).
pub fn wake(w: &mut World, e: Entity) {
    if let Some(slot) = slot_of(w, e, &MODULE) {
        w.resource_mut::<Requests>().0.push(Req::Start(e, slot));
    }
}
