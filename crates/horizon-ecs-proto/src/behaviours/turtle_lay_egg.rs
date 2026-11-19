//! `TurtleLayEggGoal` (`MoveToBlockGoal`, fiche 50): a turtle with an egg within 9 blocks of its home walks to sand with air above
//! (16 blocks, heights 0 to 1), digs for `lay_ticks` (200) ticks of its walk on the spot and lays the egg above the sand:
//! `minecraft:turtle_egg` with `eggs = nextInt(4) + 1`, `has_egg` false, `in_love = 600`. A search is drawn at each `canUse`
//! request that holds (every 2 ticks): `next_start` counts those requests down; the next search comes `2 * (countdown + 1)`
//! ticks later. While the condition holds, the requests go on as one chain of +2 deadlines (one chain per turtle).
use super::turtle_state::{self as ts, Turtle};
use super::*;
use crate::move_to_block::MoveToBlock;
use crate::{cow::*, goals::*};
use horizon_client_lib::mobs::MobWorld;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub speed: f64,
    pub range: i32,
    pub vertical: i32,
    /// `closerToCenterThan(home, 9)`.
    pub close_home: f64,
    /// The laying takes `lay_ticks` ticks of the walk on the spot (`counter > 200`).
    pub lay_ticks: i32,
}
impl Params {
    pub const DEFAULT: Self = Self { speed: 1.0, range: 16, vertical: 1, close_home: 9.0, lay_ticks: 200 };
}
/// The search of the goal: its walk, the countdown drawn at the last search, whether a `canUse` deadline is armed.
#[derive(Component, Clone, Debug)]
pub struct Lay {
    pub def: Params,
    pub walk: MoveToBlock,
    pub next_start: i32,
    pub armed: bool,
    pub slot: u8,
}
/// The goal runs: its system visits it.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Digging;

/// The egg's `eggs` property, 1 to 4 (`eggs - 1` indexes it).
const EGGS: [&[(&str, &str)]; 4] = [&[("eggs", "1")], &[("eggs", "2")], &[("eggs", "3")], &[("eggs", "4")]];

pub static MODULE: Module = Module {
    name: "turtle_lay_egg",
    flags: MOVE | JUMP,
    equip,
    fired,
    can_use,
    start,
    stop,
    tick: Some(tick),
    requires: &[Cap::Body, Cap::Goals, Cap::Locomotion],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, inst: Inst, p: &(dyn std::any::Any + Sync)) {
    let def = *params::<Params>(p);
    let walk = MoveToBlock::new(def.speed, def.range, def.vertical);
    w.entity_mut(e).insert(Lay { def, walk, next_start: 0, armed: false, slot: inst.slot });
    ts::ensure(w, e);
}
/// Arms the next `canUse` request, once: one chain per turtle.
fn chain(w: &mut World, e: Entity, wait: i32) {
    let slot = {
        let Some(mut l) = w.get_mut::<Lay>(e) else { return };
        if l.armed {
            return;
        }
        l.armed = true;
        l.slot
    };
    crate::goals::arm(w, e, slot, wait);
}
fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    if let Some(mut l) = w.get_mut::<Lay>(e) {
        l.armed = false;
    }
    w.get::<Digging>(e).is_none()
}
/// `canUse`'s first test: an egg, and home within 9 blocks.
fn wants_laying(w: &World, e: Entity) -> bool {
    let close = w.get::<Lay>(e).map_or(9.0, |l| l.def.close_home);
    let t = ts::get(w, e);
    t.has_egg && ts::close_to_home(w, e, close)
}
fn valid(mw: &MobWorld, p: [i32; 3]) -> bool {
    mw.facts.is_air(mw.world.block(p[0], p[1] + 1, p[2])) && mw.has_tag("minecraft:sand", mw.block_name(p[0], p[1], p[2]))
}
fn feet(w: &World, e: Entity) -> [i32; 3] {
    let p = w.get::<Pos>(e).unwrap().0;
    [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32]
}
fn reduced(n: i32) -> i32 {
    (n + 1) / 2
}
/// `canUse`: skip the countdown, or draw the next one and search; a search that finds nothing keeps the chain going.
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    if !wants_laying(w, e) {
        return false;
    }
    if w.get::<Lay>(e).unwrap().next_start > 0 {
        w.get_mut::<Lay>(e).unwrap().next_start -= 1;
        chain(w, e, 2);
        return false;
    }
    let countdown = with_rng(w, e, |r| reduced(200 + r.next_int(200)));
    let base = feet(w, e);
    let mut l = w.get::<Lay>(e).unwrap().clone();
    let found = {
        let mw = w.resource::<Level>().mw.0;
        l.walk.may_find(base, mw, |s| mw.has_tag("minecraft:sand", &mw.facts.get(s).name)) && l.walk.find_nearest(base, mw, valid)
    };
    l.next_start = countdown;
    *w.get_mut::<Lay>(e).unwrap() = l;
    if !found {
        chain(w, e, 2);
    }
    found
}
fn start(w: &mut World, e: Entity, _: Inst) {
    let mut l = w.get::<Lay>(e).unwrap().clone();
    with_rng(w, e, |r| l.walk.start(r));
    go_to_at(w, e, l.walk.target(), l.def.speed, 1);
    w.entity_mut(e).insert(Digging);
    *w.get_mut::<Lay>(e).unwrap() = l;
}
/// The end of the goal: a condition that holds asks again at once (the next +2 request).
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Digging>();
    chain(w, e, 2);
}
/// The tick of the module: its system, run by `arbitrate` while an animal carries it (no system in the schedule).
fn tick(w: &mut World) {
    crate::goals::tick_state(w, dig_run);
}
/// The walk and the digging, each tick of the goal (`MoveToBlockGoal.tick` then `LayEgg.tick`): the goal ends when its block is
/// lost or the turtle no longer qualifies; the path is asked again every 40 ticks of walking.
fn dig_run(
    mut q: Query<(Entity, &mut Lay, &mut Turtle, &mut Rng, &Pos, &FluidState), With<Digging>>,
    level: Res<Level>,
    mut states: ResMut<StateChanges>,
    mut reqs: ResMut<Requests>,
) {
    let mw = level.mw.0;
    for (e, mut l, mut t, mut rng, p, f) in &mut q {
        let pos = [p.0.x, p.0.y, p.0.z];
        if !(l.walk.can_continue(mw, valid) && t.has_egg && ts::is_near_home(&t, pos, l.def.close_home)) {
            reqs.0.push(Req::End(e, l.slot));
            continue;
        }
        if l.walk.tick(pos, 1.0) {
            reqs.0.push(Req::Call(e, walk_again));
        }
        if f.in_water || !l.walk.reached {
            continue;
        }
        if t.laying_ticks < 1 {
            t.laying = true;
            t.laying_ticks = 1;
        } else if t.laying_ticks > l.def.lay_ticks {
            let eggs = rng.next_int(4) as usize;
            let b = l.walk.block;
            states.0.push((b[0], b[1] + 1, b[2], "minecraft:turtle_egg", EGGS[eggs]));
            t.has_egg = false;
            t.laying = false;
            t.laying_ticks = 0;
            t.in_love = 600;
        }
        if t.laying {
            t.laying_ticks += 1;
        }
    }
}
fn walk_again(w: &mut World, e: Entity) {
    let Some(l) = w.get::<Lay>(e).cloned() else { return };
    go_to_at(w, e, l.walk.target(), l.def.speed, 1);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behaviours::Entry;
    use crate::cow::{ActiveUntil, Herd, SpeciesTable, spawn_species};
    use crate::species::Config;

    fn config() -> Config {
        let entries: &'static [Entry] = Box::leak(Box::new([Entry::new(&MODULE, 1, &Params::DEFAULT)]));
        Config { entries }
    }
    fn running(h: &Herd, e: Entity) -> bool {
        h.world.get::<GoalSet>(e).unwrap().is_running(&MODULE)
    }
    /// A cow with this goal alone, on the slab, with a sand strip at x 6 to 14 (ground row 0, the feet stand at row 1).
    fn world() -> Option<(Herd, Entity)> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = Herd::with_slab(200, false, false);
        crate::goals::set_lazy_look(&mut h.world, false);
        sand(&mut h, 6, 14, "minecraft:sand");
        let table = h.world.resource::<SpeciesTable>().0.clone();
        let mut sp = *table.iter().find(|s| s.name == "minecraft:cow").unwrap();
        sp.goals = config();
        let c = spawn_species(&mut h.world, &sp, 0.5, 0.5, 77);
        h.world.get_mut::<ActiveUntil>(c).unwrap().0 = u64::MAX;
        h.world.resource_mut::<crate::goals::Entered>().0.push(c);
        h.tick();
        ts::update(&mut h.world, c, |t| t.has_egg = false);
        Some((h, c))
    }
    fn sand(h: &mut Herd, x0: i32, x1: i32, name: &'static str) {
        for x in x0..=x1 {
            for z in -1..=1 {
                h.set_block(x, 0, z, name);
            }
        }
    }
    fn ticks(h: &mut Herd, n: usize) {
        for _ in 0..n {
            h.tick();
        }
    }
    fn lay_egg(h: &mut Herd, c: Entity) {
        ts::update(&mut h.world, c, |t| t.has_egg = true);
        crate::goals::wake(&mut h.world, c, &MODULE);
    }

    /// Without an egg nothing starts; with one, the goal starts (the witness that the wake works).
    #[test]
    fn a_turtle_without_an_egg_starts_nothing_and_one_with_an_egg_does() {
        let Some((mut h, c)) = world() else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        crate::goals::wake(&mut h.world, c, &MODULE);
        ticks(&mut h, 60);
        assert!(!running(&h, c), "pas de départ sans œuf");
        lay_egg(&mut h, c);
        ticks(&mut h, 60);
        assert!(running(&h, c), "témoin : un œuf près de la maison fait partir la tortue");
    }

    /// The turtle walks to the sand (the witness: x grows past 5). Its path stops one block short of the sand, as the Mob's
    /// `nav_move_to` does (reach 1), so the test then stands it on the sand: the digging, the egg above the sand (1 to 4),
    /// `has_egg` cleared and `in_love` set are what is checked.
    #[test]
    fn it_walks_to_the_sand_and_lays_its_egg_there() {
        let Some((mut h, c)) = world() else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        lay_egg(&mut h, c);
        let mut started = false;
        for _ in 0..200 {
            h.tick();
            started |= running(&h, c);
            if started {
                break;
            }
        }
        assert!(started, "témoin : le départ");
        ticks(&mut h, 300);
        let p = h.world.get::<Pos>(c).unwrap().0;
        assert!(p.x > 5.0, "la tortue a marché jusqu'au sable (x = {})", p.x);
        h.world.get_mut::<Pos>(c).unwrap().0 = horizon_core::geometry::Vec3::new(6.5, 1.0, 0.5);
        ticks(&mut h, 400);
        let mw = h.world.resource::<Level>().mw.0;
        let egg = (6..=14).flat_map(|x| (-1..=1).map(move |z| (x, z))).find(|&(x, z)| mw.block_name(x, 1, z) == "minecraft:turtle_egg");
        let l = h.world.get::<Lay>(c).unwrap().clone();
        let t = ts::get(&h.world, c);
        assert!(egg.is_some(), "témoin : un œuf posé au-dessus du sable (block {:?} reached {} laying {} ticks {} egg {})", l.walk.block, l.walk.reached, t.laying, t.laying_ticks, t.has_egg);
        assert!(!t.has_egg, "l'œuf est pondu");
        assert_eq!(t.in_love, 600, "l'amour est posé à 600 après la ponte");
    }

    /// Leaving the sand ends the goal (its block is no longer valid); the witness is the goal running before.
    #[test]
    fn the_goal_ends_when_its_sand_is_gone() {
        let Some((mut h, c)) = world() else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        lay_egg(&mut h, c);
        let mut started = false;
        for _ in 0..200 {
            h.tick();
            started |= running(&h, c);
            if started {
                break;
            }
        }
        assert!(started, "témoin : le départ");
        sand(&mut h, 6, 14, "minecraft:stone");
        ticks(&mut h, 3);
        assert!(!running(&h, c), "le but s'arrête quand le sable disparaît");
    }
}
