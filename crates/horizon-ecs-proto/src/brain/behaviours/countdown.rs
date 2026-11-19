//! `CountDown*`: the memory (an integer) goes down by one each tick; at zero it is erased and the behaviour stops.
use crate::brain::*;
use crate::cow::*;

#[derive(Clone, Copy)]
pub struct Params {
    pub key: &'static MemKey,
}
pub static BEHAVIOUR: Behaviour = Behaviour { name: "minecraft:count_down", tick, stop, ..Behaviour::DEFAULT };
fn key(c: &Ctx) -> &'static MemKey {
    c.get::<Params>().key
}
fn tick(w: &mut World, e: Entity, c: &Ctx) -> bool {
    match int(w, e, key(c)) {
        Some(v) if v > 0 => {
            set(w, e, key(c), MemValue::Int(v - 1), None);
            true
        }
        _ => false,
    }
}
fn stop(w: &mut World, e: Entity, c: &Ctx) {
    erase(w, e, key(c));
}
