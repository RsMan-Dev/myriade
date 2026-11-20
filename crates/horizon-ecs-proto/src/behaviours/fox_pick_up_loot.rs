//! The looting block of `Mob.aiStep` for a fox (`game/mobs/foxes.rs:33-62`, `fox_pick_up` :130-190; `Fox.canHoldItem`
//! `mobs/fox.rs:1653-1660`), fiche 40: a RULE without a flag and without a goal. When the fox changes column (`CellMoves`), or
//! when the search goal looks while it walks, the items of its box grown by `reach`, in the order they joined, are taken
//! when the mouth can hold them. Not ported: the thrower of the spat item, the hand's drop chance (2,0), the pickup trigger.
use super::*;
use crate::cow::*;
use crate::goals::*;
use crate::item::{self, Item};
use horizon_client_lib::items::ItemStack;
use horizon_core::geometry::{Vec3, trig};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The box of the fox grown by this much (x, y, z) is where the items are looked for.
    pub reach: [f64; 3],
    /// The items a fox eats (`Fox.isConsumableFood`), by name. The food data of the game is not in the prototype: the species
    /// gives the list. DOUTE: empty by default (no food swap until the species gives it).
    pub foods: &'static [&'static str],
    /// The delay of the held item that is spat out (`spitOutItem`).
    pub spit_delay: i64,
    /// The delay of the rest of a stack that is picked, dropped at the feet.
    pub rest_delay: i64,
    /// `canPickUpLoot` of the species (a fox, in the game: true).
    pub picks_up: bool,
}
impl Params {
    pub const DEFAULT: Self = Self { reach: [1.0, 0.0, 1.0], foods: &[], spit_delay: 40, rest_delay: 10, picks_up: true };
}
/// The rule's data of the fox.
#[derive(Component, Clone, Copy, Debug)]
pub struct Loot {
    pub reach: [f64; 3],
    pub foods: &'static [&'static str],
    pub spit_delay: i64,
    pub rest_delay: i64,
    pub picks_up: bool,
}
/// `Fox.ticksSinceEaten` as the own time it was set (`took_item`, eating): its value is `now - at`, so it is `> 0` from the next tick.
#[derive(Component, Clone, Copy, Debug)]
pub struct Eaten(pub u64);

pub static MODULE: Module = Module {
    name: "fox_pick_up_loot",
    flags: 0,
    equip,
    fired: never,
    can_use: never,
    tick: Some(tick),
    requires: &[Cap::Body, Cap::Goals],
    ..Module::DEFAULT
};

fn equip(w: &mut World, e: Entity, _: Inst, p: &(dyn std::any::Any + Sync)) {
    let p = *params::<Params>(p);
    let now = now_of(w, e);
    w.entity_mut(e).insert((
        Loot { reach: p.reach, foods: p.foods, spit_delay: p.spit_delay, rest_delay: p.rest_delay, picks_up: p.picks_up },
        Eaten(now),
    ));
}
/// A rule, not a goal: nothing starts it.
fn never(_: &mut World, _: Entity, _: Inst) -> bool {
    false
}
/// The module's tick, called only while a creature that carries it lives (no registered system).
fn tick(w: &mut World) {
    crate::goals::tick_state(w, moved);
}
/// The foxes that changed column this tick look at their items (the others cost nothing).
fn moved(moves: Res<CellMoves>, q: Query<(), With<Loot>>, mut reqs: ResMut<Requests>) {
    if moves.0.is_empty() || q.is_empty() {
        return;
    }
    for &e in &moves.0 {
        if q.contains(e) {
            reqs.0.push(Req::Call(e, look));
        }
    }
}

/// The looting of this fox now: every item of its grown box that is pickable and that its mouth can hold is taken, in order.
/// The list is taken first (`getEntitiesOfClass`); an item that is gone by its turn is skipped.
pub fn look(w: &mut World, e: Entity) {
    let Some(l) = w.get::<Loot>(e).copied() else { return };
    if !l.picks_up || !mob_griefing(w) {
        return;
    }
    let Some(b) = w.get::<Bounds>(e).map(|b| b.0) else { return };
    let reach = b.inflate(Vec3::new(l.reach[0], l.reach[1], l.reach[2]));
    for it in item::items_in(w, reach) {
        if !item::pickable(w, it) {
            continue;
        }
        let Some(stack) = w.get::<Item>(it).map(|i| i.stack.clone()) else { continue };
        let name: &str = &stack.item;
        let held = main_hand(w, e).map(|h| h.item.clone());
        let held_food = held.as_deref().is_some_and(|h| l.foods.contains(&h));
        if !can_hold(w, e, l.foods.contains(&name), held_food) {
            continue;
        }
        take(w, e, it, l);
    }
}

/// `Fox.canHoldItem`: an empty mouth, or food to swap for what is not food once it has eaten at all.
fn can_hold(w: &World, e: Entity, item_food: bool, held_food: bool) -> bool {
    main_hand(w, e).is_none() || (ticks_since_eaten(w, e) > 0 && item_food && !held_food)
}
fn ticks_since_eaten(w: &World, e: Entity) -> u64 {
    w.get::<Eaten>(e).map_or(0, |t| now_of(w, e).saturating_sub(t.0))
}

/// `Fox.pickUpItem`: the whole stack is taken; the rest of it falls at the feet, the held item is spat out ahead of the fox,
/// and the mouth holds one of the item; `took_item` restarts the clock.
fn take(w: &mut World, e: Entity, it: Entity, l: Loot) {
    let Some(stack) = w.get::<Item>(it).map(|i| i.stack.clone()) else { return };
    let count = stack.count;
    let Some(taken) = item::pick_up(w, e, it, count) else { return };
    let at = w.get::<Pos>(e).unwrap().0;
    let mut hand = taken.clone();
    hand.count = 1;
    if count > 1 {
        let mut rest = taken;
        rest.count = count - 1;
        toss(w, e, rest, at, l.rest_delay);
    }
    if let Some(held) = main_hand(w, e).cloned() {
        let from = mouth(w, e, at);
        toss(w, e, held, from, l.spit_delay);
    }
    set_main_hand(w, e, Some(hand));
    let now = now_of(w, e);
    if let Some(mut t) = w.get_mut::<Eaten>(e) {
        t.0 = now;
    }
}

/// The spot ahead of the fox's mouth: one block out along its heading, one block up (`Mob.aiStep` spits from there).
fn mouth(w: &World, e: Entity, at: Vec3) -> Vec3 {
    let (yaw, pitch) = (w.get::<Yaw>(e).map_or(0.0, |y| y.0), w.get::<Head>(e).map_or(0.0, |h| h.pitch));
    let (yr, pr) = (-yaw * trig::DEG_TO_RAD, pitch * trig::DEG_TO_RAD);
    let (xd, zd) = (f64::from(trig::sin(f64::from(yr)) * trig::cos(f64::from(pr))), f64::from(trig::cos(f64::from(yr)) * trig::cos(f64::from(pr))));
    Vec3::new(at.x + xd, at.y + 1.0, at.z + zd)
}

/// A stack thrown as `ItemEntity`'s constructor throws it (`spawn_item_own`: the motion from the creature's own random, as
/// `spawn_at_location` draws it), with the given delay.
fn toss(w: &mut World, e: Entity, stack: ItemStack, at: Vec3, delay: i64) -> Entity {
    let (a, b) = with_rng(w, e, |r| (r.next_f64(), r.next_f64()));
    item::drop_item(w, stack, at, Vec3::new(a * 0.2 - 0.1, 0.2, b * 0.2 - 0.1), delay)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::species::Config;

    static FOOD: [&str; 1] = ["minecraft:sweet_berries"];
    static ENTRIES: [Entry; 1] = [Entry::new(&MODULE, 255, &Params { foods: &FOOD, rest_delay: 0, spit_delay: 0, ..Params::DEFAULT })];

    fn herd() -> Option<Herd> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = Herd::with_slab(30, false, false);
        set_lazy_look(&mut h.world, false);
        Some(h)
    }
    fn fox(h: &mut Herd) -> Entity {
        let mut sp = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == "minecraft:cow").unwrap();
        sp.name = "minecraft:fox";
        sp.goals = Config { entries: &ENTRIES };
        let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 13);
        h.world.flush();
        h.world.resource_mut::<Requests>().0.push(Req::Arm(e));
        h.world.flush();
        e
    }
    fn drop_at(h: &mut Herd, name: &str, count: u32, x: f64) {
        item::drop_item(&mut h.world, ItemStack::new(name, count), Vec3::new(x, 1.0, 0.5), Vec3::ZERO, 0);
        h.world.flush();
    }
    fn items_total(h: &mut Herd) -> u32 {
        h.world.query::<&Item>().iter(&h.world).map(|i| i.stack.count).sum()
    }
    fn hand(h: &Herd, e: Entity) -> Option<String> {
        main_hand(&h.world, e).map(|s| s.item.to_string())
    }

    /// A fox with an empty mouth takes one berry of a stack of three, and the other two fall at its feet.
    /// Witness: the same fox, before the look, has three berries on the ground and nothing in its mouth.
    #[test]
    fn a_fox_takes_one_of_a_stack_and_the_rest_falls() {
        let Some(mut h) = herd() else { return };
        let e = fox(&mut h);
        drop_at(&mut h, "minecraft:sweet_berries", 3, 0.5);
        assert_eq!(items_total(&mut h), 3);
        assert!(hand(&h, e).is_none(), "the mouth is empty before the look");
        look(&mut h.world, e);
        h.world.flush();
        assert_eq!(hand(&h, e).as_deref(), Some("minecraft:sweet_berries"));
        assert_eq!(main_hand(&h.world, e).unwrap().count, 1, "the mouth holds one");
        assert_eq!(items_total(&mut h), 2, "the two others are on the ground");
    }

    /// A full mouth keeps what it holds unless the item is food and the fox has eaten: a stick is spat out for a berry.
    /// Witness: the berry is left alone at first, and taken once the clock since eating has run.
    #[test]
    fn a_full_mouth_swaps_a_stick_for_food_only_after_eating() {
        let Some(mut h) = herd() else { return };
        let e = fox(&mut h);
        set_main_hand(&mut h.world, e, Some(ItemStack::new("minecraft:stick", 1)));
        drop_at(&mut h, "minecraft:sweet_berries", 1, 0.5);
        look(&mut h.world, e);
        h.world.flush();
        assert_eq!(hand(&h, e).as_deref(), Some("minecraft:stick"), "no swap at 0 ticks since eating");
        for _ in 0..3 {
            h.tick();
        }
        look(&mut h.world, e);
        h.world.flush();
        assert_eq!(hand(&h, e).as_deref(), Some("minecraft:sweet_berries"), "food replaces a stick once the fox has eaten");
        assert_eq!(items_total(&mut h), 1, "the stick is spat out on the ground");
    }

    /// The rule runs when the fox changes column: a berry on the ground is taken at that tick, and not before.
    #[test]
    fn a_column_change_runs_the_rule() {
        let Some(mut h) = herd() else { return };
        let e = fox(&mut h);
        // Creating the fox counts as a column change: forget it, the test wants the first tick without a move.
        h.world.resource_mut::<CellMoves>().0.clear();
        drop_at(&mut h, "minecraft:sweet_berries", 1, 0.5);
        h.tick();
        assert!(hand(&h, e).is_none(), "nothing moved: no look");
        h.world.resource_mut::<CellMoves>().0.push(e);
        h.tick();
        assert_eq!(hand(&h, e).as_deref(), Some("minecraft:sweet_berries"));
    }
}
