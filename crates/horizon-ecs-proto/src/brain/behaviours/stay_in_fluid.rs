//! `StayInFluid` (`AxolotlAction::StayWater` / `StayGround`, the kids of the tadpole's idle gate): a one-shot that can start while the
//! entity is in water (`StayWater`: `fluids.in_water`) or on the ground (`StayGround`: `on_ground`). It does nothing when it runs:
//! the `Mob` lists both as one-shots (`animal_brain.rs` `one_shot`), so they run for their start tick only.
use crate::behaviours::Cap;
use crate::brain::*;
use crate::cow::{Contact, FluidState};

pub static BEHAVIOUR: Behaviour = Behaviour { name: "minecraft:stay_in_fluid", one_shot: true, requires: &[Cap::Body], can_start, ..Behaviour::DEFAULT };

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stay {
    Water,
    Ground,
}

pub struct Params {
    pub what: Stay,
}
impl Params {
    pub const DEFAULT: Params = Params { what: Stay::Water };
}

fn can_start(w: &mut World, e: Entity, ctx: &Ctx) -> bool {
    match ctx.get::<Params>().what {
        Stay::Water => w.get::<FluidState>(e).is_some_and(|f| f.in_water),
        Stay::Ground => w.get::<Contact>(e).is_some_and(|c| c.on_ground),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cow::{spawn_species, Herd, SpeciesTable};

    fn ctx(p: &'static Params) -> Ctx {
        Ctx { id: 0, nth: 0, params: p }
    }
    /// In water the water kid can start and the ground kid cannot; on the ground it is the other way round.
    #[test]
    fn it_starts_in_water_or_on_the_ground_as_its_parameter_says() {
        static WATER: Params = Params { what: Stay::Water };
        static GROUND: Params = Params { what: Stay::Ground };
        let mut h = Herd::with_slab_awake(40, true, false);
        let sp = h.world.resource::<SpeciesTable>().0[0];
        let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 5);
        h.world.get_mut::<FluidState>(e).unwrap().in_water = true;
        h.world.get_mut::<Contact>(e).unwrap().on_ground = false;
        assert!((BEHAVIOUR.can_start)(&mut h.world, e, &ctx(&WATER)));
        assert!(!(BEHAVIOUR.can_start)(&mut h.world, e, &ctx(&GROUND)));
        h.world.get_mut::<FluidState>(e).unwrap().in_water = false;
        h.world.get_mut::<Contact>(e).unwrap().on_ground = true;
        assert!(!(BEHAVIOUR.can_start)(&mut h.world, e, &ctx(&WATER)));
        assert!((BEHAVIOUR.can_start)(&mut h.world, e, &ctx(&GROUND)));
    }
}
