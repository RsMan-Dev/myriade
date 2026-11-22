//! `NearestItemSensor` (`villager_brain.rs` `take_item_scan`, `set_wanted_item`): every scan, the nearest pickable item of the tag
//! within the range is remembered as `NEAREST_WANTED_ITEM` (erased when there is none). The tag and the range are the species' parameters.
//! The search goes through the items index; a sleeping creature is not stepped, so its sensor does not run.
use crate::brain::*;
use crate::cow::Pos;
use crate::item::{self, Item};
use crate::items::{has_tag, item_id};
use crate::targeting::Who;
use horizon_core::geometry::{Aabb, Vec3};

/// The item entity the creature wants to take.
pub static NEAREST_WANTED_ITEM: MemKey = MemKey::new("minecraft:nearest_wanted_item", Shape::Who);

pub struct Params {
    pub tag: &'static str,
    pub range: f64,
}

pub static SENSOR: Sensor = Sensor { name: "minecraft:nearest_wanted_item", mems: &[&NEAREST_WANTED_ITEM], scan, ..Sensor::DEFAULT };

fn scan(w: &mut World, e: Entity, c: &Ctx) {
    let p = c.get::<Params>();
    match nearest(w, e, p) {
        Some(it) => set(w, e, &NEAREST_WANTED_ITEM, MemValue::Who(Who::Entity(it)), None),
        None => erase(w, e, &NEAREST_WANTED_ITEM),
    }
}

/// The nearest pickable item of the tag within the range (squared distance, the box of the range).
fn nearest(w: &World, e: Entity, p: &Params) -> Option<Entity> {
    let at = w.get::<Pos>(e)?.0;
    let r = p.range;
    let area = Aabb::new(Vec3::new(at.x - r, at.y - r, at.z - r), Vec3::new(at.x + r, at.y + r, at.z + r));
    let dist = |it: Entity| w.get::<Pos>(it).map_or(f64::MAX, |q| q.0.distance_squared(at));
    item::items_in(w, area)
        .into_iter()
        .filter(|&it| item::pickable(w, it) && w.get::<Item>(it).is_some_and(|i| has_tag(p.tag, item_id(&i.stack.item))) && dist(it) < r * r)
        .min_by(|&a, &b| dist(a).total_cmp(&dist(b)))
}
