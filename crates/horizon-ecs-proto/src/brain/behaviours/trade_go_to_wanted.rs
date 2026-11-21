//! `GoToWantedItem` (`villager_brain.rs:1828`, `:4052`, `:4483`): with no walk target, the nearest pickable item within 4 blocks whose kind
//! is in the item tag `minecraft:villager_picks_up` is walked to (speed 0.5, reach 0) and looked at at its eye height (0.2125). At arrival,
//! within reach, it is taken into the main hand. The pick-up is `item::pick_up`'s, the tag is `items::has_tag`'s.
use crate::brain::*;
use crate::cow::Pos;
use crate::goals::set_main_hand;
use crate::item::{self, Item};
use crate::items::{has_tag, item_id};
use horizon_core::geometry::{Aabb, Vec3};

/// The item tag a villager takes (`game/mobs/villagers.rs:315`).
const PICKS_UP: &str = "minecraft:villager_picks_up";

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:villager_go_to_wanted_item",
    mems: &[&WALK_TARGET, &LOOK_TARGET],
    wakes_on_item: 4.0,
    can_start,
    start,
    tick,
    ..Behaviour::DEFAULT
};

/// The nearest pickable item of the tag within 4 blocks.
fn wanted_near(w: &World, e: Entity) -> Option<Entity> {
    let at = w.get::<Pos>(e)?.0;
    let r = 4.0;
    let area = Aabb::new(Vec3::new(at.x - r, at.y - r, at.z - r), Vec3::new(at.x + r, at.y + r, at.z + r));
    let dist = |it: Entity| w.get::<Pos>(it).map_or(f64::MAX, |q| q.0.distance_squared(at));
    item::items_in(w, area)
        .into_iter()
        .filter(|&it| item::pickable(w, it) && w.get::<Item>(it).is_some_and(|i| has_tag(PICKS_UP, item_id(&i.stack.item))) && dist(it) < 16.0)
        .min_by(|&a, &b| dist(a).total_cmp(&dist(b)))
}

fn can_start(w: &mut World, e: Entity, _: &Ctx) -> bool {
    walk_target(w, e).is_none() && wanted_near(w, e).is_some()
}

fn start(w: &mut World, e: Entity, _: &Ctx) {
    let Some(it) = wanted_near(w, e) else { return };
    let Some(at) = w.get::<Pos>(it).map(|p| p.0) else { return };
    set(w, e, &LOOK_TARGET, MemValue::Look(Tracker::Point([at.x, at.y + 0.2125, at.z])), None);
    let walk = WalkTarget { target: Tracker::Point([at.x, at.y, at.z]), speed: 0.5, close: 0 };
    set(w, e, &WALK_TARGET, MemValue::Walk(walk), None);
}

/// Walking until the path is done; then the item in reach (2.25 squared) is taken and the behaviour ends.
fn tick(w: &mut World, e: Entity, _: &Ctx) -> bool {
    // The walk target is the sink's to take: until it has and the path is done, the walk is under way.
    if walk_target(w, e).is_some() || !nav_done(w, e) {
        return true;
    }
    erase(w, e, &WALK_TARGET);
    let Some(me) = w.get::<Pos>(e).map(|p| p.0) else { return false };
    let reached = wanted_near(w, e).filter(|&it| w.get::<Pos>(it).is_some_and(|q| q.0.distance_squared(me) <= 2.25));
    if let Some(stack) = reached.and_then(|it| item::pick_up(w, e, it, u32::MAX)) {
        set_main_hand(w, e, Some(stack));
    }
    false
}
