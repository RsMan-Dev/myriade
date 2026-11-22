//! `StartAttacking` (`monster_brain.rs` `B::StartAttacking`, `:650-677`): a one-shot of the Idle activity that takes an attack
//! target when there is none. The finder is `HurtBy` (the creature or player that hurt it) or `NearestVisiblePlayer` (the
//! creaking: only while its `is_active` rule holds). The target is written in `ATTACK_TARGET` and given to the targeting
//! service. The "no target yet" condition is the entry's `needs`.
use crate::behaviours::Cap;
use crate::brain::*;
use crate::targeting::{self, Target, Who};

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:start_attacking",
    mems: &[&ATTACK_TARGET, &crate::brain::sensors::attackable::NEAREST_ATTACKABLE, &HURT_BY_ENTITY, &NEAREST_VISIBLE_TARGETABLE_PLAYER, &CANT_REACH_WALK_TARGET_SINCE, &ANGRY_AT, &NEAREST_VISIBLE_NEMESIS],
    requires: &[Cap::Brain, Cap::Targets],
    one_shot: true,
    can_start,
    start,
    ..Behaviour::DEFAULT
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Finder {
    HurtBy,
    NearestVisiblePlayer,
    /// The `nearest_attackable` memory (the breeze's `BreezeAttackEntitySensor`).
    NearestAttackable,
    /// `PiglinBruteAi`'s `StartAttacking`: the anger target, then the nearest visible player, then the nemesis (baby piglin).
    Piglin,
}

/// `active` is the creaking's `isActive` rule. It is read only by `NearestVisiblePlayer`; the frame has no such rule yet,
/// so the default answers no (repli of the creaking's brief).
pub struct Params {
    pub finder: Finder,
    pub active: fn(&World, Entity) -> bool,
    /// The species' own `canAttack` rule on top of the generic one (the breeze: players and iron golems only).
    pub allowed: fn(&World, Who) -> bool,
}
fn inactive(_: &World, _: Entity) -> bool {
    false
}
fn anyone(_: &World, _: Who) -> bool {
    true
}
impl Params {
    pub const DEFAULT: Params = Params { finder: Finder::HurtBy, active: inactive, allowed: anyone };
}

/// `canAttack` without the species' rule: a living player that is not creative or spectator, or a creature. The breeze's
/// iron-golem rule is not ported (DOUTE).
pub fn can_attack(w: &World, who: Who) -> bool {
    targeting::info(w, who).is_some_and(|i| !i.exempt)
}

/// The target the finder gives, if it can be attacked.
fn target(w: &World, e: Entity, p: &Params) -> Option<Who> {
    let who = match p.finder {
        Finder::HurtBy => match get(w, e, &HURT_BY_ENTITY) {
            Some(MemValue::Who(who)) => *who,
            _ => return None,
        },
        Finder::NearestAttackable => match get(w, e, &crate::brain::sensors::attackable::NEAREST_ATTACKABLE) {
            Some(MemValue::Who(who)) => *who,
            _ => return None,
        },
        Finder::Piglin => return piglin_target(w, e, p),
        Finder::NearestVisiblePlayer => {
            if !(p.active)(w, e) {
                return None;
            }
            match get(w, e, &NEAREST_VISIBLE_TARGETABLE_PLAYER) {
                Some(MemValue::Who(who)) => *who,
                _ => return None,
            }
        }
    };
    (can_attack(w, who) && (p.allowed)(w, who)).then_some(who)
}

/// The first of the piglin memories that holds an attackable entity.
fn piglin_target(w: &World, e: Entity, p: &Params) -> Option<Who> {
    [&ANGRY_AT, &NEAREST_VISIBLE_TARGETABLE_PLAYER, &NEAREST_VISIBLE_NEMESIS].into_iter().find_map(|k| match get(w, e, k) {
        Some(MemValue::Who(who)) if can_attack(w, *who) && (p.allowed)(w, *who) => Some(*who),
        _ => None,
    })
}

fn can_start(w: &mut World, e: Entity, c: &Ctx) -> bool {
    target(w, e, c.get::<Params>()).is_some()
}

fn start(w: &mut World, e: Entity, c: &Ctx) {
    let Some(who) = target(w, e, c.get::<Params>()) else { return };
    set(w, e, &ATTACK_TARGET, MemValue::Who(who), None);
    // The memory of the target: the brain decides when it ends (StopAttackingIfTargetInvalid), not the targeting's sight.
    targeting::set_target(w, e, Target::new(who, u16::MAX, u8::MAX));
    erase(w, e, &CANT_REACH_WALK_TARGET_SINCE);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behaviours::Cap;
    use crate::cow::{ActiveUntil, Herd, SpeciesTable, spawn_species};
    use crate::goals::{Online, Player};
    use crate::hostile::PlayerState;
    use crate::targeting::Target;

    const CAPS: &[Cap] = &[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Locomotion, Cap::Targets, Cap::Goals, Cap::Brain];
    static NEEDS: Needs = &[(&ATTACK_TARGET, Need::Absent)];
    static HURT: Params = Params { finder: Finder::HurtBy, active: inactive, allowed: anyone };
    static CREAK_OFF: Params = Params { finder: Finder::NearestVisiblePlayer, active: inactive, allowed: anyone };
    fn active_yes(_: &World, _: Entity) -> bool {
        true
    }
    static CREAK_ON: Params = Params { finder: Finder::NearestVisiblePlayer, active: active_yes, allowed: anyone };

    fn brained(h: &mut Herd, params: &'static Params) -> Entity {
        let entries: &'static [BEntry] = Box::leak(Box::new([BEntry { behaviour: &BEHAVIOUR, activity: "core", priority: 1, params, needs: NEEDS, duration: None, kids: &[] }]));
        let def = BrainDef {
            sensors: &[],
            activities: &[ActivityDef { name: "core", slot: 1, requires: &[] }],
            pick: &[],
            initial: &[],
            behaviours: entries,
        };
        let plan: &'static Plan = Box::leak(Box::new(Plan::build(&def, CAPS).unwrap()));
        let sp = h.world.resource::<SpeciesTable>().0[0];
        let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 91);
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        h.world.entity_mut(e).insert(bundle(plan));
        crate::brain::start(&mut h.world, e);
        e
    }
    fn step(h: &mut Herd, e: Entity) {
        wake(&mut h.world, e);
        brain_step(&mut h.world);
    }
    fn player(h: &mut Herd, id: u32, x: f64) {
        h.world.resource_mut::<Online>().set(Player { id, x, y: 1.0, z: 0.5, main: 0, off: 0, ..Default::default() });
    }

    /// A creature that hurt it becomes its attack target, in the memory and in the targeting service; before the hurt, nothing.
    #[test]
    fn it_takes_the_creature_that_hurt_it() {
        let mut h = Herd::with_slab_awake(40, true, false);
        let e = brained(&mut h, &HURT);
        let sp = h.world.resource::<SpeciesTable>().0[0];
        let prey = spawn_species(&mut h.world, &sp, 3.5, 0.5, 92);
        step(&mut h, e);
        assert!(get(&h.world, e, &ATTACK_TARGET).is_none(), "sans blessure, pas de cible");
        set(&mut h.world, e, &HURT_BY_ENTITY, MemValue::Who(Who::Entity(prey)), None);
        step(&mut h, e);
        assert!(matches!(get(&h.world, e, &ATTACK_TARGET), Some(MemValue::Who(Who::Entity(p))) if *p == prey), "témoin : la cible est l'assaillant");
        assert_eq!(h.world.get::<Target>(e).map(|t| t.who), Some(Who::Entity(prey)));
    }

    /// An existing target is not replaced; without it, the same hurt starts the behaviour.
    #[test]
    fn it_keeps_the_target_it_has() {
        let mut h = Herd::with_slab_awake(40, true, false);
        let e = brained(&mut h, &HURT);
        let sp = h.world.resource::<SpeciesTable>().0[0];
        let prey = spawn_species(&mut h.world, &sp, 3.5, 0.5, 92);
        let other = spawn_species(&mut h.world, &sp, -3.5, 0.5, 93);
        set(&mut h.world, e, &ATTACK_TARGET, MemValue::Who(Who::Entity(other)), None);
        set(&mut h.world, e, &HURT_BY_ENTITY, MemValue::Who(Who::Entity(prey)), None);
        step(&mut h, e);
        assert!(matches!(get(&h.world, e, &ATTACK_TARGET), Some(MemValue::Who(Who::Entity(p))) if *p == other), "une cible déjà là ne change pas");
        erase(&mut h.world, e, &ATTACK_TARGET);
        step(&mut h, e);
        assert!(matches!(get(&h.world, e, &ATTACK_TARGET), Some(MemValue::Who(Who::Entity(p))) if *p == prey), "témoin : sans cible, le même assaillant la prend");
    }

    /// A creaking takes the nearest visible player only while it is active; a creative player is never taken.
    #[test]
    fn a_creaking_takes_a_visible_player_only_while_active_and_never_a_creative_one() {
        let mut h = Herd::with_slab_awake(40, true, false);
        let e = brained(&mut h, &CREAK_OFF);
        player(&mut h, 1, 4.5);
        set(&mut h.world, e, &NEAREST_VISIBLE_TARGETABLE_PLAYER, MemValue::Who(Who::Player(1)), None);
        step(&mut h, e);
        assert!(get(&h.world, e, &ATTACK_TARGET).is_none(), "inactif : pas de cible");
        erase(&mut h.world, e, &NEAREST_VISIBLE_TARGETABLE_PLAYER);
        let active = brained(&mut h, &CREAK_ON);
        set(&mut h.world, active, &NEAREST_VISIBLE_TARGETABLE_PLAYER, MemValue::Who(Who::Player(1)), None);
        step(&mut h, active);
        assert!(matches!(get(&h.world, active, &ATTACK_TARGET), Some(MemValue::Who(Who::Player(1)))), "témoin : actif, le joueur visible est pris");
        let spare = brained(&mut h, &CREAK_ON);
        h.world.resource_mut::<PlayerState>().set_creative(1, true);
        set(&mut h.world, spare, &NEAREST_VISIBLE_TARGETABLE_PLAYER, MemValue::Who(Who::Player(1)), None);
        step(&mut h, spare);
        assert!(get(&h.world, spare, &ATTACK_TARGET).is_none(), "un joueur créatif n'est pas attaqué");
        h.world.resource_mut::<PlayerState>().set_creative(1, false);
        step(&mut h, spare);
        assert!(get(&h.world, spare, &ATTACK_TARGET).is_some(), "témoin : hors créatif, il est pris");
    }
}
