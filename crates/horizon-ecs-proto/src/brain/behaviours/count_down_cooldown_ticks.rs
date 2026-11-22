//! `CountDownCooldownTicks` (`CopperGolemAi`, the gaze cooldown): it runs while its key's memory is present and positive, takes one off
//! each tick, and forgets the memory when it ends. Any cooldown counted by the brain is a `Params` key.
use crate::behaviours::Cap;
use crate::brain::*;

pub static BEHAVIOUR: Behaviour = Behaviour { name: "minecraft:count_down_cooldown_ticks", mems: &[], requires: &[Cap::Brain], can_start, tick, stop, ..Behaviour::DEFAULT };

pub struct Params {
    pub key: &'static MemKey,
}

fn can_start(w: &mut World, e: Entity, c: &Ctx) -> bool {
    present(w, e, c.get::<Params>().key)
}

fn tick(w: &mut World, e: Entity, c: &Ctx) -> bool {
    let key = c.get::<Params>().key;
    match int(w, e, key) {
        Some(v) if v > 0 => {
            set(w, e, key, MemValue::Int(v - 1), None);
            true
        }
        _ => false,
    }
}

fn stop(w: &mut World, e: Entity, c: &Ctx) {
    erase(w, e, c.get::<Params>().key);
}
