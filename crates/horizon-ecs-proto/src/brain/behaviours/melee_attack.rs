//! `MeleeAttack` (`monster_brain.rs` `B::MeleeAttack`, `:795-820`; creaking Fight 11 at `:1536`): a one-shot of the Fight
//! activity. With an attack target that is visible, within melee reach and not cooling down, it looks at the target, strikes
//! it with its attribute damage (1 when none), then cools down for `cooldown` ticks (a memory with a deadline).
use crate::behaviours::Cap;
use crate::brain::*;
use crate::cow::{AttackDamage, Bounds, Clock, DamageQueue, Pos};
use crate::hostile::PlayerState;
use crate::rules::melee_attack as rules;
use crate::targeting::{self, Who};
use horizon_core::geometry::Vec3;

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:melee_attack",
    mems: &[&ATTACK_TARGET, &VISIBLE_MOBS, &ATTACK_COOLING_DOWN, &LOOK_TARGET],
    requires: &[Cap::Brain, Cap::Targets],
    one_shot: true,
    can_start,
    start,
    ..Behaviour::DEFAULT
};

fn always(_: &World, _: Entity) -> bool {
    true
}
/// `cooldown`: the ticks the attack cools down (40). `can_move` is the creaking's `canMove` (false while looked at): not in
/// the frame yet, so the default answers yes (repli).
pub struct Params {
    pub cooldown: u64,
    pub can_move: fn(&World, Entity) -> bool,
}
impl Params {
    pub const DEFAULT: Params = Params { cooldown: 40, can_move: always };
}

/// `Mob.isWithinMeleeAttackRange`: the attacker's box inflated by `sqrt(2.04) - 0.6` meets the target's box. The same test is
/// inside `hostile::melee_run`; it is written here because the frame has no such function (the lot's brief asked for its visibility).
pub fn within_melee(w: &World, e: Entity, who: Who) -> bool {
    let (Some(b), Some(i)) = (w.get::<Bounds>(e), targeting::info(w, who)) else { return false };
    let reach = rules::reach();
    b.0.inflate(Vec3::new(reach, 0.0, reach)).intersects(i.bounds)
}

fn target(w: &World, e: Entity) -> Option<Who> {
    match get(w, e, &ATTACK_TARGET) {
        Some(MemValue::Who(who)) => Some(*who),
        _ => None,
    }
}

fn can_start(w: &mut World, e: Entity, c: &Ctx) -> bool {
    let Some(who) = target(w, e) else { return false };
    if present(w, e, &ATTACK_COOLING_DOWN) || !present(w, e, &VISIBLE_MOBS) {
        return false;
    }
    if !(c.get::<Params>().can_move)(w, e) || !within_melee(w, e, who) {
        return false;
    }
    visible_contains(w, e, who)
}

fn start(w: &mut World, e: Entity, c: &Ctx) {
    let Some(who) = target(w, e) else { return };
    set(w, e, &LOOK_TARGET, MemValue::Look(crate::brain::Tracker::Entity(who, true)), None);
    let amount = w.get::<AttackDamage>(e).map_or(0.0, |d| d.0);
    crate::hostile::strike(w, e, who, if amount <= 0.0 { 1.0 } else { amount });
    set(w, e, &ATTACK_COOLING_DOWN, MemValue::Bool(true), Some(c.get::<Params>().cooldown));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behaviours::Cap;
    use crate::cow::{ActiveUntil, Herd, SpeciesTable, spawn_species};

    const CAPS: &[Cap] = &[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Locomotion, Cap::Targets, Cap::Goals, Cap::Brain];
    static NEEDS: Needs = &[(&ATTACK_TARGET, Need::Present), (&ATTACK_COOLING_DOWN, Need::Absent)];
    static PARAMS: Params = Params { cooldown: 40, can_move: always };
    fn frozen(_: &World, _: Entity) -> bool {
        false
    }
    static FROZEN: Params = Params { cooldown: 40, can_move: frozen };

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
    fn hits(h: &Herd, prey: Entity) -> usize {
        h.world.resource::<DamageQueue>().0.iter().filter(|x| x.target == prey).count()
    }
    fn armed(h: &mut Herd, e: Entity, prey: Entity, visible: bool) {
        set(&mut h.world, e, &ATTACK_TARGET, MemValue::Who(Who::Entity(prey)), None);
        let seen = if visible { vec![Who::Entity(prey)] } else { vec![] };
        set(&mut h.world, e, &VISIBLE_MOBS, MemValue::Whos(seen), None);
    }

    /// A visible target in reach is struck once and the attack cools down; the next step does not strike again.
    #[test]
    fn it_strikes_a_target_in_reach_once_then_cools_down() {
        let mut h = Herd::with_slab_awake(40, true, false);
        let e = brained(&mut h, &PARAMS);
        let sp = h.world.resource::<SpeciesTable>().0[0];
        let prey = spawn_species(&mut h.world, &sp, 1.5, 0.5, 92);
        armed(&mut h, e, prey, true);
        step(&mut h, e);
        assert_eq!(hits(&h, prey), 1, "témoin : un coup à portée");
        assert!(present(&h.world, e, &ATTACK_COOLING_DOWN), "le coup met en recharge");
        assert!(matches!(look_target(&h.world, e), Some(crate::brain::Tracker::Entity(Who::Entity(p), true)) if p == prey));
        step(&mut h, e);
        assert_eq!(hits(&h, prey), 1, "en recharge : pas de second coup");
    }

    /// Out of reach, or not visible, no blow; the same target in reach and seen does get one.
    #[test]
    fn it_does_not_strike_out_of_reach_or_unseen() {
        let mut h = Herd::with_slab_awake(40, true, false);
        let e = brained(&mut h, &PARAMS);
        let sp = h.world.resource::<SpeciesTable>().0[0];
        let far = spawn_species(&mut h.world, &sp, 6.5, 0.5, 92);
        armed(&mut h, e, far, true);
        step(&mut h, e);
        assert_eq!(hits(&h, far), 0, "hors de portée : pas de coup");
        let near = spawn_species(&mut h.world, &sp, 1.5, 0.5, 93);
        armed(&mut h, e, near, false);
        step(&mut h, e);
        assert_eq!(hits(&h, near), 0, "hors de vue : pas de coup");
        armed(&mut h, e, near, true);
        step(&mut h, e);
        assert_eq!(hits(&h, near), 1, "témoin : à portée et vue, un coup");
    }

    /// A creature that cannot move (the creaking looked at) does not strike; the same creature free does.
    #[test]
    fn a_frozen_attacker_does_not_strike() {
        let mut h = Herd::with_slab_awake(40, true, false);
        let e = brained(&mut h, &FROZEN);
        let sp = h.world.resource::<SpeciesTable>().0[0];
        let prey = spawn_species(&mut h.world, &sp, 1.5, 0.5, 92);
        armed(&mut h, e, prey, true);
        step(&mut h, e);
        assert_eq!(hits(&h, prey), 0, "figé : pas de coup");
        let free = brained(&mut h, &PARAMS);
        armed(&mut h, free, prey, true);
        step(&mut h, free);
        assert_eq!(hits(&h, prey), 1, "témoin : libre, le même coup part");
    }
}
