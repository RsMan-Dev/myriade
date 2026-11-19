//! `HurtBySensor`: what hurt the entity in the last 40 ticks (whether it makes it panic, and who), seen at the next scan.
use crate::brain::*;
use crate::cow::*;

pub static SENSOR: Sensor = Sensor { name: "minecraft:hurt_by", mems: &[&HURT_BY, &HURT_BY_ENTITY], scan, ..Sensor::DEFAULT };

fn scan(w: &mut World, e: Entity, _: &Ctx) {
    let now = now_of(w, e);
    let last = w.get::<LastDamage>(e).copied().filter(|d| now.saturating_sub(d.at) <= 40);
    if let Some(d) = last {
        set(w, e, &HURT_BY, MemValue::Bool(d.panic), None);
        if let Some(by) = d.by {
            set(w, e, &HURT_BY_ENTITY, MemValue::Who(by), None);
        }
    } else {
        erase(w, e, &HURT_BY);
    }
    let gone = matches!(get(w, e, &HURT_BY_ENTITY), Some(MemValue::Who(who)) if crate::targeting::info(w, *who).is_none());
    if gone {
        erase(w, e, &HURT_BY_ENTITY);
    }
}
