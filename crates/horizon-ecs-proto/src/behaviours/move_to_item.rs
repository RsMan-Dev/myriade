//! `Dolphin.MoveToItemGoal` (`mobs/dolphin.rs:442-477`, fiche 110, MOVE): swim to the first item that floats in the water within
//! the box of the animal grown by 8 (no pickup delay, in water). While it runs the point is asked again each tick (the nearest
//! in the item order, as the `Mob`'s scan); it ends when no item is left in reach. At its end the mouth is emptied by throwing
//! (`play_with_items::throw`) and the goal rests for `nextInt(100)` ticks.
//! Written by event: the event `item_dropped` wakes the dolphins near the new item, and the deadline of an item still under its
//! pickup delay is its own `ready_at`. A static item that an animal swims up to is not an event (recorded gap: the `Mob` scans it
//! each tick). The sound `entity.dolphin.play` at the start is not ported (no sound line in the prototype).
use super::*;
use crate::cow::*;
use crate::goals::*;
use crate::item;
use crate::rules::move_to_item::reaches_item;
use horizon_core::geometry::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub speed: f64,
    pub scan: f64,
    /// The rest after a throw: `nextInt(rest)` ticks.
    pub rest: i32,
}
impl Params {
    pub const DEFAULT: Self = Self { speed: 1.2_f32 as f64, scan: 8.0, rest: 100 };
}

/// The state: the parameters, the own time the rest ends at, whether a deadline is armed.
#[derive(Component, Clone, Copy, Debug)]
pub struct Fetcher {
    pub p: Params,
    pub rest_until: u64,
    pub armed: bool,
    pub slot: u8,
}
/// The goal runs: its tick visits it.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Fetching;

pub static MODULE: Module = Module {
    name: "move_to_item",
    tick: Some(tick),
    flags: MOVE,
    equip,
    arm: arm_first,
    fired,
    can_use,
    start,
    stop,
    requires: &[Cap::Body, Cap::Goals, Cap::Locomotion],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = *params::<Params>(p);
    w.entity_mut(e).insert(Fetcher { p, rest_until: 0, armed: false, slot: inst.slot });
}
fn chain(w: &mut World, e: Entity, wait: i32) {
    let slot = {
        let Some(mut f) = w.get_mut::<Fetcher>(e) else { return };
        if f.armed {
            return;
        }
        f.armed = true;
        f.slot
    };
    crate::goals::arm(w, e, slot, wait.max(1));
}
/// One ask at the start for the items already floating around the animal.
fn arm_first(w: &mut World, e: Entity, _: Inst) {
    chain(w, e, 1);
}
fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    if let Some(mut f) = w.get_mut::<Fetcher>(e) {
        f.armed = false;
    }
    w.get::<Fetching>(e).is_none()
}
/// The first item (the order of `items_in`) in the grown box that floats and is pickable.
fn point(w: &World, e: Entity, scan: f64) -> Option<Entity> {
    let b = w.get::<Bounds>(e)?.0;
    let area = b.inflate(Vec3::new(scan, scan, scan));
    item::items_in(w, area).into_iter().find(|&it| item::pickable(w, it) && w.get::<FluidState>(it).is_some_and(|f| f.in_water))
}
/// The ticks until the soonest item of the grown box is pickable (items under their delay): the deadline for the ask. `ready_at` is in
/// the item's own time, not the animal's.
fn soonest_ready(w: &World, e: Entity, scan: f64) -> Option<u64> {
    let b = w.get::<Bounds>(e)?.0;
    item::items_in(w, b.inflate(Vec3::new(scan, scan, scan))).into_iter().filter_map(|it| w.get::<item::Item>(it)?.ready_at.and_then(|r| r.checked_sub(now_of(w, it)))).filter(|&d| d > 0).min()
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let f = *w.get::<Fetcher>(e).unwrap();
    let now = now_of(w, e);
    if now < f.rest_until {
        chain(w, e, (f.rest_until - now) as i32);
        return false;
    }
    if point(w, e, f.p.scan).is_some() {
        return true;
    }
    if let Some(wait) = soonest_ready(w, e, f.p.scan) {
        chain(w, e, wait as i32);
    }
    false
}
fn start(w: &mut World, e: Entity, _: Inst) {
    let f = *w.get::<Fetcher>(e).unwrap();
    w.entity_mut(e).insert(Fetching);
    if let Some(it) = point(w, e, f.p.scan) {
        go_to_entity(w, e, it, f.p.speed, 0);
    }
}
/// `stop`: the mouth is emptied; a throw starts the rest (`nextInt(100)`), the next ask comes when it is over.
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Fetching>();
    let rest = w.get::<Fetcher>(e).map_or(100, |f| f.p.rest);
    if super::play_with_items::throw(w, e) {
        let n = with_rng(w, e, |r| r.next_int(rest)) as u64;
        let now = now_of(w, e);
        if let Some(mut f) = w.get_mut::<Fetcher>(e) {
            f.rest_until = now + n;
        }
        chain(w, e, n as i32);
    } else {
        // Nothing thrown (the mouth stayed empty): items may still be around.
        chain(w, e, 1);
    }
}
/// The tick: the dolphins near a new item are asked, and each running goal aims at the point of this tick or ends.
fn tick(w: &mut World) {
    let dropped: Vec<Entity> = w.resource::<Events>().item_dropped.iter().collect();
    if !dropped.is_empty() {
        let spots: Vec<Vec3> = dropped.iter().filter_map(|&i| w.get::<Pos>(i).map(|p| p.0)).collect();
        let near: Vec<Entity> = w
            .query::<(Entity, &Pos, &Fetcher)>()
            .iter(w)
            .filter(|(_, p, f)| spots.iter().any(|&s| reaches_item(s, p.0, f.p.scan)))
            .map(|x| x.0)
            .collect();
        for e in near {
            chain(w, e, 1);
        }
    }
    let running: Vec<Entity> = crate::queries::entities::<With<Fetching>>(w);
    for e in running {
        let scan = w.get::<Fetcher>(e).map_or(8.0, |f| f.p.scan);
        match point(w, e, scan) {
            None => end(w, e, &MODULE),
            Some(it) => {
                let speed = w.get::<Fetcher>(e).map_or(1.2, |f| f.p.speed);
                go_to_entity(w, e, it, speed, 0);
                // Another item is in reach: the goal goes on, and `PlayWithItemsGoal` (not woken by the pickup meanwhile) throws what is held.
                if crate::goals::main_hand(w, e).is_some() {
                    crate::goals::wake(w, e, &super::play_with_items::MODULE);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behaviours::{play_with_items, Entry};
    use crate::cow::{ActiveUntil, Herd, SpeciesTable, spawn_species};
    use crate::species::Config;
    use myriade_engine_lib::items::ItemStack;

    fn config() -> Config {
        let entries: &'static [Entry] = Box::leak(Box::new([
            Entry::new(&MODULE, 7, &Params::DEFAULT),
            Entry::new(&crate::behaviours::item_pickup::MODULE, 7, &crate::behaviours::item_pickup::Params::DEFAULT),
            Entry::new(&play_with_items::MODULE, 8, &play_with_items::Params::DEFAULT),
        ]));
        Config { entries }
    }
    fn skip(name: &str) -> bool {
        if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
            eprintln!("skipped {name}: HORIZON_TEST_RESOURCES unset");
            return true;
        }
        false
    }
    /// A dolphin in a pool of water (x, z in -8..=8, rows `sea-3..=sea+1`).
    fn world() -> (Herd, Entity, i32) {
        let mut h = Herd::with_slab_awake(200, false, false);
        crate::goals::set_lazy_look(&mut h.world, false);
        let table = h.world.resource::<SpeciesTable>().0.clone();
        let mut sp = *table.iter().find(|s| s.name == "minecraft:dolphin").unwrap();
        sp.goals = config();
        let sea = h.world.resource::<Level>().mw.0.sea_level;
        let c = spawn_species(&mut h.world, &sp, 0.5, 0.5, 77);
        h.world.get_mut::<ActiveUntil>(c).unwrap().0 = u64::MAX;
        h.world.resource_mut::<crate::goals::Entered>().0.push(c);
        for x in -8..=8 {
            for z in -8..=8 {
                h.set_block(x, sea - 4, z, "minecraft:stone");
                for y in sea - 3..=sea + 1 {
                    h.set_block(x, y, z, "minecraft:water");
                }
            }
        }
        for _ in 0..2 {
            h.tick();
        }
        h.teleport(c, Vec3::new(0.5, f64::from(sea - 2), 0.5));
        (h, c, sea)
    }
    fn drop_near(h: &mut Herd, sea: i32) -> Entity {
        let stack = ItemStack::new("minecraft:cod", 1);
        item::drop_item(&mut h.world, stack, Vec3::new(5.5, f64::from(sea - 2), 0.5), Vec3::new(0.0, 0.0, 0.0), 10)
    }

    /// A floating item that appears within 8 blocks: the goal starts (after its pickup delay), with no polling in between.
    #[test]
    fn a_dropped_item_in_reach_starts_the_goal() {
        if skip("a_dropped_item_in_reach_starts_the_goal") {
            return;
        }
        let (mut h, c, sea) = world();
        for _ in 0..20 {
            h.tick();
        }
        assert!(!h.world.get::<GoalSet>(c).unwrap().is_running(&MODULE), "rien a chercher : le but ne tourne pas");
        let it = drop_near(&mut h, sea);
        let mut started = false;
        for _ in 0..40 {
            h.tick();
            started |= h.world.get::<GoalSet>(c).unwrap().is_running(&MODULE);
            if started {
                break;
            }
        }
        assert!(started, "temoin : le depart vers l'objet {it:?}");
    }

    /// The dolphin takes the item, the goal ends (nothing left in reach) and the mouth is emptied: the item is thrown back.
    #[test]
    fn it_takes_the_item_then_throws_it_and_rests() {
        if skip("it_takes_the_item_then_throws_it_and_rests") {
            return;
        }
        let (mut h, c, sea) = world();
        drop_near(&mut h, sea);
        let mut thrown = false;
        for _ in 0..400 {
            h.tick();
            let r = h.world.get::<Fetcher>(c).unwrap().rest_until;
            if r > 0 {
                thrown = true;
                break;
            }
        }
        assert!(thrown, "temoin : l'objet est ramasse puis jete, le repos commence");
        assert!(!h.world.get::<GoalSet>(c).unwrap().is_running(&MODULE), "le but est fini");
    }
}
