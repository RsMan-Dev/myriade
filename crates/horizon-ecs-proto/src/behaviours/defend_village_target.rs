//! `IronGolem` `DefendVillageTargetGoal` (target flag): a golem takes as target the player whom a villager near it holds in bad
//! reputation (`Gossips::reputation` at or below -100). The box is the golem's, inflated (10, 8, 10); the villagers in it, within 64
//! blocks, that the golem sees count; the player must be in the box, within 64 blocks scaled by its visibility, not creative or spectator,
//! and seen. A golem a player built takes none. The nearest such player (then the lowest id) is taken. The `Mob` asks every tick; the ecs asks on the events that can change the answer, never on a
//! timer: the golem's creation, a gossip that makes a villager hold a player in bad reputation (`wake_golems`, from `gossip::add` and `meet`),
//! and a player's move near it (`Online.seen`). Not an event: a villager or the golem walking into reach while the player stands still.
use super::gossip::Gossips;
use super::*;
use crate::cow::*;
use crate::goals::*;
use crate::hostile::PlayerState;
use crate::targeting::{self, Target, Who};
use horizon_core::geometry::Vec3;
use crate::rules::defend_village_target::nearest_player;
use crate::behaviours::hurt_by::PlayerCreated;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The reputation at or below which a villager's word turns the golem (`DefendVillageTargetGoal`: -100).
    pub reputation: i32,
    /// The box inflation across and up/down, from the golem's bounds.
    pub across: f64,
    pub up: f64,
    /// The reach of the golem, in blocks (the villagers and the player within it).
    pub range: f64,
    /// `unseenMemoryTicks` of the target (`TargetGoal` default: 60, to verify in the jar).
    pub memory: u16,
}
impl Params {
    pub const DEFAULT: Self = Self { reputation: -100, across: 10.0, up: 8.0, range: 64.0, memory: 60 };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Defend {
    p: Params,
    slot: u8,
}

/// The count of `canUse` asks made, kept only in a world that holds it (the cost test inserts it).
#[derive(Resource, Default)]
pub struct Asks(pub u64);
pub static MODULE: Module = Module { name: "defend_village_target", flags: TARGET, equip, arm, fired, systems, can_use, start, stop, ..Module::DEFAULT };
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Defend { p: *params::<Params>(p), slot: inst.slot });
}
fn arm(w: &mut World, e: Entity, inst: Inst) {
    if let Some(d) = w.get::<Defend>(e).copied() {
        if grudge(w, e, d.p) {
            crate::goals::arm(w, e, inst.slot, 1);
        }
    }
}
fn fired(_: &mut World, _: Entity, _: Inst) -> bool {
    true
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems(wake_on_player.in_set(slot));
}
/// A player moved: the golems around the place he reached ask whether he is the one to defend against.
fn wake_on_player(on: Res<Online>, idx: Res<Index>, q: Query<&Defend>, mut reqs: ResMut<Requests>) {
    if q.is_empty() {
        return;
    }
    for &(_, new) in &on.seen {
        let Some(n) = new else { continue };
        idx.for_radius(n.x, n.z, 16.0, |c| {
            if let Ok(d) = q.get(c) {
                reqs.0.push(Req::Start(c, d.slot));
            }
        });
    }
}
/// A gossip made `villager` hold a player at or below the golems' threshold: the golems that see it wake.
pub fn wake_golems(w: &mut World, villager: Entity) {
    let Some(at) = w.get::<Pos>(villager).map(|p| p.0) else { return };
    let bad = w.get::<Gossips>(villager).is_some_and(|g| g.players().any(|pl| g.reputation(pl) <= Params::DEFAULT.reputation));
    if !bad {
        return;
    }
    let mut golems = Vec::new();
    w.resource::<Index>().for_radius(at.x, at.z, 16.0, |c| golems.push(c));
    for c in golems {
        if w.get::<Defend>(c).is_some() {
            crate::goals::wake(w, c, &MODULE);
        }
    }
}
/// A villager in the golem's box, in reach and seen, holds a player in bad reputation.
fn grudge(w: &World, e: Entity, p: Params) -> bool {
    let (Some(pos), Some(b)) = (w.get::<Pos>(e).map(|x| x.0), w.get::<Bounds>(e)) else { return false };
    let area = b.0.inflate(Vec3::new(p.across, p.up, p.across));
    let mut found = false;
    w.resource::<Index>().for_radius(pos.x, pos.z, 16.0, |c| {
        found |= w.get::<Gossips>(c).is_some_and(|g| g.players().any(|pl| g.reputation(pl) <= p.reputation))
            && w.get::<Pos>(c).is_some_and(|cp| area.contains(cp.0) && cp.0.distance_squared(pos) <= p.range * p.range)
            && w.get::<Health>(c).is_some_and(|h| h.0 > 0.0)
            && targeting::sees(w, e, Who::Entity(c));
    });
    found
}
/// The player this golem defends against now, if any.
fn pick(w: &World, e: Entity, p: Params) -> Option<u32> {
    // `IronGolem.canAttack`: a golem a player built never takes a player for its target.
    if w.get::<PlayerCreated>(e).is_some() {
        return None;
    }
    let pos = w.get::<Pos>(e)?.0;
    let looker = w.get::<SpeciesId>(e).map(|s| s.0);
    let area = w.get::<Bounds>(e)?.0.inflate(Vec3::new(p.across, p.up, p.across));
    let reach = p.range * p.range;
    let mut villagers = Vec::new();
    w.resource::<Index>().for_radius(pos.x, pos.z, 16.0, |c| {
        if let (Some(g), Some(cp), Some(h)) = (w.get::<Gossips>(c), w.get::<Pos>(c), w.get::<Health>(c)) {
            if h.0 > 0.0 && area.contains(cp.0) && cp.0.distance_squared(pos) <= reach && targeting::sees(w, e, Who::Entity(c)) {
                villagers.push(g.clone());
            }
        }
    });
    if villagers.is_empty() {
        return None;
    }
    let ps = w.resource::<PlayerState>();
    let mut players = Vec::new();
    w.resource::<Online>().for_near(pos.x, pos.z, p.range, |pl| {
        let at = Vec3::new(pl.x, pl.y, pl.z);
        let i = pl.id as usize;
        if ps.creative.get(i).copied().unwrap_or(false) || ps.spectator.get(i).copied().unwrap_or(false) {
            return;
        }
        // `forCombat().range(64)`: the reach is scaled by the player's visibility (`Player.getVisibilityPercent`, worn head).
        let vis = ps.visibility(pl.id) * targeting::head_factor(ps.helmet(pl.id), looker);
        let sight = targeting::sight_range(p.range, vis);
        if area.contains(at) && pos.distance_squared(at) <= sight * sight && villagers.iter().any(|g| g.reputation(pl.id) <= p.reputation) && targeting::sees(w, e, Who::Player(pl.id)) {
            players.push((pos.distance_squared(at), pl.id));
        }
    });
    nearest_player(players)
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    if let Some(mut a) = w.get_resource_mut::<Asks>() {
        a.0 += 1;
    }
    let Some(d) = w.get::<Defend>(e).copied() else { return false };
    pick(w, e, d.p).is_some()
}
fn start(w: &mut World, e: Entity, inst: Inst) {
    let Some(d) = w.get::<Defend>(e).copied() else { return };
    if let Some(id) = pick(w, e, d.p) {
        targeting::set_target(w, e, Target::player(id, d.p.memory));
    }
    let _ = inst;
}
fn stop(w: &mut World, e: Entity, inst: Inst) {
    if w.get::<Target>(e).is_some_and(|t| t.slot == inst.slot) {
        targeting::clear_target(w, e);
    }
}
