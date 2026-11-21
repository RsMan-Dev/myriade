//! `TurtleGoToWaterGoal` (`MoveToBlockGoal`, fiche 47): a turtle out of the water (a baby always) walks to water within 24
//! blocks (heights -1 to 1). After each search, the countdown `reduced(200 + nextInt(200))` is armed: the next search comes
//! `2 * (countdown + 1)` ticks later. The walk asks its path again every 160 ticks of walking.
use super::turtle_state as ts;
use super::*;
use crate::move_to_block::MoveToBlock;
use crate::{cow::*, goals::*};
use horizon_client_lib::mobs::MobWorld;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub speed: f64,
    pub range: i32,
    pub vertical: i32,
    pub vertical_start: i32,
    pub path_every: i32,
    pub give_up: i32,
}
impl Params {
    pub const DEFAULT: Self = Self { speed: 1.0, range: 24, vertical: 1, vertical_start: -1, path_every: 160, give_up: 1200 };
}
/// The search of the goal: its walk, the countdown drawn at the last search, whether a deadline is armed.
#[derive(Component, Clone, Debug)]
pub struct Seek {
    pub def: &'static Params,
    pub walk: MoveToBlock,
    pub countdown: i32,
    pub armed: bool,
    pub slot: u8,
}
/// The goal runs: the walk's system visits it.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct GoingToWater;

pub static MODULE: Module = Module {
    name: "turtle_go_to_water",
    flags: MOVE | JUMP,
    equip,
    arm: arm_first,
    fired,
    can_use,
    start,
    stop,
    systems,
    requires: &[Cap::Body, Cap::Goals, Cap::Locomotion],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let def = params::<Params>(p);
    let mut walk = MoveToBlock::new(def.speed, def.range, def.vertical);
    walk.vertical_start = def.vertical_start;
    w.entity_mut(e).insert(Seek { def, walk, countdown: 0, armed: false, slot: inst.slot });
    ts::ensure(w, e);
}
fn arm_first(w: &mut World, e: Entity, _: Inst) {
    schedule(w, e, 1);
}
/// Arms the next search, once: a second arming would start a second chain of searches.
fn schedule(w: &mut World, e: Entity, wait: i32) {
    let slot = {
        let Some(mut s) = w.get_mut::<Seek>(e) else { return };
        if s.armed {
            return;
        }
        s.armed = true;
        s.slot
    };
    crate::goals::arm(w, e, slot, wait);
}
fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    if let Some(mut s) = w.get_mut::<Seek>(e) {
        s.armed = false;
    }
    w.get::<GoingToWater>(e).is_none()
}
/// `canUse`'s first test: a baby out of the water, or an animal out of it, not going home and without an egg.
pub fn wants_water(w: &World, e: Entity) -> bool {
    let in_water = w.get::<FluidState>(e).is_some_and(|f| f.in_water);
    if w.get::<GrowAt>(e).is_some() && !in_water {
        return true;
    }
    let t = ts::get(w, e);
    !t.going_home && !in_water && !ts::has_egg(w, e)
}
fn is_water(mw: &MobWorld, p: [i32; 3]) -> bool {
    mw.facts.get(mw.world.block(p[0], p[1], p[2])).name.as_ref() == "minecraft:water"
}
fn reduced(n: i32) -> i32 {
    (n + 1) / 2
}
fn feet(w: &World, e: Entity) -> [i32; 3] {
    let p = w.get::<Pos>(e).unwrap().0;
    [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32]
}
/// `canUse`: the countdown is drawn, the search runs; a search that finds nothing arms the next one.
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    if !wants_water(w, e) {
        return false;
    }
    let countdown = with_rng(w, e, |r| reduced(200 + r.next_int(200)));
    let base = feet(w, e);
    let mut s = w.get::<Seek>(e).unwrap().clone();
    let found = {
        let mw = w.resource::<Level>().mw.0;
        s.walk.may_find(base, mw, |s| &*mw.facts.get(s).name == "minecraft:water") && s.walk.find_nearest(base, mw, is_water)
    };
    s.countdown = countdown;
    *w.get_mut::<Seek>(e).unwrap() = s;
    if !found {
        schedule(w, e, 2 * (countdown + 1));
    }
    found
}
fn start(w: &mut World, e: Entity, _: Inst) {
    let mut s = w.get::<Seek>(e).unwrap().clone();
    with_rng(w, e, |r| s.walk.start(r));
    go_to_at(w, e, s.walk.target(), s.def.speed, 1);
    w.entity_mut(e).insert(GoingToWater);
    *w.get_mut::<Seek>(e).unwrap() = s;
}
/// The end of the goal: the countdown drawn at the last search runs again from the next ask.
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<GoingToWater>();
    let n = w.get::<Seek>(e).map_or(0, |s| s.countdown);
    schedule(w, e, 2 * (n + 1));
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems((water_events, water_run).chain().in_set(slot));
}
/// Entering water ends the walk; leaving it asks for the search again (the countdown restarts: see the notes).
fn water_events(ev: Res<Events>, running: Query<&Seek, With<GoingToWater>>, idle: Query<Entity, (With<Seek>, Without<GoingToWater>)>, mut reqs: ResMut<Requests>) {
    for e in ev.entered_fluid.iter() {
        if let Ok(s) = running.get(e) {
            reqs.0.push(Req::End(e, s.slot));
        }
    }
    for e in ev.left_fluid.iter() {
        if idle.contains(e) {
            reqs.0.push(Req::Call(e, rearm));
        }
    }
}
fn rearm(w: &mut World, e: Entity) {
    if wants_water(w, e) && !w.get::<Seek>(e).is_some_and(|s| s.armed) {
        let n = w.get::<Seek>(e).map_or(0, |s| s.countdown);
        schedule(w, e, 2 * (n + 1));
    }
}
/// The walk, each tick of the goal: it ends in the water, on a lost block or after `give_up` ticks; the path is asked again
/// every `path_every` ticks of walking (`MoveToBlock.tick` counts them; its own 40-tick cadence is not used).
fn water_run(mut q: Query<(Entity, &mut Seek, &Pos, &FluidState), With<GoingToWater>>, level: Res<Level>, mut reqs: ResMut<Requests>) {
    let mw = level.mw.0;
    for (e, mut s, p, f) in &mut q {
        if f.in_water || s.walk.try_ticks > s.def.give_up || !is_water(mw, s.walk.block) {
            reqs.0.push(Req::End(e, s.slot));
            continue;
        }
        s.walk.tick([p.0.x, p.0.y, p.0.z], 1.0);
        if !s.walk.reached && s.walk.try_ticks % s.def.path_every == 0 {
            reqs.0.push(Req::Call(e, walk_again));
        }
    }
}
fn walk_again(w: &mut World, e: Entity) {
    let Some(s) = w.get::<Seek>(e).cloned() else { return };
    go_to_at(w, e, s.walk.target(), s.def.speed, 1);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behaviours::Entry;
    use crate::cow::{ActiveUntil, Herd, SpeciesTable, spawn_species};
    use crate::species::Config;

    fn config() -> Config {
        let entries: &'static [Entry] = Box::leak(Box::new([Entry::new(&MODULE, 3, &Params::DEFAULT)]));
        Config { entries }
    }
    fn running(h: &Herd, e: Entity) -> bool {
        h.world.get::<GoalSet>(e).unwrap().is_running(&MODULE)
    }
    /// A cow with this goal alone, on the slab; the pool is set by the test.
    fn world() -> Option<(Herd, Entity)> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = Herd::with_slab_awake(200, false, false);
        crate::goals::set_lazy_look(&mut h.world, false);
        let table = h.world.resource::<SpeciesTable>().0.clone();
        let mut sp = *table.iter().find(|s| s.name == "minecraft:cow").unwrap();
        sp.goals = config();
        let c = spawn_species(&mut h.world, &sp, 0.5, 0.5, 77);
        h.world.get_mut::<ActiveUntil>(c).unwrap().0 = u64::MAX;
        h.world.resource_mut::<crate::goals::Entered>().0.push(c);
        h.tick();
        Some((h, c))
    }
    /// A pool from `x0` to `x1` at the rows the search reads: `vertical_start` -1 walks only the row feet - 2 (`y - 1` with
    /// y = -1: the walk's heights go -1, then 2 > range), so the water is on row -1 (and row 0, its surface).
    fn pool(h: &mut Herd, x0: i32, x1: i32) {
        set_rows(h, x0, x1, "minecraft:water");
    }
    fn set_rows(h: &mut Herd, x0: i32, x1: i32, name: &'static str) {
        for x in x0..=x1 {
            for z in -1..=1 {
                for y in [-1, 0] {
                    h.set_block(x, y, z, name);
                }
            }
        }
    }
    fn ticks(h: &mut Herd, n: usize) {
        for _ in 0..n {
            h.tick();
        }
    }

    /// A cow on land with a pool 10 blocks away walks to it and stops at its edge (the walk's target is the pool's block).
    #[test]
    fn a_cow_on_land_walks_to_a_pool_within_reach() {
        let Some((mut h, c)) = world() else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        pool(&mut h, 10, 11);
        let mut started = false;
        for _ in 0..600 {
            h.tick();
            started |= running(&h, c);
            if started {
                break;
            }
        }
        assert!(started, "témoin : un départ vers la mare à 10 blocs");
        assert_eq!(h.world.get::<Seek>(c).unwrap().walk.block[0], 10, "la mare trouvée est celle de x = 10");
        ticks(&mut h, 400);
        let p = h.world.get::<Pos>(c).unwrap().0;
        assert!(p.x > 9.0, "le chemin mène à la mare (x = {})", p.x);
    }

    /// A pool beyond 24 blocks is not searched: no departure; a pool placed within reach later is the witness.
    #[test]
    fn a_pool_out_of_reach_starts_nothing_and_one_in_reach_does() {
        let Some((mut h, c)) = world() else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        pool(&mut h, 30, 31);
        ticks(&mut h, 600);
        assert!(!running(&h, c), "pas de départ vers une mare hors de portée");
        pool(&mut h, 8, 9);
        let mut started = false;
        for _ in 0..500 {
            h.tick();
            started |= running(&h, c);
            if started {
                break;
            }
        }
        assert!(started, "témoin : une mare à portée fait partir la vache");
    }

    /// The goal ends when its pool disappears (`can_continue` fails: the block is no longer water).
    #[test]
    fn the_walk_ends_when_its_pool_is_gone() {
        let Some((mut h, c)) = world() else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        pool(&mut h, 10, 11);
        let mut started = false;
        for _ in 0..600 {
            h.tick();
            started |= running(&h, c);
            if started {
                break;
            }
        }
        assert!(started, "témoin : le départ");
        set_rows(&mut h, 10, 11, "minecraft:stone");
        ticks(&mut h, 3);
        assert!(!running(&h, c), "la marche s'arrête quand la mare disparaît");
    }

}
