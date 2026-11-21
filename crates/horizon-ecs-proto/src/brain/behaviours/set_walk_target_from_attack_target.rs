//! `SetWalkTargetFromAttackTarget` (`monster_brain.rs` `B::SetWalkTargetFromAttackTarget`, `:775-794`; creaking Fight 10 at
//! `:1535`): a one-shot of the Fight activity. While there is an attack target, it walks to it, looking at it, unless the
//! target is seen and within melee reach, then it clears the walk target. The attack target's memory is the entry's need.
use crate::behaviours::Cap;
use crate::brain::*;

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:set_walk_target_from_attack_target",
    mems: &[&ATTACK_TARGET, &VISIBLE_MOBS, &WALK_TARGET, &LOOK_TARGET],
    requires: &[Cap::Brain, Cap::Locomotion, Cap::Targets],
    one_shot: true,
    start,
    ..Behaviour::DEFAULT
};

/// The walk's speed (`WalkTarget` speed 1.0 in the `Mob`).
pub struct Params {
    pub speed: f32,
    /// The speed when the entity is out of water (the axolotl's chase: 0.15, 0.6 in water); none: `speed` always.
    pub dry_speed: Option<f32>,
}
impl Params {
    pub const DEFAULT: Params = Params { speed: 1.0, dry_speed: None };
}

fn start(w: &mut World, e: Entity, c: &Ctx) {
    let target = match get(w, e, &ATTACK_TARGET) {
        Some(MemValue::Who(who)) => *who,
        _ => return,
    };
    let seen = present(w, e, &VISIBLE_MOBS) && visible_contains(w, e, target);
    if seen && super::melee_attack::within_melee(w, e, target) {
        erase(w, e, &WALK_TARGET);
    } else {
        set(w, e, &LOOK_TARGET, MemValue::Look(Tracker::Entity(target, true)), None);
        let p = c.get::<Params>();
        let wet = w.get::<crate::cow::FluidState>(e).is_some_and(|f| f.in_water);
        let walk = WalkTarget { target: Tracker::Entity(target, false), speed: if wet { p.speed } else { p.dry_speed.unwrap_or(p.speed) }, close: 0 };
        set(w, e, &WALK_TARGET, MemValue::Walk(walk), None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behaviours::Cap;
    use crate::cow::{ActiveUntil, Herd, SpeciesTable, spawn_species};
    use crate::targeting::Who;

    const CAPS: &[Cap] = &[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Locomotion, Cap::Targets, Cap::Goals, Cap::Brain];
    static NEEDS: Needs = &[(&ATTACK_TARGET, Need::Present)];
    static PARAMS: Params = Params { speed: 1.0, dry_speed: None };

    fn brained(h: &mut Herd) -> Entity {
        let entries: &'static [BEntry] = Box::leak(Box::new([BEntry { behaviour: &BEHAVIOUR, activity: "core", priority: 1, params: &PARAMS, needs: NEEDS, duration: None, kids: &[] }]));
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

    /// A seen target 6 blocks away: it is looked at and walked to, at speed 1.0 and close 0.
    #[test]
    fn it_walks_to_a_seen_target_out_of_reach() {
        let mut h = Herd::with_slab_awake(40, true, false);
        let e = brained(&mut h);
        let sp = h.world.resource::<SpeciesTable>().0[0];
        let prey = spawn_species(&mut h.world, &sp, 6.5, 0.5, 92);
        set(&mut h.world, e, &ATTACK_TARGET, MemValue::Who(Who::Entity(prey)), None);
        set(&mut h.world, e, &VISIBLE_MOBS, MemValue::Whos(vec![Who::Entity(prey)]), None);
        step(&mut h, e);
        assert_eq!(walk_target(&h.world, e), Some(WalkTarget { target: Tracker::Entity(Who::Entity(prey), false), speed: 1.0, close: 0 }));
        assert_eq!(look_target(&h.world, e), Some(Tracker::Entity(Who::Entity(prey), true)));
    }

    /// Seen and within reach: the walk target is cleared (a walk left over from before is erased); not seen, the walk is set.
    #[test]
    fn it_stops_walking_when_seen_and_in_reach_and_walks_when_unseen() {
        let mut h = Herd::with_slab_awake(40, true, false);
        let e = brained(&mut h);
        let sp = h.world.resource::<SpeciesTable>().0[0];
        let near = spawn_species(&mut h.world, &sp, 1.5, 0.5, 92);
        set(&mut h.world, e, &ATTACK_TARGET, MemValue::Who(Who::Entity(near)), None);
        set(&mut h.world, e, &VISIBLE_MOBS, MemValue::Whos(vec![Who::Entity(near)]), None);
        set(&mut h.world, e, &WALK_TARGET, MemValue::Walk(WalkTarget { target: Tracker::Point([0.0, 1.0, 0.0]), speed: 1.0, close: 0 }), None);
        step(&mut h, e);
        assert_eq!(walk_target(&h.world, e), None, "vue et à portée : la marche est effacée");
        set(&mut h.world, e, &VISIBLE_MOBS, MemValue::Whos(vec![]), None);
        step(&mut h, e);
        assert!(walk_target(&h.world, e).is_some(), "témoin : hors de vue, la marche vers la cible est posée");
    }
}
