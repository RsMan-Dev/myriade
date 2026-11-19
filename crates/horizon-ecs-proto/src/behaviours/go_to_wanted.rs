//! `GoToWantedItem` (`mobs/piglin.rs:1807-1826`): an item of the wanted kinds within `range` blocks, that can be picked up,
//! is walked to (reach 1) and taken into the main hand at arrival. The search runs when the selector asks this goal.
use super::*;
use crate::{cow::*, goals::*, item};
use horizon_client_lib::items::ItemStack;
use horizon_core::geometry::{Aabb, Vec3};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub items: &'static [&'static str],
    pub range: f64,
    pub speed: f64,
}
impl Params {
    pub const DEFAULT: Self = Self { items: &[], range: 8.0, speed: 1.0 };
}
/// The item walked to.
#[derive(Component, Clone, Copy, Debug)]
pub struct Wanted {
    pub p: Params,
    pub item: Option<Entity>,
}

pub static MODULE: Module = Module {
    name: "go_to_wanted",
    flags: MOVE,
    ends_with_path: true,
    equip,
    can_use,
    start,
    stop,
    requires: &[Cap::Body, Cap::Goals, Cap::Locomotion],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, _: Inst, p: &(dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Wanted { p: *params::<Params>(p), item: None });
}
/// The nearest pickable item of a wanted kind within `range`.
fn wanted_near(w: &World, e: Entity, p: &Params) -> Option<Entity> {
    let at = w.get::<Pos>(e)?.0;
    let r = p.range;
    let area = Aabb::new(Vec3::new(at.x - r, at.y - r, at.z - r), Vec3::new(at.x + r, at.y + r, at.z + r));
    item::items_in(w, area)
        .into_iter()
        .filter(|&it| item::pickable(w, it) && item::kind_of(w, it).is_some_and(|k| p.items.contains(&k)))
        .min_by(|&a, &b| {
            let d = |it: Entity| w.get::<Pos>(it).map_or(f64::MAX, |q| q.0.distance_squared(at));
            d(a).total_cmp(&d(b))
        })
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let p = w.get::<Wanted>(e).unwrap().p;
    wanted_near(w, e, &p).is_some()
}
fn start(w: &mut World, e: Entity, _: Inst) {
    let p = w.get::<Wanted>(e).unwrap().p;
    let Some(it) = wanted_near(w, e, &p) else { return };
    let at = w.get::<Pos>(it).unwrap().0;
    go_to_at(w, e, [at.x, at.y, at.z], p.speed, 1);
    w.get_mut::<Wanted>(e).unwrap().item = Some(it);
}
/// At the end of the path, the item in reach is taken into the main hand.
fn stop(w: &mut World, e: Entity, _: Inst) {
    stop_nav(w, e);
    let Some(it) = w.get_mut::<Wanted>(e).and_then(|mut wd| wd.item.take()) else { return };
    let (Some(me), Some(q)) = (w.get::<Pos>(e).map(|p| p.0), w.get::<Pos>(it).map(|p| p.0)) else { return };
    if q.distance_squared(me) > 2.25 || !item::pickable(w, it) {
        return;
    }
    if let Some(stack) = item::pick_up(w, e, it, u32::MAX) {
        set_main_hand(w, e, Some(stack as ItemStack));
    }
}
