//! `SwimStroll` (`AnimalBrain` `SwimStroll`, `animal_brain.rs:2641-2693`, one kid of the tadpole's idle gate, weight 2): a walk
//! target in a fluid, found by the `Mob`'s tries. The first try draws a default position at (h, v) = (1, 1); each further try
//! extends the last point that was in a fluid by (h, v) in its direction; the search stops at the first point out of the fluid.
//! The start needs the entity in water (`can_start`); "no walk target" is the entry's `needs`.
use crate::behaviours::Cap;
use crate::brain::*;
use crate::cow::{FluidState, Level, Pos};
use crate::spots::{pick_for, Pick};
use horizon_client_lib::blocks::Fluid;
use horizon_client_lib::mobs::data::PathComputationType;

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:swim_stroll",
    mems: &[&WALK_TARGET],
    requires: &[Cap::Body, Cap::Locomotion],
    one_shot: true,
    can_start,
    ..Behaviour::DEFAULT
};

/// The species' values: the walk speed (0.5 for the tadpole, a speed modifier) and the scale of the tries, each (h, v) in
/// blocks (the `Mob`'s list `[(1,1),(3,3),(5,5),(6,5),(7,7),(10,7)]`).
pub struct Params {
    pub speed: f32,
    pub scale: &'static [(i32, i32)],
}
impl Params {
    pub const DEFAULT: Params = Params { speed: 0.5, scale: &[(1, 1), (3, 3), (5, 5), (6, 5), (7, 7), (10, 7)] };
}

fn can_start(w: &mut World, e: Entity, c: &Ctx) -> bool {
    if !w.get::<FluidState>(e).is_some_and(|f| f.in_water) {
        return false;
    }
    let p = c.get::<Params>();
    match swim_point(w, e, p.scale) {
        Some(at) => set(w, e, &WALK_TARGET, MemValue::Walk(WalkTarget { target: Tracker::Point(at), speed: p.speed, close: 0 }), None),
        None => erase(w, e, &WALK_TARGET),
    }
    true
}

/// The `Mob`'s search: the last point of the tries that is still in a fluid (None when the first try finds none).
fn swim_point(w: &mut World, e: Entity, scale: &[(i32, i32)]) -> Option<[f64; 3]> {
    let level = *w.resource::<Level>();
    let mw = level.mw.0;
    let me = w.get::<Pos>(e)?.0;
    let me = [me.x, me.y, me.z];
    let mut fallback: Option<[f64; 3]> = None;
    for &(h, v) in scale {
        let point = match fallback {
            None => first_try(w, e, h, v),
            Some(last) => {
                let d = [last[0] - me[0], last[1] - me[1], last[2] - me[2]];
                let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
                let n = if len < 1.0e-5 { [0.0; 3] } else { d.map(|c| c / len) };
                Some([me[0] + n[0] * f64::from(h), me[1] + n[1] * f64::from(v), me[2] + n[2] * f64::from(h)])
            }
        };
        let Some(point) = point else { break };
        let q = block(point);
        if mw.fluid(q[0], q[1], q[2]) == Fluid::None {
            break;
        }
        fallback = Some(point);
    }
    fallback
}

/// The first try: a default position, drawn again (ten times at most) while its block is not passable by water.
fn first_try(w: &mut World, e: Entity, h: i32, v: i32) -> Option<[f64; 3]> {
    let mw = w.resource::<Level>().mw.0;
    let mut p = pick_for(w, e, Pick::Default { h, v });
    let mut count = 0;
    while let Some(q) = p {
        let b = block(q);
        if mw.data.pathfindable(mw.world.block(b[0], b[1], b[2]), PathComputationType::Water) || count >= 10 {
            break;
        }
        count += 1;
        p = pick_for(w, e, Pick::Default { h, v });
    }
    p
}

fn block(p: [f64; 3]) -> [i32; 3] {
    [p[0].floor() as i32, p[1].floor() as i32, p[2].floor() as i32]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cow::{ActiveUntil, Herd, SpeciesTable, spawn_species};
    use horizon_core::geometry::Vec3;

    const CAPS: &[Cap] = &[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Locomotion, Cap::Targets, Cap::Goals, Cap::Brain];
    static NEEDS: Needs = &[(&WALK_TARGET, Need::Absent)];
    static ENTRIES: [BEntry; 1] = [BEntry { behaviour: &BEHAVIOUR, activity: "core", priority: 2, params: &Params::DEFAULT, needs: NEEDS, duration: None, kids: &[] }];

    /// A pool of water above the slab (x 38..=46, z -2..=6, y 0..=2); None without the prepared resources (the fluids
    /// and the path data come from them).
    fn world() -> Option<Herd> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = Herd::with_slab_awake(40, true, false);
        for x in 38..=46 {
            for z in -2..=6 {
                for y in 0..=2 {
                    h.set_block(x, y, z, "minecraft:water");
                }
            }
        }
        h.tick();
        Some(h)
    }
    fn brained(h: &mut Herd, x: f64, y: f64, z: f64) -> Entity {
        let def = BrainDef {
            sensors: &[],
            activities: &[ActivityDef { name: "core", slot: 1, requires: &[] }],
            pick: &[],
            initial: &[],
            behaviours: &ENTRIES,
        };
        let plan: &'static Plan = Box::leak(Box::new(Plan::build(&def, CAPS).unwrap()));
        let sp = h.world.resource::<SpeciesTable>().0[0];
        let e = spawn_species(&mut h.world, &sp, x, z, 77);
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        h.teleport(e, Vec3::new(x, y, z));
        // A swimmer's own path maluses: water costs nothing (the species' navigation, not this module's).
        h.world.get_mut::<NavParams>(e).unwrap().malus[horizon_client_lib::mobs::path::PathType::Water as usize] = Some(0.0);
        // The tadpole swims: its active mode is the water's (its navigation is a swimmer's, its stable destination is not a walker's).
        h.world.entity_mut(e).remove::<crate::locomotion::WalkMode>().insert(crate::locomotion::SwimMode);
        h.world.entity_mut(e).insert(bundle(plan));
        start(&mut h.world, e);
        e
    }
    fn set_in_water(h: &mut Herd, e: Entity, on: bool) {
        h.world.get_mut::<FluidState>(e).unwrap().in_water = on;
        wake(&mut h.world, e);
        brain_step(&mut h.world);
    }
    fn walk(h: &Herd, e: Entity) -> Option<WalkTarget> {
        match get(&h.world, e, &WALK_TARGET) {
            Some(MemValue::Walk(t)) => Some(*t),
            _ => None,
        }
    }

    /// In water the entity gets a walk target at 0.5 (close 0), and the point is in a fluid, as the `Mob`'s search requires.
    #[test]
    fn it_strolls_to_a_point_in_the_water() {
        let Some(mut h) = world() else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        let e = brained(&mut h, 42.5, 0.0, 2.5);
        set_in_water(&mut h, e, true);
        let t = walk(&h, e).expect("témoin : dans l'eau, un point de marche est tiré");
        assert_eq!((t.speed, t.close), (0.5, 0));
        let Tracker::Point(p) = t.target else { panic!("un point") };
        let mw = h.world.resource::<Level>().mw.0;
        let b = block(p);
        assert_eq!(mw.fluid(b[0], b[1], b[2]), Fluid::Water, "le point est dans l'eau");
    }

    /// Out of water it does not start; once in water again it does (the start depends on the fluid, not on a stale state).
    #[test]
    fn it_does_not_start_out_of_the_water() {
        let Some(mut h) = world() else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        let e = brained(&mut h, 42.5, 0.0, 2.5);
        set_in_water(&mut h, e, false);
        assert!(walk(&h, e).is_none(), "hors de l'eau, pas de départ");
        set_in_water(&mut h, e, true);
        assert!(walk(&h, e).is_some(), "témoin : dans l'eau, la même entité part");
    }
}
