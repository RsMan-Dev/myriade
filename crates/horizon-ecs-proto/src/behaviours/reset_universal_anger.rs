//! `ResetUniversalAngerTargetGoal` (`mobs/golem.rs:26-57`, the iron golem's target goal, priority 4): with the `universal_anger`
//! rule on, a golem whose latest attacker is a player, hit after the one it last handled, forgets its target and starts its
//! anger over. The start stamps the attacker's time, forgets the attacker (`LastHurtBy`) and clears the target; the countdown
//! is `neutral_anger`'s, which starts when a held target is lost. Woken by the hits on the entity (`Events.hurt_by_entity`),
//! nothing else: the rule is a fact of the world and the attacker changes only with a hit.
use super::*;
use crate::cow::{Level, LastHurtBy};
use crate::goals::{Events, GoalSlot, Req, Requests};
use crate::targeting::{self, Who};

#[derive(Clone, Copy, Debug)]
pub struct Params;
impl Params {
    pub const DEFAULT: Self = Self;
}
/// `timestamp`: the time of the attacker the last reset handled (`lastHurtByPlayerTimestamp`, 0 at the start).
#[derive(Component, Clone, Copy, Debug)]
pub struct Reset {
    pub timestamp: u64,
    pub slot: u8,
}

pub static MODULE: Module = Module {
    name: "reset_universal_anger",
    flags: 0,
    equip,
    can_use,
    start,
    systems,
    requires: &[Cap::Goals, Cap::Targets],
    ..Module::DEFAULT
};

fn equip(w: &mut World, e: Entity, inst: Inst, _: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Reset { timestamp: 0, slot: inst.slot });
}
/// `ResetUniversalAngerTargetGoal.canUse`: the rule, a player that hit it last, and a hit after the handled one.
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let Some(r) = w.get::<Reset>(e).copied() else { return false };
    let Some(hit) = w.get::<LastHurtBy>(e).copied() else { return false };
    w.get_resource::<Level>().is_some_and(|l| l.mw.0.universal_anger) && matches!(hit.who, Who::Player(_)) && hit.at > r.timestamp
}
/// `start`: the attacker's time is kept, the attacker is forgotten, the target is cleared (its loss starts the anger
/// countdown of `neutral_anger` when the golem held it).
fn start(w: &mut World, e: Entity, inst: Inst) {
    if let Some(hit) = w.get::<LastHurtBy>(e).copied() {
        if let Some(mut r) = w.get_mut::<Reset>(e) {
            r.timestamp = hit.at;
        }
        w.entity_mut(e).remove::<LastHurtBy>();
    }
    targeting::clear_target(w, e);
    w.resource_mut::<Requests>().0.push(Req::End(e, inst.slot));
}

fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems(hear.in_set(slot));
}
/// The hits of this tick on the entities that carry the reset: each one asks the selector to wake the goal (the others cost
/// nothing).
fn hear(ev: Res<Events>, mut reqs: ResMut<Requests>, q: Query<&Reset>) {
    if q.is_empty() {
        return;
    }
    for e in ev.hurt_by_entity.iter() {
        if let Ok(r) = q.get(e) {
            reqs.0.push(Req::Start(e, r.slot));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behaviours::neutral_anger;
    use crate::goals::GoalSet;
    use crate::cow::{species, spawn_species, DamageQueue, Herd, Mw, Species};
    use crate::species::Config;
    use crate::targeting::{Target, Who};
    use horizon_client_lib::mobs::MobWorld;

    static ENTRIES: [Entry; 2] = [Entry::new(&MODULE, 4, &Params::DEFAULT), Entry::new(&neutral_anger::MODULE, 255, &neutral_anger::Params::DEFAULT)];

    /// Two cows: `e` carries the reset and the anger, `foe` is the target `e` holds (alive, so the targeting keeps it).
    fn pair(seed: u64) -> (Herd, Entity, Entity) {
        let mut h = Herd::with_slab_awake(8, true, false);
        let mut sp: Species = *species();
        sp.goals = Config { entries: &ENTRIES };
        let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, seed);
        let foe = spawn_species(&mut h.world, &sp, 3.5, 0.5, seed + 1);
        crate::goals::arm_all(&mut h.world);
        (h, e, foe)
    }
    fn hold(h: &mut Herd, e: Entity, foe: Entity) {
        let mut t = Target::new(Who::Entity(foe), u16::MAX, u8::MAX);
        t.must_see = false;
        t.follow_scale = f64::INFINITY;
        crate::targeting::set_target(&mut h.world, e, t);
    }
    fn hit_by(h: &mut Herd, e: Entity, who: Who) {
        let at = (0.5, 0.5);
        let mut q = h.world.resource_mut::<DamageQueue>();
        match who {
            Who::Player(id) => q.add_by(e, 1.0, at, id),
            Who::Entity(a) => q.add_by_entity(e, 1.0, at, a),
        }
    }
    fn ticks(h: &mut Herd, n: usize) {
        for _ in 0..n {
            h.tick();
        }
    }
    fn timestamp(h: &Herd, e: Entity) -> u64 {
        h.world.get::<Reset>(e).unwrap().timestamp
    }
    /// The rule of the world: `universal_anger` is read from the level's mob world, so the test builds a copy with the rule.
    fn with_universal_anger(h: &mut Herd, on: bool) {
        let mut mw: MobWorld<'static> = h.world.resource::<Level>().mw.0.clone();
        mw.universal_anger = on;
        let mw: &'static MobWorld<'static> = Box::leak(Box::new(mw));
        h.world.resource_mut::<Level>().mw = Mw(mw);
    }

    /// A player's hit on a golem that holds a target: the target is forgotten, the anger counts down (400 to 780 ticks of
    /// `neutral_anger`), the attacker's time is kept and the goal is not running any more. The control: the same hit from a
    /// creature does nothing, and so does the player's hit while the rule is off.
    #[test]
    fn a_player_hit_forgets_the_target_and_starts_the_anger_again() {
    if crate::cow::skip_unless_small_body("a_player_hit_forgets_the_target_and_starts_the_anger_again") {
        return;
    }
        if crate::cow::skip_unless_walker("a_player_hit_forgets_the_target_and_starts_the_anger_again") {
            return;
        }
        let (mut h, e, foe) = pair(31);
        with_universal_anger(&mut h, true);
        hold(&mut h, e, foe);
        ticks(&mut h, 3);
        assert!(neutral_anger::anger_at(&h.world, e) == Some(Who::Entity(foe)), "witness: the golem holds its target");
        hit_by(&mut h, e, Who::Entity(foe));
        // The hurt cooldown of a living entity (20 ticks) would ignore a second hit that comes too soon.
        ticks(&mut h, 25);
        assert!(h.world.get::<Target>(e).is_some(), "a creature's hit reset the target");
        assert_eq!(timestamp(&h, e), 0, "a creature's hit stamped the reset");
        assert!(h.world.get::<LastHurtBy>(e).is_some(), "the creature's hit is not the golem's last attacker");

        hit_by(&mut h, e, Who::Player(7));
        ticks(&mut h, 6);
        assert!(h.world.get::<Target>(e).is_none(), "the player's hit did not clear the target");
        let stamped = timestamp(&h, e);
        assert!(stamped > 0, "the reset did not keep the attacker's time");
        assert!(h.world.get::<LastHurtBy>(e).is_none(), "the attacker is not forgotten");
        assert!(neutral_anger::is_angry(&h.world, e), "the anger did not start");
        assert!(!h.world.get::<GoalSet>(e).unwrap().is_running(&MODULE), "the goal is still running after its start");
    }

    /// A reset is made per hit: a target taken again without a new hit is not reset; a new hit by a player is.
    #[test]
    fn one_reset_per_hit_of_a_player() {
    if crate::cow::skip_unless_small_body("one_reset_per_hit_of_a_player") {
        return;
    }
        if crate::cow::skip_unless_walker("one_reset_per_hit_of_a_player") {
            return;
        }
        let (mut h, e, foe) = pair(32);
        with_universal_anger(&mut h, true);
        hold(&mut h, e, foe);
        hit_by(&mut h, e, Who::Player(3));
        ticks(&mut h, 6);
        let first = timestamp(&h, e);
        assert!(first > 0 && h.world.get::<Target>(e).is_none(), "witness: the first hit reset the golem");
        hold(&mut h, e, foe);
        ticks(&mut h, 30);
        assert!(h.world.get::<Target>(e).is_some(), "a target taken again was reset without a hit");
        assert_eq!(timestamp(&h, e), first, "the stamp moved without a hit");
        hit_by(&mut h, e, Who::Player(3));
        ticks(&mut h, 6);
        assert!(h.world.get::<Target>(e).is_none() && timestamp(&h, e) > first, "the second hit did not reset the golem");
    }

    /// With the rule off, a player's hit does nothing (the control of the test above).
    #[test]
    fn without_the_rule_a_player_hit_does_nothing() {
    if crate::cow::skip_unless_small_body("without_the_rule_a_player_hit_does_nothing") {
        return;
    }
        if crate::cow::skip_unless_walker("without_the_rule_a_player_hit_does_nothing") {
            return;
        }
        let (mut h, e, foe) = pair(33);
        hold(&mut h, e, foe);
        hit_by(&mut h, e, Who::Player(5));
        ticks(&mut h, 6);
        assert!(h.world.get::<Target>(e).is_some(), "the rule is off: the target was reset");
        assert_eq!(timestamp(&h, e), 0, "the rule is off: the stamp moved");
    }
}
