//! The turtle's state, shared by its goals (fiches 47 to 51). Not a module: only the water goal's agent writes this file, the
//! other turtle goals read and write the fields through these functions (as `panda_state.rs`).
use super::*;
use crate::cow::Pos;

#[derive(Component, Clone, Copy, Debug, Default)]
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
        let p = w.get::<Pos>(e).map_or([0.0; 3], |p| [p.0.x, p.0.y, p.0.z]);
        let home = [p[0].floor() as i32, p[1].floor() as i32, p[2].floor() as i32];
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
    }
}

/// The same test as `close_to_home`, for a state already read (a system that holds the turtle's query, not the world).
pub fn is_near_home(t: &Turtle, p: [f64; 3], distance: f64) -> bool {
    let d = [f64::from(t.home[0]) + 0.5 - p[0], f64::from(t.home[1]) + 0.5 - p[1], f64::from(t.home[2]) + 0.5 - p[2]];
    d[0] * d[0] + d[1] * d[1] + d[2] * d[2] < distance * distance
}

/// `homePos.closerToCenterThan(position, distance)`: the centre of the home block is nearer than `distance`.
pub fn close_to_home(w: &World, e: Entity, distance: f64) -> bool {
    let (Some(t), Some(p)) = (w.get::<Turtle>(e), w.get::<Pos>(e)) else { return false };
    let d = [f64::from(t.home[0]) + 0.5 - p.0.x, f64::from(t.home[1]) + 0.5 - p.0.y, f64::from(t.home[2]) + 0.5 - p.0.z];
    d[0] * d[0] + d[1] * d[1] + d[2] * d[2] < distance * distance
}
