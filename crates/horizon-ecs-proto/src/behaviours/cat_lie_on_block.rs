//! `CatLieOnBlockGoal` (fiche 63, `Mob` `tame.rs:1464-1495`, priorité 5, `MoveToBlock(1.1, range 8, vertical 6 from -2)`): a tame cat that is
//! neither sitting nor lying searches for a block of `cats_can_lie_on` with air above, walks to it, and lies there while it stays
//! reached. The search is a deadline: `reduced(40)` asks (one ask per 2 ticks), so 2 x (asks + 1) ticks between two searches.
//! Refused while sitting, lying or untamed: nothing is armed (the wake-up when the cat is tamed or stands up is a BESOIN DU CADRE).
use super::cat_state::{self as cs};
use super::*;
use crate::move_to_block::MoveToBlock;
use crate::rules::cat_lie_on_block::{can_lie_on, search_ticks};
use crate::{cow::*, goals::*};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub speed: f64,
    pub range: i32,
    pub vertical: i32,
    pub vertical_start: i32,
    /// `reduced(40)`: the asks between two searches (the countdown of the `Mob`'s `MoveToBlock`).
    pub asks: i32,
}
impl Params {
    pub const DEFAULT: Self = Self { speed: 1.1, range: 8, vertical: 6, vertical_start: -2, asks: 20 };
}

#[derive(Component, Clone, Debug)]
pub struct Lie {
    p: &'static Params,
    walk: MoveToBlock,
    slot: u8,
    armed: bool,
}
/// The goal runs: its system visits this cat each tick.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct LyingDown;

pub static MODULE: Module = Module {
    name: "cat_lie_on_block",
    flags: MOVE | JUMP,
    equip,
    arm,
    fired,
    can_use,
    start,
    stop,
    systems,
    requires: &[Cap::Body, Cap::Goals, Cap::Locomotion, Cap::Tamable],
    ..Module::DEFAULT
};

fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = params::<Params>(p);
    cs::ensure(w, e);
    let mut walk = MoveToBlock::new(p.speed, p.range, p.vertical);
    walk.vertical_start = p.vertical_start;
    w.entity_mut(e).insert(Lie { p, walk, slot: inst.slot, armed: false });
}
fn arm(w: &mut World, e: Entity, _: Inst) {
    schedule(w, e, 1);
}
/// Arms the next search, once: a second arming would start a second chain of searches.
fn schedule(w: &mut World, e: Entity, wait: i32) {
    let slot = {
        let Some(mut l) = w.get_mut::<Lie>(e) else { return };
        if l.armed {
            return;
        }
        l.armed = true;
        l.slot
    };
    crate::goals::arm(w, e, slot, wait);
}
/// A deadline came up: asked unless the goal runs (a start replayed by the frame comes through here too).
fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    if let Some(mut l) = w.get_mut::<Lie>(e) {
        l.armed = false;
    }
    w.get::<LyingDown>(e).is_none()
}
fn period(l: &Lie) -> i32 {
    search_ticks(l.p.asks)
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    if !cs::tamed(w, e) || is_occupied(w, e, OCC_SITTING) || cs::pose(w, e).lying {
        return false;
    }
    let base = cs::feet(w, e);
    let mut l = w.get::<Lie>(e).unwrap().clone();
    let found = l.walk.find_nearest(base, cs::level(w), can_lie_on);
    let wait = period(&l);
    *w.get_mut::<Lie>(e).unwrap() = l;
    if !found {
        schedule(w, e, wait);
    }
    found
}
fn start(w: &mut World, e: Entity, _: Inst) {
    let mut l = w.get::<Lie>(e).unwrap().clone();
    with_rng(w, e, |r| l.walk.start(r));
    go_to_at(w, e, l.walk.target(), l.p.speed, 1);
    cs::set(w, e, |c| c.sitting_pose = false);
    w.entity_mut(e).insert(LyingDown);
    *w.get_mut::<Lie>(e).unwrap() = l;
}
/// The end: not lying; the next search comes one period later.
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<LyingDown>();
    cs::set(w, e, |c| c.lying = false);
    let wait = w.get::<Lie>(e).map_or(0, period);
    schedule(w, e, wait);
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems(lie_run.in_set(slot));
}
fn lie_run(q: Query<Entity, With<LyingDown>>, mut reqs: ResMut<Requests>) {
    for e in &q {
        reqs.0.push(Req::Call(e, lie_step));
    }
}
/// One tick of the goal: it ends when the block is lost or the stay is over; else it walks (the path asked again every 40 ticks)
/// and lies while reached.
fn lie_step(w: &mut World, e: Entity) {
    let Some(mut l) = w.get::<Lie>(e).cloned() else { return };
    let ok = l.walk.can_continue(cs::level(w), can_lie_on);
    if !ok {
        w.resource_mut::<Requests>().0.push(Req::End(e, l.slot));
        return;
    }
    let pos = w.get::<Pos>(e).unwrap().0;
    if l.walk.tick([pos.x, pos.y, pos.z], 1.0) {
        go_to_at(w, e, l.walk.target(), l.p.speed, 1);
    }
    let reached = l.walk.reached;
    cs::set(w, e, |c| {
        c.sitting_pose = false;
        c.lying = reached;
    });
    *w.get_mut::<Lie>(e).unwrap() = l;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behaviours::Entry;
    use crate::cow::{ActiveUntil, SpeciesTable, spawn_species};
    use crate::species::Config;

    fn config() -> Config {
        let entries: &'static [Entry] = Box::leak(Box::new([Entry::new(&MODULE, 5, &Params::DEFAULT)]));
        Config { entries }
    }
    fn running(h: &Herd, e: Entity) -> bool {
        h.world.get::<GoalSet>(e).unwrap().is_running(&MODULE)
    }
    /// A tame wolf (owner player 1) with this goal alone, awake, at (0.5, 0.5) on the slab.
    fn world() -> Option<(Herd, Entity)> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = Herd::with_slab_awake(200, false, false);
        crate::goals::set_lazy_look(&mut h.world, false);
        let table = h.world.resource::<SpeciesTable>().0.clone();
        let mut sp = *table.iter().find(|s| s.name == "minecraft:wolf").unwrap();
        sp.goals = config();
        let c = spawn_species(&mut h.world, &sp, 0.5, 0.5, 77);
        h.world.get_mut::<ActiveUntil>(c).unwrap().0 = u64::MAX;
        h.world.resource_mut::<crate::goals::Entered>().0.push(c);
        crate::tame::set_owner(&mut h.world, c, crate::targeting::Who::Player(1));
        h.tick();
        Some((h, c))
    }
    /// A bed in a hole: the search of the `Mob` visits the heights -2, 3, -3, 4... (`verticalSearchStart` -2), never the ground
    /// row, so the bed sits two blocks under the cat's feet, with air above it.
    fn bed(h: &mut Herd, x: i32) {
        h.set_block(x, 0, 0, "minecraft:air");
        h.set_block(x, -1, 0, "minecraft:air");
        h.set_block_state(x, -2, 0, "minecraft:white_bed", &[("part", "foot")]);
    }
    fn ticks(h: &mut Herd, n: usize) {
        for _ in 0..n {
            h.tick();
        }
    }

    /// A bed 6 blocks away: the cat walks to it and lies on it; a bed placed later is the witness that an absent bed starts nothing.
    #[test]
    fn a_cat_walks_to_a_bed_in_reach() {
        let Some((mut h, c)) = world() else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        ticks(&mut h, 60);
        assert!(!running(&h, c), "témoin négatif : pas de lit, pas de départ");
        bed(&mut h, 6);
        let mut started = false;
        for _ in 0..200 {
            h.tick();
            started |= running(&h, c);
            if started {
                break;
            }
        }
        assert!(started, "témoin : le départ vers le lit posé à portée");
        ticks(&mut h, 200);
        let p = h.world.get::<Pos>(c).unwrap().0;
        assert!(p.x > 2.0, "le chat se dirige vers le lit (x {})", p.x);
    }

    /// Removing the bed ends the stay: the cat stops and is no longer lying.
    #[test]
    fn the_stay_ends_when_the_bed_is_gone() {
        let Some((mut h, c)) = world() else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        bed(&mut h, 6);
        let mut started = false;
        for _ in 0..200 {
            h.tick();
            started |= running(&h, c);
            if started {
                break;
            }
        }
        assert!(started, "témoin : le départ");
        ticks(&mut h, 150);
        h.set_block(6, -2, 0, "minecraft:air");
        ticks(&mut h, 4);
        assert!(!running(&h, c), "le but s'arrête quand le lit disparaît");
    }
}
