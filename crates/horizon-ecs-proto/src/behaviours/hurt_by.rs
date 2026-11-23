//! `HurtByTargetGoal`: whoever hurt the attacker last (within 100 ticks, once per hit) becomes its target, and the
//! creatures of the same kind around it that have no target are alerted (`alertOthers`). What wakes it: the damage event
//! (`apply_damage` writes `HurtBy`), heard on the selector's next tick; nothing else, no polling.
use super::*;
use crate::targeting::{self, FollowRange, Target, Who};
use crate::{cow::*, goals::*, hostile::HurtBy};

/// `IronGolem.isPlayerCreated` (NBT `PlayerCreated`): `canAttack` refuses a player, so the golem never answers one.
#[derive(Component, Clone, Copy, Debug)]
pub struct PlayerCreated;

/// `IronGolem.setPlayerCreated`: the marker is posed by a golem built from blocks, and taken off the others.
pub fn set_player_created(w: &mut World, e: Entity, on: bool) {
    if on {
        w.entity_mut(e).insert(PlayerCreated);
    } else {
        w.entity_mut(e).remove::<PlayerCreated>();
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// `setAlertOthers`: the creatures around take the same target.
    pub alert: bool,
    /// The kinds alerted (empty: the attacker's own kind), and the kinds never alerted.
    pub alert_types: &'static [&'static str],
    pub alert_except: &'static [&'static str],
    /// `unseenMemoryTicks` of the target it takes, and of the one it gives to the alerted.
    pub unseen_memory: i32,
    /// The owner (`tame::Owner`) is never taken for a target, whoever hit.
    pub spare_owner: bool,
    /// `alert_if`: a condition the ALERTED creature must meet (the panda: only the aggressive ones are called).
    pub alert_when: Option<fn(&World, Entity) -> bool>,
    /// `LlamaHurtByTargetGoal`: a shot by this creature (`ranged_attack::DidShot`) ends the goal, once: a shot made while the goal did
    /// not run is held and ends it right after its next start (the `Mob`'s `flag2`, cleared when read).
    pub ends_on_shot: bool,
    /// `PolarBearHurtByTargetGoal`: the others are alerted only when THIS creature is a baby (a cub calls the adults; combine with `alert_when`).
    pub alert_if_baby: bool,
    /// Attackers of these species (names) are never answered (`HurtByTarget.ignoring`: the drowned ignore the drowned, the dolphin the guardians).
    pub ignoring: &'static [&'static str],
    /// The attacker's own state: when this holds, the hit is not answered (`HoglinAi.maybeRetaliate`: a piglin that is avoiding).
    pub attacker_when: Option<fn(&World, Entity) -> bool>,
}
impl Params {
    pub const DEFAULT: Self = Self { alert: false, alert_types: &[], alert_except: &[], unseen_memory: 300, spare_owner: false, alert_when: None, ends_on_shot: false, alert_if_baby: false, ignoring: &[], attacker_when: None };
    pub const ALERT: Self = Self { alert: true, ..Self::DEFAULT };
}
/// The state: the last hit (who, tick) and the one the goal answered, the kinds resolved to masks.
#[derive(Component, Clone, Copy, Debug)]
pub struct HurtByGoal {
    p: &'static Params,
    pub slot: u8,
    last: Option<(Who, u64)>,
    handled: u64,
    alert_types: crate::species::SpeciesSet,
    alert_except: crate::species::SpeciesSet,
    ignore: crate::species::SpeciesSet,
    /// A target an alerted creature takes (written by the one that called it).
    called_by: Option<Who>,
}

pub static MODULE: Module = Module {
    name: "hurt_by",
    flags: TARGET,
    equip,
    can_use,
    start,
    stop,
    systems,
    requires: &[Cap::Living, Cap::Targets],
    ..Module::DEFAULT
};

fn mask_of(w: &World, names: &[&str]) -> crate::species::SpeciesSet {
    let _ = w;
    crate::species::SpeciesSet::of(names.iter().filter_map(|n| crate::species::registry().id(n)).map(|i| i as usize))
}
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = params::<Params>(p);
    let (alert_types, alert_except, ignore) = (mask_of(w, p.alert_types), mask_of(w, p.alert_except), mask_of(w, p.ignoring));
    w.entity_mut(e).insert(HurtByGoal { p, slot: inst.slot, last: None, handled: u64::MAX, alert_types, alert_except, ignore, called_by: None });
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems(hurt_events.after(apply_damage).in_set(slot));
}
/// `DidShot` was just set (the hook of the component, so no system runs when nobody shoots): a creature that shoots while its hunt
/// (this goal's target) runs ends the hunt, `ends_on_shot`.
pub(crate) fn on_shot(w: &mut bevy_ecs::world::DeferredWorld, e: Entity) {
    let Some(g) = w.get::<HurtByGoal>(e) else { return };
    let (ends, slot) = (g.p.ends_on_shot, g.slot);
    if ends && w.get::<Target>(e).is_some_and(|t| t.slot == slot) {
        w.commands().entity(e).remove::<crate::behaviours::ranged_attack::DidShot>();
        w.resource_mut::<Requests>().0.push(Req::End(e, slot));
    }
}
/// A creature was hurt by a player or a creature: the goal is woken at the selector's next tick.
/// A player hit `e` less than `ticks` ticks ago (`PiglinAi.wasHurtBy` sets `ADMIRING_DISABLED` for 400 ticks): read from the last hit this goal kept.
pub fn hit_by_player_within(w: &World, e: Entity, ticks: u64) -> bool {
    w.get::<HurtByGoal>(e).and_then(|g| g.last).is_some_and(|(who, at)| matches!(who, Who::Player(_)) && now_of(w, e).saturating_sub(at) < ticks)
}
fn hurt_events(clock: Res<Clock>, mut by: ResMut<HurtBy>, mut q: Query<&mut HurtByGoal>, mut reqs: ResMut<Requests>) {
    for (e, who) in std::mem::take(&mut by.0) {
        if let Ok(mut g) = q.get_mut(e) {
            g.last = Some((who, clock.of(e)));
            reqs.0.push(Req::Start(e, g.slot));
        }
    }
}
/// `HurtByTargetGoal.canUse`: a hit it has not answered yet, by someone alive that can be targeted.
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let now = crate::cow::now_of(w, e);
    let Some(g) = w.get::<HurtByGoal>(e).copied() else { return false };
    let Some((who, at)) = g.last else { return false };
    // `getLastHurtByMob` is forgotten after 100 ticks.
    if at == g.handled || now.saturating_sub(at) > 100 {
        return false;
    }
    if g.p.spare_owner && w.get::<crate::tame::Owner>(e).is_some_and(|o| o.0 == Some(who)) {
        return false;
    }
    // `Mob.canAttack` refuses the attacker (the ghast): no retaliation.
    if let Who::Entity(o) = who
        && w.get::<crate::targeting::RefusedByMobs>(o).is_some()
    {
        return false;
    }
    if let Who::Entity(o) = who
        && w.get::<SpeciesId>(o).is_some_and(|s| g.ignore.contains(s.0))
    {
        return false;
    }
    if let Who::Entity(o) = who
        && g.p.attacker_when.is_some_and(|f| f(w, o))
    {
        return false;
    }
    // `HurtByTargetGoal.canUse`: a player's hit is left to the anger goals under the universal anger rule.
    if matches!(who, Who::Player(_)) && w.get_resource::<Level>().is_some_and(|l| l.mw.0.universal_anger) {
        return false;
    }
    if matches!(who, Who::Player(_)) && w.get::<PlayerCreated>(e).is_some() {
        return false;
    }
    match targeting::info(w, who) {
        Some(i) => !i.exempt,
        None => false,
    }
}
fn start(w: &mut World, e: Entity, inst: Inst) {
    let Some(mut g) = w.get::<HurtByGoal>(e).copied() else { return };
    let Some((who, at)) = g.last else { return };
    g.handled = at;
    w.get_mut::<HurtByGoal>(e).unwrap().handled = at;
    let memory = ((g.p.unseen_memory + 1) / 2) as u16;
    targeting::set_target(w, e, Target::new(who, memory, inst.slot));
    if g.p.alert || (g.p.alert_if_baby && w.get::<GrowAt>(e).is_some()) {
        alert_others(w, e, who, g);
    }
    if g.p.ends_on_shot && w.entity_mut(e).take::<crate::behaviours::ranged_attack::DidShot>().is_some() {
        w.resource_mut::<Requests>().0.push(Req::End(e, inst.slot));
    }
}
/// `HurtByTargetGoal.alertOthers`: the others of the alerted kinds within the follow range (10 blocks up and down) that
/// have no target take the same one.
fn alert_others(w: &mut World, e: Entity, who: Who, g: HurtByGoal) {
    let (pos, sid) = (w.get::<Pos>(e).unwrap().0, w.get::<SpeciesId>(e).unwrap().0);
    let follow = w.get::<FollowRange>(e).map_or(16.0, |f| f.0);
    let kinds = if g.alert_types.is_empty() { crate::species::SpeciesSet::of([sid as usize]) } else { g.alert_types };
    let mut others = Vec::new();
    w.resource::<Index>().for_radius(pos.x, pos.z, follow, |o| others.push(o));
    for o in others {
        if o == e || Who::Entity(o) == who || w.get::<Target>(o).is_some() {
            continue;
        }
        let (Some(op), Some(os), true) = (w.get::<Pos>(o), w.get::<SpeciesId>(o), w.get::<HurtByGoal>(o).is_some()) else { continue };
        if kinds.contains(os.0)
            && !g.alert_except.contains(os.0)
            && g.p.alert_when.is_none_or(|f| f(w, o))
            && (op.0.x - pos.x).abs() <= follow + 1.0
            && (op.0.z - pos.z).abs() <= follow + 1.0
            && (op.0.y - pos.y).abs() <= 11.0
        {
            w.get_mut::<HurtByGoal>(o).unwrap().called_by = Some(who);
            w.resource_mut::<Requests>().0.push(Req::Call(o, called));
        }
    }
}
/// An alerted creature takes the target of the one that called it: `MobEvent::SetTarget` writes the target of the creature
/// without any of its goals running, so it keeps it until the target dies (no memory, no follow distance), and a target
/// goal of the creature can still replace it.
fn called(w: &mut World, e: Entity) {
    let Some(g) = w.get::<HurtByGoal>(e).copied() else { return };
    let Some(who) = g.called_by else { return };
    if w.get::<Target>(e).is_some() {
        return;
    }
    w.get_mut::<HurtByGoal>(e).unwrap().called_by = None;
    let mut t = Target::new(who, u16::MAX, u8::MAX);
    (t.must_see, t.follow_scale) = (false, f64::INFINITY);
    targeting::set_target(w, e, t);
}
fn stop(w: &mut World, e: Entity, inst: Inst) {
    if w.get::<Target>(e).is_some_and(|t| t.slot == inst.slot) {
        targeting::clear_target(w, e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behaviours::ranged_attack::DidShot;
    use crate::species::Config;

    /// A victim whose `hurt_by` is `p`, and a hitter; the victim is hit once, and given a few ticks to answer.
    fn hit_world(p: &'static Params) -> Option<(Herd, Entity, Entity)> {
        if crate::cow::skip_unless_walker("hurt_by") {
            return None;
        }
        let mut h = Herd::with_slab_awake(20, false, false);
        let entries: &'static [Entry] = Box::leak(vec![Entry::new(&MODULE, 1, p)].into_boxed_slice());
        let mut sp = *species();
        sp.goals = Config { entries };
        let (victim, hitter) = (spawn_species(&mut h.world, &sp, 0.5, 0.5, 7), spawn_species(&mut h.world, &sp, 4.5, 0.5, 9));
        for _ in 0..5 {
            h.tick();
        }
        h.world.resource_mut::<HurtBy>().0.push((victim, Who::Entity(hitter)));
        for _ in 0..4 {
            h.tick();
        }
        Some((h, victim, hitter))
    }

    /// `LlamaHurtByTargetGoal`: a shot ends the hunt (witness: without the option the hunt stays after the same shot).
    #[test]
    fn a_shot_ends_the_hunt_only_with_the_option() {
        static ENDS: Params = Params { ends_on_shot: true, ..Params::DEFAULT };
        static KEEPS: Params = Params::DEFAULT;
        for (p, ends) in [(&KEEPS, false), (&ENDS, true)] {
            let Some((mut h, victim, hitter)) = hit_world(p) else { return };
            assert_eq!(h.world.get::<Target>(victim).map(|t| t.who), Some(Who::Entity(hitter)), "witness: the hit gives a target");
            h.world.entity_mut(victim).insert(crate::behaviours::ranged_attack::DidShot);
            for _ in 0..4 {
                h.tick();
            }
            assert_eq!(h.world.get::<Target>(victim).is_none(), ends, "ends_on_shot = {ends}");
        }
    }

    /// The `Mob`'s `flag2` is held: a shot made before the goal runs ends the next hunt right after it starts, once.
    #[test]
    fn a_shot_made_before_the_hunt_ends_it_once() {
        static ENDS: Params = Params { ends_on_shot: true, ..Params::DEFAULT };
        if crate::cow::skip_unless_walker("hurt_by") {
            return;
        }
        let mut h = Herd::with_slab_awake(20, false, false);
        let entries: &'static [Entry] = Box::leak(vec![Entry::new(&MODULE, 1, &ENDS)].into_boxed_slice());
        let mut sp = *species();
        sp.goals = Config { entries };
        let (victim, hitter) = (spawn_species(&mut h.world, &sp, 0.5, 0.5, 7), spawn_species(&mut h.world, &sp, 4.5, 0.5, 9));
        h.world.entity_mut(victim).insert(DidShot);
        for _ in 0..5 {
            h.tick();
        }
        h.world.resource_mut::<HurtBy>().0.push((victim, Who::Entity(hitter)));
        for _ in 0..6 {
            h.tick();
        }
        assert!(h.world.get::<Target>(victim).is_none(), "the held shot ended the hunt");
        assert!(h.world.get::<DidShot>(victim).is_none(), "and was consumed");
    }

    /// `PolarBearHurtByTargetGoal`: only a hurt cub calls the others; an adult hit the same way calls nobody (the witness).
    #[test]
    fn only_a_hurt_baby_alerts_the_others() {
        static P: Params = Params { alert_if_baby: true, ..Params::DEFAULT };
        if crate::cow::skip_unless_walker("hurt_by") {
            return;
        }
        for baby in [false, true] {
            let mut h = Herd::with_slab_awake(20, false, false);
            let entries: &'static [Entry] = Box::leak(vec![Entry::new(&MODULE, 1, &P)].into_boxed_slice());
            let mut sp = *species();
            sp.goals = Config { entries };
            let victim = spawn_species(&mut h.world, &sp, 0.5, 0.5, 7);
            let ally = spawn_species(&mut h.world, &sp, 2.5, 0.5, 8);
            let hitter = spawn_species(&mut h.world, &sp, 6.5, 0.5, 9);
            if baby {
                h.world.entity_mut(victim).insert(GrowAt(u64::MAX));
            }
            for _ in 0..5 {
                h.tick();
            }
            h.world.resource_mut::<HurtBy>().0.push((victim, Who::Entity(hitter)));
            for _ in 0..4 {
                h.tick();
            }
            assert_eq!(h.world.get::<Target>(ally).map(|t| t.who), baby.then_some(Who::Entity(hitter)), "baby {baby}: the ally's target");
        }
    }
}
