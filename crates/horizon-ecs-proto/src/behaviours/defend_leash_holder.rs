//! `TraderLlamaDefendWanderingTraderGoal`, as a module with no species in it: a creature held on a lead by a CREATURE attacks whoever hurt
//! that holder last (within 100 ticks, once per hit), if the attacker lives, is not a creative or spectating player, is within the
//! follow range and seen. The holder being "a trader" is the holder having a record of its last attacker (`LastHurtBy`): a holder that is
//! never hurt gives nothing to defend. What wakes it: the holder hurt by an entity (`Events.hurt_by_entity`: the creatures it holds are
//! looked up in the directory of leads), and a lead put on a creature (`Events.leashed`). No polling. The target is kept by the frame
//! (`targeting::target_continue`, not requiring sight).
use super::*;
use crate::leash::{held_by, holder_of, Holder};
use crate::targeting::{self, Eye, FollowRange, Target, Who};
use crate::{cow::*, goals::*};
use horizon_core::geometry::Vec3;
use crate::rules::defend_leash_holder::beyond_range;
use crate::rules::defend_trusted::{hit_is_fresh, target_memory};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// `unseenMemoryTicks` of the target (the `TargetGoal` default, 60).
    pub unseen_memory: i32,
}
impl Params {
    pub const DEFAULT: Self = Self { unseen_memory: 60 };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Defender {
    p: Params,
    slot: u8,
    /// The hit already avenged (`LastHurtBy.at` of the holder).
    handled: u64,
    attacker: Option<Who>,
}
pub static MODULE: Module = Module { name: "defend_leash_holder", flags: TARGET, equip, can_use, start, stop, tick: Some(tick), requires: &[Cap::Body, Cap::Goals, Cap::Targets], ..Module::DEFAULT };
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Defender { p: *params::<Params>(p), slot: inst.slot, handled: u64::MAX, attacker: None });
}
fn tick(w: &mut World) {
    tick_state(w, wake);
}
/// The holder was hurt by an entity: the creatures it holds look at who did it; a lead put on a creature: it looks at once.
fn wake(ev: Res<Events>, leashes: Option<Res<crate::leash::Leashes>>, q: Query<&Defender>, mut reqs: ResMut<Requests>) {
    if let Some(leashes) = leashes {
        for victim in ev.hurt_by_entity.iter() {
            for &held in leashes.held_by(Holder::Entity(victim)) {
                if let Ok(d) = q.get(held) {
                    reqs.0.push(Req::Start(held, d.slot));
                }
            }
        }
    }
    for e in ev.leashed.iter() {
        if let Ok(d) = q.get(e) {
            reqs.0.push(Req::Start(e, d.slot));
        }
    }
}
/// The record of the holder: who hurt it last, when (its own time), if within 100 ticks.
fn record(w: &World, e: Entity) -> Option<(Who, u64)> {
    let Some(Holder::Entity(h)) = holder_of(w, e) else { return None };
    let r = w.get::<LastHurtBy>(h)?;
    hit_is_fresh(now_of(w, h), r.at).then_some((r.who, r.at))
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let Some(d) = w.get::<Defender>(e).copied() else { return false };
    let Some((who, at)) = record(w, e) else { return false };
    if at == d.handled {
        return false;
    }
    let Some(i) = targeting::info(w, who) else { return false };
    if i.exempt {
        return false;
    }
    let (Some(pos), Some(range), Some(look)) = (w.get::<Pos>(e).map(|p| p.0), w.get::<FollowRange>(e).map(|r| r.0), w.get::<Eye>(e).map(|y| y.0)) else { return false };
    if beyond_range(d2(pos, i.pos), range) || !w.resource::<Level>().mw.0.line_of_sight_for(Some(pos.y), Vec3::new(pos.x, pos.y + look, pos.z), Vec3::new(i.pos.x, i.eye, i.pos.z)) {
        return false;
    }
    w.get_mut::<Defender>(e).unwrap().attacker = Some(who);
    true
}
fn start(w: &mut World, e: Entity, inst: Inst) {
    let Some(d) = w.get::<Defender>(e).copied() else { return };
    let Some(who) = d.attacker else { return };
    if let Some((_, at)) = record(w, e) {
        w.get_mut::<Defender>(e).unwrap().handled = at;
    }
    let mut t = Target::new(who, target_memory(d.p.unseen_memory), inst.slot);
    t.must_see = false;
    targeting::set_target(w, e, t);
}
fn stop(w: &mut World, e: Entity, inst: Inst) {
    if w.get::<Target>(e).is_some_and(|t| t.slot == inst.slot) {
        targeting::clear_target(w, e);
    }
}
/// The creatures a holder holds, for the tests and the hosts.
pub fn defenders_of(w: &World, h: Entity) -> Vec<Entity> {
    held_by(w, Holder::Entity(h))
}
