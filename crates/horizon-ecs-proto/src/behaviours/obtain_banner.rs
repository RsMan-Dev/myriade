//! `Raider.ObtainRaidLeaderBannerGoal`: in an active raid that is not over, a raider that can lead (the species says it) and whose wave has no
//! living leader walks to the ominous banner lying within reach (an item entity of the banner, 16 blocks horizontally and 8 vertically) at 1.15 and picks
//! it up on arrival (`item::pick_up`).  Move flag. Woken by the event of a dropped item near it (`Events.item_dropped`) and when the raid changes phase: no poll. The pickup is `Raider.pickUpItem`: the banner goes on the head, the wave's captain is named (`Raid.setLeader`, told to the host by `raider_leads`) and the raider becomes a patrol leader. Not done: the 600-tick memory of a banner with no path to it.
use super::*;
use crate::raids::{has_active_raid, raid_of, Raider};
use crate::rules::obtain_banner::{banner_area, picked_up, REACH_H};
use crate::{cow::*, goals::*, item};
use horizon_items::equipment::HEAD;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub speed: f64,
    /// `Raider.canBeLeader` (false for the ravager and the witch); `PatrollingMonster.finalizeSpawn` reads it for its leader draw.
    pub can_lead: bool,
}
impl Params {
    pub const DEFAULT: Self = Self { speed: 1.15, can_lead: true };
    pub const CANNOT_LEAD: Self = Self { can_lead: false, ..Self::DEFAULT };
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
    if !can_be_leader(w, e) || wears_banner(w, e) || wave_has_leader(w, e) {
        return false;
    }
    let Some(i) = banner_near(w, e) else { return false };
    let Some(at) = w.get::<Pos>(i).map(|p| p.0) else { return false };
    w.get_mut::<Fetcher>(e).unwrap().item = Some(i);
    let speed = w.get::<Fetcher>(e).unwrap().p.speed;
    go_to_at(w, e, [at.x, at.y, at.z], speed, 1);
    true
}
/// `Raider.canBeLeader`: what the species' `obtain_banner` entry says (a raider with none cannot).
pub fn can_be_leader(w: &World, e: Entity) -> bool {
    w.get::<Fetcher>(e).is_some_and(|f| f.p.can_lead)
}
/// The head holds the ominous banner.
fn wears_banner(w: &World, e: Entity) -> bool {
    crate::equip_stack::get(w, e, HEAD).is_some_and(|s| s.data().is_some_and(|d| d.ominous_banner))
}
/// `Raid.getLeader(wave)` is alive: a raider of the raider's raid and wave is the captain. Reads the raiders only, and only once a banner is near.
fn wave_has_leader(w: &World, e: Entity) -> bool {
    let Some(me) = w.get::<Raider>(e) else { return false };
    let Some(raid) = me.raid else { return false };
    let wave = me.wave;
    let Some(q) = w.try_query::<(&Raider, &Health)>() else { return false };
    q.iter_manual(w).any(|(r, h)| r.raid == Some(raid) && r.wave == wave && r.leader && h.0 > 0.0)
}
/// `Raider.pickUpItem` of the ominous banner: what the head wore may drop (`DropChances`), the head takes the banner, `Raid.setLeader` (the
/// wave's captain, the banner on the head, drop chance 2.0) and `setPatrolLeader`. False when the wave has a living captain already.
pub fn take_banner(w: &mut World, e: Entity, it: Entity) -> bool {
    use crate::equipment_drop::chance_of;
    if !has_active_raid(w, e) || wave_has_leader(w, e) || !is_ominous(w, it) {
        return false;
    }
    let Some((raid, wave)) = w.get::<Raider>(e).and_then(|r| Some((r.raid?, r.wave))) else { return false };
    let Some(at) = w.get::<Pos>(e).map(|p| p.0) else { return false };
    if let Some(old) = crate::equip_stack::get(w, e, HEAD) {
        let roll = with_rng(w, e, |r| r.next_f64() as f32);
        if f64::from((roll - 0.1).max(0.0)) < f64::from(chance_of(w, e, Slot::Head)) {
            let seed = with_rng(w, e, |r| r.next_u32());
            item::spawn_at_location(w, old, at, &mut horizon_core::random::LegacyRandom::new(i64::from(seed)));
        }
    }
    let count = w.get::<item::Item>(it).map_or(1, |i| i.stack.count);
    if item::pick_up(w, e, it, count).is_none() {
        return false;
    }
    wear_banner(w, e);
    if let Some(mut r) = w.get_mut::<Raider>(e) {
        r.leader = true;
    }
    crate::raids::set_patrol_leader(w, e, true);
    if let Some(id) = crate::bridge::ids::id_of(w, e) {
        w.resource_mut::<Events>().host.raider_leads(raid, id, wave);
    }
    true
}
/// The head takes the ominous banner and its drop chance 2.0 (`Raid.setLeader`, `PatrollingMonster.finalizeSpawn`).
pub fn wear_banner(w: &mut World, e: Entity) {
    use crate::equipment_drop::{index, DropChances, DEFAULT};
    crate::equip_stack::set(w, e, HEAD, Some(myriade_engine_lib::mobs::raider::ominous_banner()));
    let mut chances = w.get::<DropChances>(e).copied().unwrap_or(DropChances([DEFAULT; 8]));
    chances.0[index(Slot::Head)] = 2.0;
    w.entity_mut(e).insert(chances);
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
                    take_banner(w, e, it);
                }
            }
            w.resource_mut::<Requests>().0.push(Req::End(e, slot));
        });
    }
}
