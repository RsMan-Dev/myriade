//! `WalkFromLook` / `TadpoleWalkFromLook` (`animal_brain.rs:2700-2731`, one kid of the tadpole's idle gate, weight 3): the entity
//! walks to the place it looks at, copied as its walk target. The axolotl's version (`needs_same_fluid`: the look target is in
//! the fluid the entity is in, its own speed) is the same module with other parameters. "No walk target" and "a look target
//! present" are the entry's `needs`.
use crate::behaviours::Cap;
use crate::brain::*;
use crate::cow::{FluidState, Level};
use horizon_client_lib::blocks::Fluid;

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:walk_to_look_target",
    mems: &[&LOOK_TARGET, &WALK_TARGET],
    requires: &[Cap::Body, Cap::Locomotion],
    one_shot: true,
    can_start,
    ..Behaviour::DEFAULT
};

/// The species' values: the walk speed (0.5 for the tadpole), the close-enough distance in blocks (3), and whether the
/// look target must be in the fluid the entity is in (the axolotl: true).
pub struct Params {
    pub speed: f32,
    pub close: i32,
    pub needs_same_fluid: bool,
}
impl Params {
    pub const DEFAULT: Params = Params { speed: 0.5, close: 3, needs_same_fluid: false };
}

fn can_start(w: &mut World, e: Entity, c: &Ctx) -> bool {
    let p = c.get::<Params>();
    let Some(look) = look_target(w, e) else { return false };
    if p.needs_same_fluid {
        let Some(b) = look.block(w) else { return false };
        let mw = w.resource::<Level>().mw.0;
        let water = mw.fluid(b[0], b[1], b[2]) == Fluid::Water;
        if water != w.get::<FluidState>(e).is_some_and(|f| f.in_water) {
            return false;
        }
    }
    set(w, e, &WALK_TARGET, MemValue::Walk(WalkTarget { target: look, speed: p.speed, close: p.close }), None);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cow::{ActiveUntil, Herd, SpeciesTable, spawn_species};

    const CAPS: &[Cap] = &[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Locomotion, Cap::Targets, Cap::Goals, Cap::Brain];
    static NEEDS: Needs = &[(&WALK_TARGET, Need::Absent), (&LOOK_TARGET, Need::Present)];
    static ENTRIES: [BEntry; 1] = [BEntry { behaviour: &BEHAVIOUR, activity: "core", priority: 3, params: &Params::DEFAULT, needs: NEEDS, duration: None, kids: &[] }];
    static SAME_FLUID: Params = Params { speed: 0.5, close: 3, needs_same_fluid: true };

    fn brained(h: &mut Herd) -> Entity {
        let def = BrainDef {
            sensors: &[],
            activities: &[ActivityDef { name: "core", slot: 1, requires: &[] }],
            pick: &[],
            initial: &[],
            behaviours: &ENTRIES,
        };
        let plan: &'static Plan = Box::leak(Box::new(Plan::build(&def, CAPS).unwrap()));
        let sp = h.world.resource::<SpeciesTable>().0[0];
        let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 78);
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        h.world.entity_mut(e).insert(bundle(plan));
        start(&mut h.world, e);
        e
    }
    fn step(h: &mut Herd, e: Entity) {
        wake(&mut h.world, e);
        brain_step(&mut h.world);
    }

    /// With a look target and no walk target, the walk target is the look target at 0.5, close 3, the same tick.
    #[test]
    fn it_walks_to_what_it_looks_at() {
        let mut h = Herd::with_slab(40, true, false);
        let e = brained(&mut h);
        let at = Tracker::Point([10.5, 1.0, 0.5]);
        set(&mut h.world, e, &LOOK_TARGET, MemValue::Look(at), None);
        step(&mut h, e);
        assert_eq!(walk_target(&h.world, e), Some(WalkTarget { target: at, speed: 0.5, close: 3 }));
    }

    /// Nothing without a look target; and a walk target that exists is never replaced by the look target.
    #[test]
    fn it_does_nothing_without_a_look_target_or_over_a_walk_target() {
        let mut h = Herd::with_slab(40, true, false);
        let e = brained(&mut h);
        step(&mut h, e);
        assert!(walk_target(&h.world, e).is_none(), "sans cible de regard, rien");
        let first = Tracker::Point([10.5, 1.0, 0.5]);
        set(&mut h.world, e, &LOOK_TARGET, MemValue::Look(first), None);
        step(&mut h, e);
        assert!(walk_target(&h.world, e).is_some(), "témoin : la cible de regard devient la cible de marche");
        set(&mut h.world, e, &LOOK_TARGET, MemValue::Look(Tracker::Point([-9.5, 1.0, 0.5])), None);
        step(&mut h, e);
        assert_eq!(walk_target(&h.world, e).map(|t| t.target), Some(first), "une cible de marche existante est gardée");
    }

    /// The axolotl's test: a look target in air with the entity in water does not start the walk; the same entity out of
    /// the water, looking at air, does.
    #[test]
    fn a_same_fluid_walk_needs_the_look_target_in_the_entitys_fluid() {
        let mut h = Herd::with_slab(40, true, false);
        let e = brained(&mut h);
        let ctx = Ctx { id: 0, nth: 0, params: &SAME_FLUID };
        h.world.get_mut::<FluidState>(e).unwrap().in_water = true;
        set(&mut h.world, e, &LOOK_TARGET, MemValue::Look(Tracker::Point([10.5, 1.0, 0.5])), None);
        assert!(!(BEHAVIOUR.can_start)(&mut h.world, e, &ctx), "air visé, entité dans l'eau : pas de départ");
        h.world.get_mut::<FluidState>(e).unwrap().in_water = false;
        assert!((BEHAVIOUR.can_start)(&mut h.world, e, &ctx), "témoin : air visé, entité hors de l'eau : départ");
    }
}
