//! `GoToWantedItem` (`mobs/piglin.rs:1807-1826`): an item of the wanted kinds within `range` blocks, that can be picked up,
//! is walked to (reach 1) and taken into the main hand at arrival. The search runs when the selector asks this goal.
//! `hand` (the allay's `Fetch`): the wanted item is the one the main hand holds and the pocket takes, and it goes into the pocket.
//! Woken by a dropped item near the creature (`Events.item_dropped`): no poll.
use super::*;
use crate::pocket::{self, Pocket};
use crate::{cow::*, goals::*, item};
use horizon_client_lib::items::ItemStack;
use horizon_core::geometry::{Aabb, Vec3};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub items: &'static [&'static str],
    pub range: f64,
    pub speed: f64,
    pub hand: bool,
}
impl Params {
    pub const DEFAULT: Self = Self { items: &[], range: 8.0, speed: 1.0, hand: false };
}
/// The item walked to, and the slot of the goal (the wake event starts it).
#[derive(Component, Clone, Copy, Debug)]
pub struct Wanted {
    pub p: &'static Params,
    pub item: Option<Entity>,
    slot: u8,
}
/// The most a pocket takes of one stack: the stack size of the items the allay takes (64 here, the registry is not read).
const MAX_PICK: u32 = 64;
/// The widest `range` of the goals that use `hand`: a dropped item farther than this does not wake anyone.
const WAKE_RANGE: f64 = 32.0;

pub static MODULE: Module = Module {
    name: "go_to_wanted",
    flags: MOVE,
    ends_with_path: true,
    equip,
    can_use,
    start,
    stop,
    systems,
    requires: &[Cap::Body, Cap::Goals, Cap::Locomotion],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Wanted { p: params::<Params>(p), item: None, slot: inst.slot });
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems(wake_on_drop.in_set(slot));
}
/// A dropped item wakes the `hand` creatures whose `range` box holds it.
fn wake_on_drop(ev: Res<Events>, items: Query<&Pos, With<item::Item>>, idx: Res<Index>, q: Query<(&Wanted, &Pos)>, mut reqs: ResMut<Requests>) {
    for i in ev.item_dropped.iter() {
        let Ok(at) = items.get(i) else { continue };
        idx.for_radius(at.0.x, at.0.z, WAKE_RANGE, |c| {
            let Ok((wd, me)) = q.get(c) else { return };
            let r = wd.p.range;
            if wd.p.hand && (me.0.x - at.0.x).abs() <= r && (me.0.y - at.0.y).abs() <= r && (me.0.z - at.0.z).abs() <= r {
                reqs.0.push(Req::Start(c, wd.slot));
            }
        });
    }
}
/// The pocket's room for `stack` (an empty pocket when the creature has none yet).
fn room_for(w: &World, e: Entity, stack: &ItemStack) -> u32 {
    w.get::<Pocket>(e).map_or(MAX_PICK, |pk| pk.room(stack, MAX_PICK))
}
/// The item is one the goal walks to: of a wanted kind, or with `hand`, the same as the main hand and room in the pocket.
fn accepts(w: &World, e: Entity, p: &Params, it: Entity) -> bool {
    let Some(stack) = w.get::<item::Item>(it).map(|i| &i.stack) else { return false };
    if !p.hand {
        return item::kind_of(w, it).is_some_and(|k| p.items.contains(&k));
    }
    main_hand(w, e).is_some_and(|h| pocket::considers_item_equal(h, stack)) && room_for(w, e, stack) > 0
}
/// The nearest pickable item the goal wants within `range`.
fn wanted_near(w: &World, e: Entity, p: &Params) -> Option<Entity> {
    let at = w.get::<Pos>(e)?.0;
    let r = p.range;
    let area = Aabb::new(Vec3::new(at.x - r, at.y - r, at.z - r), Vec3::new(at.x + r, at.y + r, at.z + r));
    item::items_in(w, area)
        .into_iter()
        .filter(|&it| item::pickable(w, it) && accepts(w, e, p, it))
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
/// At the end of the path, the item in reach is taken: into the main hand, or into the pocket with `hand`.
fn stop(w: &mut World, e: Entity, _: Inst) {
    stop_nav(w, e);
    let Some((it, p)) = w.get_mut::<Wanted>(e).and_then(|mut wd| wd.item.take().map(|it| (it, wd.p))) else { return };
    let (Some(me), Some(q)) = (w.get::<Pos>(e).map(|p| p.0), w.get::<Pos>(it).map(|p| p.0)) else { return };
    if q.distance_squared(me) > 2.25 || !item::pickable(w, it) || (p.hand && !accepts(w, e, p, it)) {
        return;
    }
    if p.hand {
        let Some(stack) = w.get::<item::Item>(it).map(|i| i.stack.clone()) else { return };
        let room = room_for(w, e, &stack);
        let Some(taken) = item::pick_up(w, e, it, room) else { return };
        if w.get::<Pocket>(e).is_none() {
            w.entity_mut(e).insert(Pocket::default());
        }
        w.get_mut::<Pocket>(e).unwrap().add(taken, MAX_PICK);
    } else if let Some(stack) = item::pick_up(w, e, it, u32::MAX) {
        set_main_hand(w, e, Some(stack as ItemStack));
    }
}
/// The main hand or the pocket changed: the goal is asked again. Exported, nothing calls it yet.
pub fn pocket_or_hand_changed(w: &mut World, e: Entity) {
    crate::goals::wake(w, e, &MODULE);
}
