//! `Raider.ObtainRaidLeaderBannerGoal`: in an active raid that is not over, a raider that can lead (the species says it) and whose wave has no
//! living leader walks to the ominous banner lying within reach (an item entity of the banner, 16 blocks horizontally and 8 vertically) at 1.15 and picks
//! it up on arrival (`item::pick_up`).  Move flag. Woken by the event of a dropped item near it (`Events.item_dropped`) and when the raid changes phase: no poll. Not done: the 600-tick memory of a banner with no path to it, the banner put on the head (`wear_banner`), the leader of the wave is not set again.
use super::*;
use crate::raids::{has_active_raid, raid_of, Raider};
use crate::rules::obtain_banner::{banner_area, picked_up, REACH_H};
use crate::{cow::*, goals::*, item};

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

pub static MODULE: Module = Module { name: "obtain_banner", flags: MOVE, equip, fired, systems, can_use, start, stop, requires: &[Cap::Locomotion], ..Module::DEFAULT };
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Fetcher { p: *params::<Params>(p), slot: inst.slot, item: None });
}
/// An item entity of the ominous banner within reach of the raider.
fn banner_near(w: &World, e: Entity) -> Option<Entity> {
    let p = w.get::<Pos>(e)?.0;
    let area = banner_area(p);
    item::items_in(w, area).into_iter().find(|&i| item::pickable(w, i) && is_ominous(w, i))
}
/// No poll: a banner that falls (an item dropped, `Events.item_dropped`) wakes the raiders of a running raid around it, and the raid's phase change wakes them all.
fn fired(_: &mut World, _: Entity, _: Inst) -> bool {
    true
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems((wake_on_banner, run).in_set(slot));
}
fn wake_on_banner(ev: Res<Events>, items: Query<&Pos, With<item::Item>>, idx: Res<Index>, q: Query<&Fetcher>, mut reqs: ResMut<Requests>) {
    for i in ev.item_dropped.iter() {
        let Ok(p) = items.get(i) else { continue };
        idx.for_radius(p.0.x, p.0.z, REACH_H, |c| {
            if let Ok(f) = q.get(c) {
                reqs.0.push(Req::Start(c, f.slot));
            }
        });
    }
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
                if picked_up(a, b) {
                    let _ = item::pick_up(w, e, it, 1);
                }
            }
            w.resource_mut::<Requests>().0.push(Req::End(e, slot));
        });
    }
}
