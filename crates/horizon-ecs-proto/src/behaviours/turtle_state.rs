//! The turtle's state, shared by its goals (fiches 47 to 51). Not a module: only the water goal's agent writes this file, the
//! other turtle goals read and write the fields through these functions (as `panda_state.rs`).
use super::*;
use crate::cow::Pos;
use crate::rules::teleport_on::cell_of;
use crate::rules::turtle_state::near_home_centre;

#[derive(Component, Clone, Copy, Debug, Default)]
#[component(on_insert = crate::bridge::view::touched, on_remove = crate::bridge::view::touched)]
pub struct Turtle {
    pub home: [i32; 3],
    pub laying: bool,
    pub laying_ticks: i32,
    pub going_home: bool,
    /// The spot of the current trip in water (`Turtle.travelPos`).
    pub travel: Option<[i32; 3]>,
    /// `in_love` set to 600 by the laying of an egg (`TurtleLayEggGoal`); its countdown belongs to the breeding goal (not ported).
    pub in_love: u64,
}

/// Gives the animal its state if it has none; its home is the block it stands in (`homePos` when it is set at spawn).
pub fn ensure(w: &mut World, e: Entity) {
    if w.get::<Turtle>(e).is_none() {
        let home = w.get::<Pos>(e).map_or([0; 3], |p| cell_of(p.0));
        w.entity_mut(e).insert(Turtle { home, ..Default::default() });
    }
}

/// `hasEgg`: the turtle carries the egg of `lay_egg` (one fact, `Carrying`, not a copy).
pub fn has_egg(w: &World, e: Entity) -> bool {
    w.get::<super::lay_egg::Carrying>(e).is_some()
}

pub fn get(w: &World, e: Entity) -> Turtle {
    w.get::<Turtle>(e).copied().unwrap_or_default()
}

pub fn update(w: &mut World, e: Entity, f: impl FnOnce(&mut Turtle)) {
    if let Some(mut t) = w.get_mut::<Turtle>(e) {
        f(&mut t);
        drop(t);
        crate::bridge::view::mark(w, e);
    }
}

/// The same test as `close_to_home`, for a state already read (a system that holds the turtle's query, not the world).
pub fn is_near_home(t: &Turtle, p: [f64; 3], distance: f64) -> bool {
    near_home_centre(t.home, p, distance)
}

/// `homePos.closerToCenterThan(position, distance)`: the centre of the home block is nearer than `distance`.
pub fn close_to_home(w: &World, e: Entity, distance: f64) -> bool {
    let (Some(t), Some(p)) = (w.get::<Turtle>(e), w.get::<Pos>(e)) else { return false };
    near_home_centre(t.home, [p.0.x, p.0.y, p.0.z], distance)
}
