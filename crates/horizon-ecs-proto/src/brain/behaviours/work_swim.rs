//! `Swim` du villageois (`villager_brain.rs:4032`, tick `:2675`, condition `:3205`) : dans un fluide, il saute avec 80 % de chance
//! à chaque tick, pendant sa durée (60 ticks, celle de l'entrée). Tant qu'il nage, le comportement travaille chaque tick.
use crate::behaviours::Cap;
use crate::brain::*;
use crate::cow::FluidState;
use crate::goals::{with_rng, Jump};
use crate::rules::work_swim as rules;

pub static BEHAVIOUR: Behaviour = Behaviour { name: "minecraft:villager_swim", requires: &[Cap::Body], can_start, tick, ..Behaviour::DEFAULT };

fn in_water(w: &World, e: Entity) -> bool {
    w.get::<FluidState>(e).is_some_and(|f| f.in_water)
}

fn can_start(w: &mut World, e: Entity, _: &Ctx) -> bool {
    in_water(w, e)
}

/// `Swim.tick` : saut à chaque tick avec probabilité 0.8 (`next_float() < 0.8` côté Mob).
fn tick(w: &mut World, e: Entity, _: &Ctx) -> bool {
    if !in_water(w, e) {
        return false;
    }
    let jump = rules::jumps(with_rng(w, e, |r| r.next_f64()));
    if let Some(mut j) = w.get_mut::<Jump>(e) {
        j.0 = jump;
    }
    true
}
