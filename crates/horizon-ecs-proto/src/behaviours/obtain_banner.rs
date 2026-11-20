//! `Raider.ObtainRaidLeaderBannerGoal`: in an active raid that is not over, a raider that can lead (the species says it) and whose wave has no
//! living leader walks to the ominous banner lying within reach (an item entity of the banner, 16 blocks horizontally and 8 vertically) at 1.15 and picks
//! it up on arrival (`item::pick_up`).  Move flag. Asked each 40 ticks while its raid runs (armed at the wave's spawn, `raids::spawn_wave`) and when the raid changes phase. Not done: the 600-tick memory of a banner with no path to it, the banner put on the head (`wear_banner`), the leader of the wave is not set again.
use super::*;
use crate::raids::{has_active_raid, raid_of, Raider};
use crate::{cow::*, goals::*, item};
use horizon_core::geometry::{Aabb, Vec3};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub speed: f64,
}
impl Params {
    pub const DEFAULT: Self = Self { speed: 1.15 };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Fetcher {
    p: Params,
    slot: u8,
    /// The banner item being fetched.
    item: Option<Entity>,
}
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Fetching;

pub static MODULE: Module = Module { name: "obtain_banner", flags: MOVE, equip, fired, can_use, start, stop, tick: Some(tick), requires: &[Cap::Locomotion], ..Module::DEFAULT };
fn equip(w: &mut World, e: Entity, inst: Inst, p: &(dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Fetcher { p: *params::<Params>(p), slot: inst.slot, item: None });
}
/// An item entity of the ominous banner within reach of the raider.
fn banner_near(w: &World, e: Entity) -> Option<Entity> {
    let p = w.get::<Pos>(e)?.0;
    let area = Aabb::new(Vec3::new(p.x - 16.0, p.y - 8.0, p.z - 16.0), Vec3::new(p.x + 16.0, p.y + 8.0, p.z + 16.0));
    item::items_in(w, area).into_iter().find(|&i| item::pickable(w, i) && is_ominous(w, i))
}
/// The poll of the goal while its raid runs (a banner falls when a leader dies): one ask each 40 ticks.
fn fired(w: &mut World, e: Entity, inst: Inst) -> bool {
    if has_active_raid(w, e) {
        arm(w, e, inst.slot, 40);
    }
    true
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    if !has_active_raid(w, e) || raid_of(w, e).is_some_and(|r| r.is_over()) {
        return false;
    }
    if w.get::<Raider>(e).is_none_or(|r| r.leader) {
        return false;
    }
    let Some(i) = banner_near(w, e) else { return false };
    let Some(at) = w.get::<Pos>(i).map(|p| p.0) else { return false };
    w.get_mut::<Fetcher>(e).unwrap().item = Some(i);
    let speed = w.get::<Fetcher>(e).unwrap().p.speed;
    go_to_at(w, e, [at.x, at.y, at.z], speed, 1);
    true
}
/// `Raid.getOminousBannerInstance`: the stack carries the ominous banner component.
fn is_ominous(w: &World, i: Entity) -> bool {
    w.get::<item::Item>(i).is_some_and(|it| it.stack.data().is_some_and(|d| d.ominous_banner))
}
fn start(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).insert(Fetching);
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Fetching>();
}
fn tick(w: &mut World) {
    crate::goals::tick_state(w, run);
}
fn run(q: Query<(Entity, &Fetcher, Has<Following>, Has<NeedsPath>), With<Fetching>>, mut cmd: Commands) {
    for (e, f, following, asked) in &q {
        let (slot, it) = (f.slot, f.item);
        cmd.queue(move |w: &mut World| {
            let Some(it) = it.filter(|&i| w.get_entity(i).is_ok()) else {
                w.resource_mut::<Requests>().0.push(Req::End(e, slot));
                return;
            };
            if !has_active_raid(w, e) {
                w.resource_mut::<Requests>().0.push(Req::End(e, slot));
                return;
            }
            if following || asked {
                return;
            }
            // The walk is over: the banner is taken when it is within a block and a half, as an item is picked up.
            if let (Some(a), Some(b)) = (w.get::<Pos>(e).map(|p| p.0), w.get::<Pos>(it).map(|p| p.0)) {
                if (a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2) <= 2.25 {
                    let _ = item::pick_up(w, e, it, 1);
                }
            }
            w.resource_mut::<Requests>().0.push(Req::End(e, slot));
        });
    }
}
