//! `ZombieAttackTurtleEggGoal` (fiche 26, `mobs/undead.rs:1193-1360`), a `RemoveBlockGoal` on turtle eggs: a zombie (husk,
//! zombie villager, drowned, zombified piglin) with mob griefing walks to an egg within 24 blocks and tramples it. The search
//! is a deadline of the wheel (the Mob's `next_start` countdown, one ask every 2 ticks); the walk and the stamping are the
//! goal's tick while it runs.
use super::*;
use crate::{cow::*, goals::*, move_to_block::MoveToBlock};
use horizon_client_lib::mobs::MobWorld;
use horizon_core::geometry::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub speed: f64,
    pub range: i32,
    pub vertical: i32,
    /// `acceptedDistance` of the walk.
    pub accepted: f64,
}
impl Params {
    pub const DEFAULT: Self = Self { speed: 1.0, range: 24, vertical: 3, accepted: 1.14 };
}
/// The hunt of this animal: its walk (the egg found), and the ticks it has stood by the egg (`since_reached`).
#[derive(Component, Clone, Debug)]
pub struct EggHunt {
    pub walk: MoveToBlock,
    pub since_reached: i32,
    pub accepted: f64,
}
/// The goal runs: its tick is visited.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Trampling;

pub static MODULE: Module = Module {
    name: "attack_turtle_egg",
    flags: MOVE | JUMP,
    equip,
    arm,
    fired,
    can_use,
    start,
    stop,
    systems,
    requires: &[Cap::Body, Cap::Goals, Cap::Locomotion],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, _: Inst, p: &(dyn std::any::Any + Sync)) {
    let p = *params::<Params>(p);
    w.entity_mut(e).insert(EggHunt { walk: MoveToBlock::new(p.speed, p.range, p.vertical), since_reached: 0, accepted: p.accepted });
}
/// The hunter waits for an egg to appear: no deadline, nothing asked while the world holds none (`parked_wake` re-arms it).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Parked;
fn no_egg_anywhere(w: &World) -> bool {
    w.get_resource::<Hazards>().is_some_and(|h| h.eggs_empty())
}
/// `next_start` is 0 at creation: the first ask searches, unless the world holds no egg: then it waits for one.
fn arm(w: &mut World, e: Entity, inst: Inst) {
    if no_egg_anywhere(w) {
        w.entity_mut(e).insert(Parked);
    } else {
        crate::goals::arm(w, e, inst.slot, 1);
    }
}
/// A deadline that is not the goal's own (the replay of a start) asks `canUse`; a running goal has none.
fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    w.get::<Trampling>(e).is_none()
}
/// `RemoveBlockGoal.canUse`: no search without mob griefing (the Mob then counts nothing down, and no wait is armed: DOUTE).
/// A search that finds an egg is a start; otherwise the next search is armed after `reduced(200 + rnd(200))` asks.
fn can_use(w: &mut World, e: Entity, inst: Inst) -> bool {
    if !mob_griefing(w) {
        return false;
    }
    let mw = w.resource::<Level>().mw.0;
    let base = feet(w, e);
    // No egg in the zone of the search (the world keeps the cells of its eggs), then none in any section of it (the palette
    // sweep of the `Mob`, `may_find`): the cells are walked only when an egg can be there; same answer as the walk.
    let hunt = w.get::<EggHunt>(e).unwrap();
    let nothing = hunt.walk.zone(base).is_none_or(|(lo, hi)| w.get_resource::<Hazards>().is_some_and(|h| !h.egg_in(lo, hi)));
    let mut hunt = w.get_mut::<EggHunt>(e).unwrap();
    let found = !nothing && hunt.walk.may_find(base, mw, |s| &*mw.facts.get(s).name == "minecraft:turtle_egg") && hunt.walk.find_nearest(base, mw, egg_target);
    if found {
        return true;
    }
    if no_egg_anywhere(w) {
        w.entity_mut(e).insert(Parked);
        return false;
    }
    let k = reduced(200 + with_rng(w, e, |r| r.next_int(200)));
    crate::goals::arm(w, e, inst.slot, 2 * (k + 1));
    false
}
/// `start`: the walk's draws, the path to the block above it; the goal's tick follows in the same tick (the Mob ticks a
/// started goal at once).
fn start(w: &mut World, e: Entity, _: Inst) {
    let mut h = w.get::<EggHunt>(e).unwrap().clone();
    let mut r = *w.get::<Rng>(e).unwrap();
    h.walk.start(&mut r);
    h.since_reached = 0;
    *w.get_mut::<Rng>(e).unwrap() = r;
    let (at, speed) = (h.walk.target(), h.walk.speed);
    *w.get_mut::<EggHunt>(e).unwrap() = h;
    crate::goals::go_to_at(w, e, at, speed, 1);
    w.entity_mut(e).insert(Trampling);
    trample_tick(w, e);
}
/// `stop`: the fall distance is 1 (no fall damage after the stamp), and the next search is armed after the `reduced(20)` of a
/// found egg (the countdown the Mob keeps through the goal).
fn stop(w: &mut World, e: Entity, inst: Inst) {
    w.entity_mut(e).remove::<Trampling>();
    if let Some(mut c) = w.get_mut::<Contact>(e) {
        c.fall_distance = 1.0;
    }
    crate::goals::arm(w, e, inst.slot, 2 * (reduced(20) + 1));
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems(trample_run.in_set(slot));
    // An egg appeared (the hazards changed): the parked hunters ask again after the delay of a search that found nothing.
    s.add_systems(parked_wake.in_set(slot).run_if(bevy_ecs::schedule::common_conditions::resource_changed::<Hazards>));
}
fn parked_wake(w: &mut World, q: &mut QueryState<Entity, With<Parked>>) {
    if no_egg_anywhere(w) {
        return;
    }
    let parked: Vec<Entity> = q.iter(w).collect();
    for e in parked {
        w.entity_mut(e).remove::<Parked>();
        let Some(slot) = slot_of(w, e, &MODULE) else { continue };
        let k = reduced(200 + with_rng(w, e, |r| r.next_int(200)));
        crate::goals::arm(w, e, slot, 2 * (k + 1));
    }
}
/// The tick of every running goal of this kind (a query of the marker only: nothing when no zombie runs).
fn trample_run(w: &mut World, q: &mut QueryState<Entity, With<Trampling>>) {
    let runs: Vec<Entity> = q.iter(w).collect();
    for e in runs {
        trample_tick(w, e);
    }
}
/// `canContinueToUse`, then `tick`: the path asked again every 40 ticks of walking, and while the mob stands by an egg
/// (under it, beside it, or two below) after it reached the block: a hop, a fall, and the egg breaks after 60 ticks.
fn trample_tick(w: &mut World, e: Entity) {
    let mw = w.resource::<Level>().mw.0;
    let mut h = w.get::<EggHunt>(e).unwrap().clone();
    if !h.walk.can_continue(mw, egg_target) {
        end(w, e, &MODULE);
        return;
    }
    let pos = w.get::<Pos>(e).unwrap().0;
    if h.walk.tick([pos.x, pos.y, pos.z], h.accepted) {
        let (at, speed) = (h.walk.target(), h.walk.speed);
        crate::goals::go_to_at(w, e, at, speed, 1);
    }
    let at = feet(w, e);
    let eat = [
        at,
        [at[0], at[1] - 1, at[2]],
        [at[0] - 1, at[1], at[2]],
        [at[0] + 1, at[1], at[2]],
        [at[0], at[1], at[2] - 1],
        [at[0], at[1], at[2] + 1],
        [at[0], at[1] - 2, at[2]],
    ]
    .into_iter()
    .find(|p| mw.block_name(p[0], p[1], p[2]) == "minecraft:turtle_egg");
    if let (true, Some(eat)) = (h.walk.reached, eat) {
        let v = w.get::<Vel>(e).unwrap().0;
        let mut vy = None;
        if h.since_reached > 0 {
            vy = Some(0.3);
        }
        if h.since_reached % 2 == 0 {
            vy = Some(-0.3);
        }
        if let Some(vy) = vy {
            set_velocity(w, e, Vec3::new(v.x, vy, v.z));
        }
        if h.since_reached > 60 {
            w.resource_mut::<BlockChanges>().0.push((eat[0], eat[1], eat[2], "minecraft:air"));
        }
        h.since_reached += 1;
    }
    *w.get_mut::<EggHunt>(e).unwrap() = h;
}
/// `RemoveBlockGoal.isValidTarget` without `canEntityDestroy` (no such service: taken as true, DOUTE): an egg in a loaded
/// chunk with two air blocks above it.
/// The state ids of the turtle egg block, read once from the facts of the process (the states of a block are contiguous): the
/// search compares numbers, not names (a 24-block search reads some 16 000 cells).
fn egg_states(mw: &MobWorld) -> (u32, u32) {
    static IDS: std::sync::OnceLock<(u32, u32)> = std::sync::OnceLock::new();
    *IDS.get_or_init(|| {
        let mut it = mw.facts.states.iter().enumerate().filter(|(_, s)| &*s.name == "minecraft:turtle_egg").map(|(i, _)| i as u32);
        let first = it.next();
        first.map_or((1, 0), |f| (f, it.last().unwrap_or(f)))
    })
}
fn egg_target(mw: &MobWorld, p: [i32; 3]) -> bool {
    if !mw.world.loaded(p[0], p[2]) {
        return false;
    }
    let (lo, hi) = egg_states(mw);
    let id = mw.world.block(p[0], p[1], p[2]).0;
    id >= lo && id <= hi && mw.facts.is_air(mw.world.block(p[0], p[1] + 1, p[2])) && mw.facts.is_air(mw.world.block(p[0], p[1] + 2, p[2]))
}
fn feet(w: &World, e: Entity) -> [i32; 3] {
    let p = w.get::<Pos>(e).unwrap().0;
    [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32]
}
/// `reducedTickDelay`.
fn reduced(n: i32) -> i32 {
    (n + 1) / 2
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behaviours::Entry;
    use crate::cow::{ActiveUntil, Herd, SpeciesTable, spawn_species};
    use crate::species::Config;

    fn config() -> Config {
        let entries: &'static [Entry] = Box::leak(Box::new([Entry::new(&MODULE, 4, &Params::DEFAULT)]));
        Config { entries }
    }
    fn running(h: &Herd, e: Entity) -> bool {
        h.world.get::<GoalSet>(e).unwrap().is_running(&MODULE)
    }
    /// A zombie with this goal alone, on the slab, and its egg 8 blocks away on the row of its feet.
    fn world() -> Option<(Herd, Entity)> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = Herd::with_slab(60, true, false);
        crate::goals::set_lazy_look(&mut h.world, false);
        let table = h.world.resource::<SpeciesTable>().0.clone();
        let mut sp = *table.iter().find(|s| s.name == "minecraft:zombie").unwrap();
        sp.goals = config();
        let z = spawn_species(&mut h.world, &sp, 0.5, 0.5, 41);
        h.world.get_mut::<ActiveUntil>(z).unwrap().0 = u64::MAX;
        crate::goals::arm_timers(&mut h.world, z);
        h.tick();
        Some((h, z))
    }
    fn egg_at(h: &mut Herd, z: Entity, x: i32) {
        let row = h.world.get::<Pos>(z).unwrap().0.y.floor() as i32;
        h.set_block(x, row, 0, "minecraft:turtle_egg");
        h.tick();
    }
    fn egg_exists(h: &Herd, x: i32, row: i32) -> bool {
        h.world.resource::<Level>().mw.0.block_name(x, row, 0) == "minecraft:turtle_egg"
    }

    /// Une ponte à 8 blocs : départ, marche vers elle, écrasement jusqu'au bris, arrêt ensuite ; sans ponte, jamais.
    #[test]
    fn it_walks_to_an_egg_tramples_it_and_stops_when_it_is_gone() {
        let Some((mut h, z)) = world() else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        ticks(&mut h, 40);
        assert!(!running(&h, z), "pas de départ sans ponte");
        let row = h.world.get::<Pos>(z).unwrap().0.y.floor() as i32;
        egg_at(&mut h, z, 8);
        assert!(egg_exists(&h, 8, row), "la ponte est posée");
        let mut started = false;
        for _ in 0..1500 {
            h.tick();
            started |= running(&h, z);
            if !egg_exists(&h, 8, row) {
                break;
            }
        }
        assert!(started, "témoin : le but a démarré vers la ponte");
        assert!(!egg_exists(&h, 8, row), "la ponte est brisée");
        ticks(&mut h, 3);
        assert!(!running(&h, z), "arrêt quand la ponte a disparu");
        let x = h.world.get::<Pos>(z).unwrap().0.x;
        assert!(x > 4.0, "témoin : le zombie a marché vers la ponte (x = {x})");
    }

    fn ticks(h: &mut Herd, n: usize) {
        for _ in 0..n {
            h.tick();
        }
    }
}
