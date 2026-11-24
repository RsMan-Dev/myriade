//! The raids' view in the ECS. The state of a raid (waves, boss bar, victory, defeat, heroes) lives at the host (`myriade_engine_lib::raid`);
//! this world keeps only what the goals read: per raid its centre and whether it is active, over, lost (`RaidView`, written by the host
//! through `set_view`, which wakes the raiders when the phase changes), the difficulty, and the components `Raider`, `Patrol`,
//! `Celebrating`, `Aggressive`. A raider's death is told to the host (`HostEvent::RaiderDied`).
use crate::goals::{Req, Requests};
use bevy_ecs::prelude::*;
use std::collections::HashMap;

/// A raider: which raid it is in, the wave it came with, whether it leads the wave.
#[derive(Component, Clone, Copy, Debug)]
pub struct Raider {
    pub raid: Option<i32>,
    pub wave: i32,
    pub leader: bool,
}
/// What the goals read of a raid, as the host last wrote it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RaidView {
    pub center: [i32; 3],
    pub active: bool,
    pub over: bool,
    pub loss: bool,
    /// `Raid.getGroupsSpawned`: the wave a creature that joins the raid takes.
    pub wave: i32,
}
impl RaidView {
    pub fn is_over(&self) -> bool {
        self.over
    }
    pub fn is_loss(&self) -> bool {
        self.loss
    }
}
#[derive(Resource)]
pub struct RaidState {
    pub views: HashMap<i32, RaidView>,
    pub difficulty: i32,
}
impl Default for RaidState {
    fn default() -> Self {
        Self { views: HashMap::new(), difficulty: 2 }
    }
}

/// The host's view of raid `id` (None: the raid is gone). The raiders of the raid are asked once when its view changes
/// (it appears, starts, is won or lost), never every tick.
pub fn set_view(w: &mut World, id: i32, view: Option<RaidView>) {
    let mut st = w.get_resource_or_insert_with(RaidState::default);
    let before = st.views.get(&id).copied();
    match view {
        Some(v) => st.views.insert(id, v),
        None => st.views.remove(&id),
    };
    if before != view {
        wake_raiders(w, id);
    }
}
/// Every raider of the raid asks its raid goals (`Req::Start`: the goal asks its own `can_use`).
pub fn wake_raiders(w: &mut World, id: i32) {
    use crate::behaviours as b;
    let mut q = w.query::<(Entity, &Raider)>();
    let ids: Vec<Entity> = q.iter(w).filter(|(_, r)| r.raid == Some(id)).map(|(e, _)| e).collect();
    for e in ids {
        for m in [&b::pathfind_to_raid::MODULE, &b::raider_open_door::MODULE, &b::break_door::MODULE, &b::celebration::MODULE, &b::obtain_banner::MODULE, &b::raid_move_through_village::MODULE] {
            if let Some(slot) = crate::goals::slot_of(w, e, m) {
                w.resource_mut::<Requests>().0.push(Req::Start(e, slot));
            }
        }
    }
}
/// The raid of the raider, when it has one the host last described.
pub fn raid_of(w: &World, e: Entity) -> Option<&RaidView> {
    let id = w.get::<Raider>(e)?.raid?;
    w.get_resource::<RaidState>()?.views.get(&id)
}
/// `Raider.hasActiveRaid`: the raider's raid is active and not over.
pub fn has_active_raid(w: &World, e: Entity) -> bool {
    raid_of(w, e).is_some_and(|r| r.active && !r.is_over())
}
/// The level's difficulty (0 peaceful .. 3 hard): the raids' own, 2 when none was set.
pub fn difficulty(w: &World) -> i32 {
    w.get_resource::<RaidState>().map_or(2, |s| s.difficulty)
}
/// `PatrollingMonster`: the patrol of a raider that is not in a raid (a leader, where it walks to, until when a failed hop keeps it still).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Patrol {
    pub leader: bool,
    pub patrolling: bool,
    pub target: Option<[i32; 3]>,
    pub cooldown_until: u64,
}
/// `PatrollingMonster.setPatrolLeader`: the leader flag and `patrolling = true`; the raider's `Patrol` is made if it has none.
pub fn set_patrol_leader(w: &mut World, e: Entity, on: bool) {
    if let Some(mut p) = w.get_mut::<Patrol>(e) {
        p.leader = on;
        p.patrolling = true;
    } else {
        w.entity_mut(e).insert(Patrol { leader: on, patrolling: true, ..Default::default() });
    }
}
/// A raider shouted at by its patrol, or one that attacks for good (`Raider.hasAggressiveTarget`).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Aggressive;
/// A raider that celebrates (`Raider.setCelebrating`).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet", on_insert = crate::bridge::view::touched, on_remove = crate::bridge::view::touched)]
pub struct Celebrating;
/// A creature joins a raid: its `Raider`, then what the game's step does for an illager near a raid. `Vindicator.customServerAiStep`
/// opens doors while `raided` (game code: a raid within 96 blocks), written once here at the join, not each step (the raid does not move; a
/// vindicator that walks off 96 blocks keeps it: see `suivi/ecs-ecarts.md`); then the raider is asked to walk to the raid.
pub fn enlist(w: &mut World, e: Entity, r: Raider, opens_doors: bool) {
    w.entity_mut(e).insert(r);
    if opens_doors && let Some(mut n) = w.get_mut::<crate::cow::NavParams>(e) {
        n.can_open_doors = true;
    }
    crate::behaviours::pathfind_to_raid::wake(w, e);
}
/// A creature the host made joins raid `raid` in wave `wave` (`Raid.joinRaid`): `enlist`, the doors opened as its `BreakDoor` says.
pub fn enlist_raider(w: &mut World, e: Entity, raid: i32, wave: i32, leader: bool) {
    let opens = w.get::<crate::behaviours::break_door::BreakDoor>(e).is_some_and(|b| b.p.raid);
    enlist(w, e, Raider { raid: Some(raid), wave, leader }, opens);
    // `Raid.joinRaid`: `setCanJoinRaid(true)`, `setTicksOutsideRaid(0)`.
    set_can_join_raid(w, e, true);
    w.entity_mut(e).remove::<RaidOutside>();
}
/// `Raider.setCanJoinRaid`, saved as `CanJoinRaid` (false when the entity is not `Saved`).
pub fn set_can_join_raid(w: &mut World, e: Entity, on: bool) -> bool {
    let mut patch = horizon_core::nbt::Compound::new();
    patch.insert("CanJoinRaid".into(), horizon_core::nbt::Tag::Byte(i8::from(on)));
    crate::nbt::merge(w, e, &patch)
}
/// `Raider.canJoinRaid`.
pub fn can_join_raid(w: &World, e: Entity) -> bool {
    matches!(crate::nbt::get(w, e, "CanJoinRaid"), Some(horizon_core::nbt::Tag::Byte(v)) if v != 0)
}
/// `Raider.ticksOutsideRaid`: what `Raid.updateRaiders` counted of the ticks the raider spent idle outside the village.
#[derive(Component, Clone, Copy, Debug, Default)]
#[component(storage = "SparseSet")]
pub struct RaidOutside(pub i32);
/// A raider that came from a save with a `RaidId`: the host's raid does not know it yet. Told once it has a network id.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Unannounced;
/// `Raids.getNearbyRaid`'s radius, squared (96 blocks).
pub const VALID_RAID_RADIUS_SQR: f64 = 9216.0;
/// `Raids.canJoinRaid`: the idle time past which a raider no longer joins.
pub const MAX_NO_ACTION_TIME: i32 = 2400;
/// The ticks of a raider's raid at the host: `Raider.ticksOutsideRaid` written by the host's `Raid.updateRaiders`.
pub fn set_ticks_outside(w: &mut World, e: Entity, ticks: i32) {
    if ticks == 0 {
        w.entity_mut(e).remove::<RaidOutside>();
    } else {
        w.entity_mut(e).insert(RaidOutside(ticks));
    }
}
/// `Raider.getTicksOutsideRaid`.
pub fn ticks_outside(w: &World, e: Entity) -> i32 {
    w.get::<RaidOutside>(e).map_or(0, |t| t.0)
}
/// `Raider.setCurrentRaid(null)` (the raid dropped it): no raid, no count outside.
pub fn leave_raid(w: &mut World, e: Entity) {
    if let Some(mut r) = w.get_mut::<Raider>(e) {
        r.raid = None;
    }
    w.entity_mut(e).remove::<RaidOutside>();
}
/// `ServerLevel.getRaidAt`: the nearest active raid within 96 blocks of `at`.
pub fn raid_at(w: &World, at: [i32; 3]) -> Option<(i32, RaidView)> {
    let st = w.get_resource::<RaidState>()?;
    let mut best: Option<(i32, RaidView, f64)> = None;
    for (&id, v) in &st.views {
        let d: f64 = (0..3).map(|i| (f64::from(v.center[i]) - f64::from(at[i])).powi(2)).sum();
        if v.active && d < best.map_or(VALID_RAID_RADIUS_SQR, |b| b.2) {
            best = Some((id, *v, d));
        }
    }
    best.map(|(id, v, _)| (id, v))
}

pub fn register(s: &mut bevy_ecs::schedule::Schedule) {
    s.add_systems((announce.run_if(|w: &World| crate::goals::sparse_any::<Unannounced>(w)), step.run_if(any_active)));
}
fn any_active(w: &World) -> bool {
    w.get_resource::<RaidState>().is_some_and(|s| s.views.values().any(|v| v.active))
}
/// The loaded raiders the host's raid has not heard of: `RaiderJoined` with `loaded` (`readAdditionalSaveData`: `addWaveMob` without
/// counting the health, and the wave's leader, `PatrolLeader`). A raider with no network id yet waits.
fn announce(w: &mut World) {
    let mut q = w.query_filtered::<(Entity, &Raider, Option<&Patrol>), With<Unannounced>>();
    let list: Vec<(Entity, Raider, bool)> = q.iter(w).map(|(e, r, p)| (e, *r, p.is_some_and(|p| p.leader))).collect();
    for (e, r, patrol_leader) in list {
        let Some(id) = crate::bridge::ids::id_of(w, e) else { continue };
        w.entity_mut(e).remove::<Unannounced>();
        let Some(raid) = r.raid else { continue };
        let leader = r.leader || patrol_leader;
        if let Some(mut m) = w.get_mut::<Raider>(e) {
            m.leader = leader;
        }
        w.resource_mut::<crate::goals::Events>().host.raider_joined(raid, id, r.wave, leader, true);
    }
}
/// `Raider.aiStep` while a raid is active: a raider fighting a player in a raid is never idle (`noActionTime = 0`), and every 20 ticks
/// a raider with no raid that can join one joins the raid it stands in (`Raid.joinRaid` at the raid's wave). Runs only while the host
/// holds an active raid view, over the raiders (the `Raider` component), never the whole world.
fn step(w: &mut World) {
    use crate::targeting::{Target, Who};
    let mut fighting = w.query::<(Entity, &Raider, &Target)>();
    let busy: Vec<Entity> = fighting
        .iter(w)
        .filter(|(_, r, t)| r.raid.is_some() && match t.who {
            Who::Player(_) => true,
            Who::Entity(x) => w.get::<RaidBait>(x).is_some(),
        })
        .map(|(e, ..)| e)
        .collect();
    for e in busy {
        crate::despawn::touch(w, e);
    }
    if w.resource::<crate::cow::Clock>().0 % 20 != 0 {
        return;
    }
    let mut free = w.query::<(Entity, &Raider, &crate::cow::Pos)>();
    let cands: Vec<(Entity, [i32; 3])> = free
        .iter(w)
        .filter(|(_, r, _)| r.raid.is_none())
        .map(|(e, _, p)| (e, [p.0.x.floor() as i32, p.0.y.floor() as i32, p.0.z.floor() as i32]))
        .collect();
    for (e, at) in cands {
        if let Some((raid, view)) = raid_at(w, at) {
            join_raid(w, e, raid, view.wave);
        }
    }
}
/// A creature a raider fighting it never idles for (`Raider.aiStep`: a target that is an iron golem sets `noActionTime = 0`), as the player is.
#[derive(Component, Clone, Copy, Debug, Default)]
#[component(storage = "SparseSet")]
pub struct RaidBait;
pub static BAIT: crate::rules::Rule = crate::rules::Rule { name: "raid_bait", equip: equip_bait, attrs: crate::rules::no_attrs, interact: crate::rules::no_interact };
fn equip_bait(w: &mut World, e: Entity, _: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(RaidBait);
}
/// A raider that, born by natural spawning, can not join a raid (`Raider.finalizeSpawn`: the witch).
#[derive(Component, Clone, Copy, Debug, Default)]
#[component(storage = "SparseSet")]
pub struct NoRaidWhenNatural;
pub static NO_RAID_WHEN_NATURAL: crate::rules::Rule =
    crate::rules::Rule { name: "no_raid_when_natural", equip: equip_no_raid, attrs: crate::rules::no_attrs, interact: crate::rules::no_interact };
fn equip_no_raid(w: &mut World, e: Entity, _: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(NoRaidWhenNatural);
}
/// `Raider.finalizeSpawn` then `PatrollingMonster.finalizeSpawn` (the species' own gear and `Mob`'s draws come around it): `canJoinRaid`; a
/// spawn that is not a patrol, an event or a structure leads one time in 17 when the species can; a leader wears the banner; a patrol patrols.
pub fn finalize_raider(w: &mut World, e: Entity, reason: crate::spawn_reason::SpawnReason, r: &mut impl horizon_core::random::RandomSource) {
    use crate::spawn_reason::SpawnReason as Why;
    let bits = w.get::<crate::cow::SpeciesId>(e).map_or(0, |id| crate::cow::tags_of_id(*id));
    if bits & crate::cow::tag::RAIDER == 0 {
        return;
    }
    set_can_join_raid(w, e, !(reason == Why::Natural && w.get::<NoRaidWhenNatural>(e).is_some()));
    let roll = !matches!(reason, Why::Patrol | Why::Event | Why::Structure);
    if roll && r.next_float() < 0.06 && crate::behaviours::obtain_banner::can_be_leader(w, e) {
        let p = w.get::<Patrol>(e).copied().unwrap_or_default();
        w.entity_mut(e).insert(Patrol { leader: true, ..p });
    }
    if w.get::<Patrol>(e).is_some_and(|p| p.leader) {
        crate::behaviours::obtain_banner::wear_banner(w, e);
    }
    if reason == Why::Patrol {
        let mut p = w.get::<Patrol>(e).copied().unwrap_or_default();
        p.patrolling = true;
        w.entity_mut(e).insert(p);
    }
}
/// `Raids.canJoinRaid`: alive, `canJoinRaid`, and idle for no more than `MAX_NO_ACTION_TIME`.
pub fn can_join(w: &World, e: Entity) -> bool {
    w.get::<crate::cow::Health>(e).is_some_and(|h| h.0 > 0.0) && can_join_raid(w, e) && crate::despawn::idle_for(w, e).unwrap_or(0) <= MAX_NO_ACTION_TIME
}
/// `Raid.joinRaid(level, groupsSpawned, raider, null, true)` for a raider that has no raid and may join one: it is enlisted at the raid's
/// wave and the host is told. False when it cannot join or has no network id.
pub fn join_raid(w: &mut World, e: Entity, raid: i32, wave: i32) -> bool {
    if !can_join(w, e) {
        return false;
    }
    let Some(id) = crate::bridge::ids::id_of(w, e) else { return false };
    enlist_raider(w, e, raid, wave, false);
    w.resource_mut::<crate::goals::Events>().host.raider_joined(raid, id, wave, false, false);
    true
}
/// `PathfindToRaidGoal.recruitNearby`: the raiders without a raid within 16 blocks of `e`'s box that can join one join `e`'s raid, when
/// that raid is active. One bounded spatial query, run by the goal every 20 ticks of its own.
pub fn recruit_nearby(w: &mut World, e: Entity) {
    const REACH: f64 = 16.0;
    let Some(raid) = w.get::<Raider>(e).and_then(|r| r.raid) else { return };
    let Some(view) = w.get_resource::<RaidState>().and_then(|s| s.views.get(&raid)).copied().filter(|v| v.active && !v.is_over()) else { return };
    let area = crate::area::reach_box(w, e, REACH);
    for c in crate::area::creatures_in(w, area, e) {
        if w.get::<Raider>(c).is_some() && !has_active_raid(w, c) {
            join_raid(w, c, raid, view.wave);
        }
    }
}
/// `Raider.getCurrentRaid`'s centre, when the raid is active and not over: where `PathfindToRaid` walks to.
pub fn raid_centre(w: &World, e: Entity) -> Option<[i32; 3]> {
    let id = w.get::<Raider>(e)?.raid?;
    let r = w.get_resource::<RaidState>()?.views.get(&id)?;
    (r.active && !r.is_over()).then_some(r.center)
}
