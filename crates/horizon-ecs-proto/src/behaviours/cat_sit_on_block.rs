//! `CatSitOnBlockGoal` (fiche 64, `Mob` `tame.rs:1496-1555`, priorité 7 au code, `MoveToBlock(0.8, range 8, vertical 1)`): a tame cat that is
//! not sitting by order searches a block of `cats_can_sit_on` with air above (a lit furnace, a bed's foot, a chest) and sits there while
//! reached. Each search draws `reduced(200 + nextInt(200))` asks before the next one (2 ticks per ask): the countdown is a deadline.
//! Absent: the open-chest check (the chests are taken as closed: DOUTE, the openers are not in the frame).
use super::cat_state::{self as cs, empty_above, reduced};
use super::*;
use crate::move_to_block::MoveToBlock;
use crate::{cow::*, goals::*};
use horizon_client_lib::mobs::MobWorld;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub speed: f64,
    pub range: i32,
    pub vertical: i32,
    /// The countdown is `reduced(delay + nextInt(delay))` asks.
    pub delay: i32,
}
impl Params {
    pub const DEFAULT: Self = Self { speed: 0.8, range: 8, vertical: 1, delay: 200 };
}

#[derive(Component, Clone, Debug)]
pub struct Sit {
    walk: MoveToBlock,
    speed: f64,
    delay: i32,
    slot: u8,
    armed: bool,
    /// The countdown drawn at the last search, in asks: the next search comes that many asks (2 ticks each) after the goal ends.
    countdown: i32,
}
/// The goal runs: its system visits this cat each tick.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct SittingOn;

pub static MODULE: Module = Module {
    name: "cat_sit_on_block",
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
    let p = *params::<Params>(p);
    cs::ensure(w, e);
    let walk = MoveToBlock::new(p.speed, p.range, p.vertical);
    w.entity_mut(e).insert(Sit { walk, speed: p.speed, delay: p.delay, slot: inst.slot, armed: false, countdown: 0 });
}
fn arm(w: &mut World, e: Entity, _: Inst) {
    schedule(w, e, 1);
}
/// Arms the next search, once: a second arming would start a second chain of searches.
fn schedule(w: &mut World, e: Entity, wait: i32) {
    let slot = {
        let Some(mut s) = w.get_mut::<Sit>(e) else { return };
        if s.armed {
            return;
        }
        s.armed = true;
        s.slot
    };
    crate::goals::arm(w, e, slot, wait);
}
fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    if let Some(mut s) = w.get_mut::<Sit>(e) {
        s.armed = false;
    }
    w.get::<SittingOn>(e).is_none()
}
/// `Mob`'s `can_sit_on`: air above, the tag, then the block: a furnace only when lit, a bed's foot only, a chest (taken as closed).
pub fn can_sit_on(mw: &MobWorld, p: [i32; 3]) -> bool {
    if !empty_above(mw, p) || !mw.has_tag("minecraft:cats_can_sit_on", mw.block_name(p[0], p[1], p[2])) {
        return false;
    }
    match mw.block_name(p[0], p[1], p[2]) {
        "minecraft:furnace" => mw.facts.get(mw.world.block(p[0], p[1], p[2])).emission > 0,
        "minecraft:chest" => true,
        _ => crate::blocks::property(mw, p, "part").as_deref() != Some("head"),
    }
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    if !cs::tamed(w, e) || is_occupied(w, e, OCC_SITTING) {
        return false;
    }
    let base = cs::feet(w, e);
    let delay = w.get::<Sit>(e).unwrap().delay;
    let next = with_rng(w, e, |r| reduced(delay + r.next_int(delay)));
    let mut s = w.get::<Sit>(e).unwrap().clone();
    s.countdown = next;
    let found = s.walk.find_nearest(base, cs::level(w), can_sit_on);
    *w.get_mut::<Sit>(e).unwrap() = s;
    if !found {
        schedule(w, e, 2 * (next + 1));
    }
    found
}
fn start(w: &mut World, e: Entity, _: Inst) {
    let mut s = w.get::<Sit>(e).unwrap().clone();
    with_rng(w, e, |r| s.walk.start(r));
    go_to_at(w, e, s.walk.target(), s.speed, 1);
    cs::set(w, e, |c| c.sitting_pose = false);
    w.entity_mut(e).insert(SittingOn);
    *w.get_mut::<Sit>(e).unwrap() = s;
}
/// The end: not sitting; the next search comes after the countdown drawn at the last search.
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<SittingOn>();
    cs::set(w, e, |c| c.sitting_pose = false);
    let n = w.get::<Sit>(e).map_or(0, |s| s.countdown);
    schedule(w, e, 2 * (n + 1));
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems(sit_run.in_set(slot));
}
fn sit_run(q: Query<Entity, With<SittingOn>>, mut reqs: ResMut<Requests>) {
    for e in &q {
        reqs.0.push(Req::Call(e, sit_step));
    }
}
/// One tick of the goal: it ends when the block is lost or the stay is over; else it walks (asked again every 40 ticks) and sits
/// while reached.
fn sit_step(w: &mut World, e: Entity) {
    let Some(mut s) = w.get::<Sit>(e).cloned() else { return };
    if !s.walk.can_continue(cs::level(w), can_sit_on) {
        w.resource_mut::<Requests>().0.push(Req::End(e, s.slot));
        return;
    }
    let pos = w.get::<Pos>(e).unwrap().0;
    if s.walk.tick([pos.x, pos.y, pos.z], 1.0) {
        go_to_at(w, e, s.walk.target(), s.speed, 1);
    }
    let reached = s.walk.reached;
    cs::set(w, e, |c| c.sitting_pose = reached);
    *w.get_mut::<Sit>(e).unwrap() = s;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behaviours::Entry;
    use crate::cow::{ActiveUntil, SpeciesTable, spawn_species};
    use crate::species::Config;

    fn config() -> Config {
        let entries: &'static [Entry] = Box::leak(Box::new([Entry::new(&MODULE, 7, &Params::DEFAULT)]));
        Config { entries }
    }
    fn running(h: &Herd, e: Entity) -> bool {
        h.world.get::<GoalSet>(e).unwrap().is_running(&MODULE)
    }
    /// A tame wolf (owner player 1) with this goal alone, awake, at (0.5, 0.5) on the slab.
    fn world() -> Option<(Herd, Entity)> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = Herd::with_slab(200, false, false);
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
    fn ticks(h: &mut Herd, n: usize) {
        for _ in 0..n {
            h.tick();
        }
    }

    /// A lit furnace 5 blocks away: the cat walks to it and sits; an unlit one is never searched as a place (the witness is the lit one).
    #[test]
    fn a_cat_walks_to_a_lit_furnace_and_sits_there() {
        let Some((mut h, c)) = world() else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        h.set_block_state(5, 0, 0, "minecraft:furnace", &[("facing", "north"), ("lit", "false")]);
        ticks(&mut h, 120);
        assert!(!running(&h, c), "témoin négatif : un four éteint n'attire pas le chat");
        h.set_block_state(5, 0, 0, "minecraft:furnace", &[("facing", "north"), ("lit", "true")]);
        let mut started = false;
        for _ in 0..600 {
            h.tick();
            started |= running(&h, c);
            if started {
                break;
            }
        }
        assert!(started, "témoin : le départ vers le four allumé");
        ticks(&mut h, 200);
        let p = h.world.get::<Pos>(c).unwrap().0;
        // The walk ends one block short of the block (`nav_move_to`, reach 1, as the `Mob`): the sitting is not reached here.
        assert!(p.x > 3.5, "le chat se dirige vers le four (x {})", p.x);
        assert!(running(&h, c), "le but tient la marche tant que le four est là");
    }
}
