//! `EvokerFangs` (`projectile/EvokerFangs`): a fang is an entity of its own with a position, its owner and its clock, and no tick.
//! Three deadlines of its own time wake it: the start at warmup `+ 1` (event 4 to the host, which plays the attack sound), the bite at
//! `-8` (6 of indirect magic to what stands in its box), the end at `-23` (22 ticks of life after the start).
use crate::area::{creatures_in, players_in, species_name};
use crate::colony_register::MiscOut;
use crate::cow::*;
use crate::goals::Events;
use crate::bridge::teams;
use crate::host_out_misc::MiscEvent;
use crate::hostile::PlayerState;
use crate::targeting::Who;
use crate::tame::Owner;
use bevy_ecs::prelude::*;
use horizon_core::geometry::{Aabb, Vec3};

/// A fang and the one that summoned it (`EvokerFangs.owner`: a player or a creature, `None` when none is known; a creature owner gone
/// from the world is no owner at the bite).
#[derive(Component, Clone, Copy, Debug)]
pub struct Fang {
    pub owner: Option<Who>,
}

/// `new EvokerFangs(level, x, y, z, yaw, warmup, owner)`: the three deadlines are armed now, nothing runs in between.
pub fn spawn_fang(w: &mut World, at: Vec3, warmup: i32, owner: Option<Who>) -> Entity {
    let e = w.spawn((Pos(at), OwnTime, Fang { owner })).id();
    crate::nbt::mark_saved(w, e, crate::species::lists::EVOKER_FANGS);
    after(w, e, warmup + 1, start);
    after(w, e, warmup + 8, bite);
    after(w, e, warmup + 23, end);
    e
}

/// `EvokerFangs.tick` at `warmup` 0 after the countdown: the start (`broadcastEntityEvent` 4, once).
fn start(w: &mut World, e: Entity) {
    w.resource_mut::<Events>().host.entity_event(e, 4);
}

fn end(w: &mut World, e: Entity) {
    w.despawn(e);
}

/// The box of a fang: 0.5 wide (0.45 each side once inflated by 0.2 sideways), 0.8 high.
fn fang_box(at: Vec3) -> Aabb {
    let h = 0.45;
    Aabb::new(Vec3::new(at.x - h, at.y, at.z - h), Vec3::new(at.x + h, at.y + 0.8, at.z + h))
}

use crate::species::lists::is_vex;

/// `EvokerFangs.dealDamageTo` for a creature: no owner present hurts everyone; with one, `owner.isAlliedTo(target)` (`Entity.isAlliedTo`:
/// the same, or either one considers the other an ally, or both are in one team).
fn allied(w: &World, owner: Option<Who>, target: Entity) -> bool {
    let Some(owner) = owner else { return false };
    match owner {
        Who::Entity(o) => o == target || considers(w, o, target) || considers(w, target, o) || teams::allied(w, owner, Who::Entity(target)),
        Who::Player(_) => teams::allied(w, owner, Who::Entity(target)),
    }
}

/// `AbstractIllager.considersEntityAsAlly` (an illager's allies are the illagers, `#illager`, no teams), and `Evoker`'s: the vex whose
/// root owner is the evoker or an illager. Only evokers own fangs, so the vex branch is read for any illager here.
fn considers(w: &World, a: Entity, b: Entity) -> bool {
    let illager = |e: Entity| crate::type_tags::named(w, "illager", e);
    if !illager(a) {
        return false;
    }
    illager(b) || is_vex(species_name(w, b)) && root_owner(w, b).is_some_and(|r| r == a || illager(r))
}

/// `Vex.getRootOwner`: the owner of a vex, followed up through the vexes that own vexes.
fn root_owner(w: &World, mut e: Entity) -> Option<Entity> {
    let mut found = None;
    while let Some(Who::Entity(r)) = w.get::<Owner>(e).and_then(|o| o.0) {
        found = Some(r);
        if !is_vex(species_name(w, r)) {
            break;
        }
        e = r;
    }
    found
}

/// The bite (`warmup == -8`): 6 of indirect magic (the owner still there) or of magic (no owner) to each living creature and
/// player in the box, the owner and its allies excepted.
fn bite(w: &mut World, e: Entity) {
    let (Some(at), Some(owner)) = (w.get::<Pos>(e).map(|p| p.0), w.get::<Fang>(e).map(|f| f.owner)) else { return };
    let b = fang_box(at);
    let by = match owner {
        Some(Who::Entity(o)) if w.get_entity(o).is_err() => None,
        other => other,
    };
    let from = Some((at.x, at.z));
    let victims: Vec<Entity> = creatures_in(w, b, e)
        .into_iter()
        .filter(|&c| by != Some(Who::Entity(c)) && w.get::<Health>(c).is_some_and(|h| h.0 > 0.0) && !allied(w, by, c))
        .collect();
    for v in victims {
        w.resource_mut::<DamageQueue>().0.push(Hit { target: v, amount: 6.0, cause: Cause::Magic, damage: if by.is_some() { "indirect_magic" } else { "magic" }, from, by, direct: false, power: None, projectile: None });
    }
    let tick = w.resource::<Clock>().0;
    for id in players_in(w, b) {
        if by == Some(Who::Player(id)) || by.is_some_and(|o| teams::allied(w, o, Who::Player(id))) {
            continue;
        }
        w.resource_mut::<PlayerState>().hurt(tick, id, 6.0, (at.x, at.z), by);
    }
    w.get_resource_or_insert_with(MiscOut::default).0.push(MiscEvent::FangsBite { at: [at.x, at.y, at.z] });
}
