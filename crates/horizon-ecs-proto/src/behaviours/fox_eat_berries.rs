//! `Fox.FoxEatBerriesGoal` (fiche 27), a `MoveToBlockGoal`: a fox walks to a ripe berry bush (or a cave vine with berries)
//! within 12 blocks and, once it has waited 40 game ticks at it, picks it. The search is a deadline of the wheel: after a
//! search the next one comes `reduced(200 + U(200))` asks later, the asks counted from the end of a goal (the `Mob` does
//! not ask while the goal runs). Not ported, no service yet: the berries on the ground and in the hand, the bush going
//! back to age 1, the berries on the ground and in the mouth, fiche 41. Not ported: the sniff sound, `GameEvent`s.
use super::fox_state::{self as fs, SITTING, SLEEPING};
use super::*;
use crate::blocks::set_state;
use crate::{cow::*, goals::*, move_to_block::MoveToBlock};
use horizon_client_lib::items::ItemStack;
use horizon_client_lib::mobs::MobWorld;
use horizon_core::geometry::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub speed: f64,
    pub range: i32,
    pub vertical: i32,
    /// The game ticks the fox waits at the bush before it picks it.
    pub wait: i32,
    /// The delay of the berries that fall on the ground (`spawn_item_own`).
    pub drop_delay: i64,
}
impl Params {
    pub const DEFAULT: Self = Self { speed: 1.2_f32 as f64, range: 12, vertical: 1, wait: 40, drop_delay: 10 };
}
/// The goal's data: its rank, its wait and the delay of the berries it drops.
#[derive(Component, Clone, Copy, Debug)]
pub struct BerryDef {
    pub slot: u8,
    pub wait: i32,
    pub drop_delay: i64,
}
/// The next search: the local time it may run at, and the `next_start` drawn by the last search.
#[derive(Component, Clone, Copy, Debug)]
pub struct BerrySearch {
    pub due: u64,
    pub next: i32,
}
/// The goal runs: the game ticks spent at the block.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Picking {
    pub waited: i32,
}
/// The berries of the last pick.
#[derive(Component, Clone, Copy, Debug)]
pub struct Picked(pub i32);

pub static MODULE: Module = Module {
    name: "fox_eat_berries",
    flags: MOVE | JUMP,
    equip,
    arm: arm_first,
    fired,
    can_use,
    start,
    stop,
    tick: Some(tick),
    requires: &[Cap::Body, Cap::Goals, Cap::Locomotion],
    ..Module::DEFAULT
};

fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    fs::ensure(w, e);
    let p = *params::<Params>(p);
    w.entity_mut(e).insert((
        BerryDef { slot: inst.slot, wait: p.wait, drop_delay: p.drop_delay },
        MoveToBlock::new(p.speed, p.range, p.vertical),
        BerrySearch { due: 0, next: 0 },
    ));
}
/// The first search: the first ask of the `Mob` (its `next_start` is 0 at creation).
fn arm_first(w: &mut World, e: Entity, inst: Inst) {
    crate::goals::arm(w, e, inst.slot, 1);
}
/// A deadline of the search, or the framework's replay of a start request: while the goal runs there is no search.
fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    w.get::<Picking>(e).is_none()
}
/// `sweet_berry_bush` of age 2 or more, or `cave_vines` with berries (`Fox.hasBerries`).
fn has_berries(mw: &MobWorld, p: [i32; 3]) -> bool {
    match mw.block_name(p[0], p[1], p[2]) {
        "minecraft:sweet_berry_bush" => mw.prop(p[0], p[1], p[2], "age").and_then(|a| a.parse::<i32>().ok()).is_some_and(|a| a >= 2),
        "minecraft:cave_vines" | "minecraft:cave_vines_plant" => mw.prop(p[0], p[1], p[2], "berries") == Some("true"),
        _ => false,
    }
}
/// `Goal.reducedTickDelay`.
fn reduced(n: i32) -> i32 {
    (n + 1) / 2
}
fn feet(w: &World, e: Entity) -> [i32; 3] {
    let p = w.get::<Pos>(e).unwrap().0;
    [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32]
}
/// `EatBerriesGoal.canUse`: not asleep, the search is due; it draws the next delay and searches the nearest ripe block.
/// A search that finds nothing is asked again after that delay.
fn can_use(w: &mut World, e: Entity, inst: Inst) -> bool {
    if fs::has(w, e, SLEEPING) {
        return false;
    }
    let now = now_of(w, e);
    if w.get::<BerrySearch>(e).is_some_and(|s| now < s.due) {
        return false;
    }
    let next = with_rng(w, e, |r| reduced(200 + r.next_int(200)));
    let base = feet(w, e);
    let mut m = w.get::<MoveToBlock>(e).unwrap().clone();
    let found = {
        let mw = w.resource::<Level>().mw.0;
        m.may_find(base, mw, |s| matches!(&*mw.facts.get(s).name, "minecraft:sweet_berry_bush" | "minecraft:cave_vines" | "minecraft:cave_vines_plant")) && m.find_nearest(base, mw, has_berries)
    };
    *w.get_mut::<MoveToBlock>(e).unwrap() = m;
    w.get_mut::<BerrySearch>(e).unwrap().next = next;
    if found {
        return true;
    }
    let wait = 2 * (next + 1);
    w.get_mut::<BerrySearch>(e).unwrap().due = now + wait as u64;
    crate::goals::arm(w, e, inst.slot, wait);
    false
}
/// `EatBerriesGoal.start`: the head goes down, the walk to the block above starts, the wait starts; the first tick is the
/// one the selector gives a started goal.
fn start(w: &mut World, e: Entity, inst: Inst) {
    fs::set(w, e, SITTING, false);
    let mut m = w.get::<MoveToBlock>(e).unwrap().clone();
    with_rng(w, e, |r| m.start(r));
    let (at, speed) = (m.target(), m.speed);
    *w.get_mut::<MoveToBlock>(e).unwrap() = m;
    go_to_at(w, e, at, speed, 1);
    w.entity_mut(e).insert(Picking { waited: 0 });
    tick_once(w, e, inst.slot);
}
/// The goal ends: the next search is `next` asks away from now (the `Mob` counts them while it does not run).
fn stop(w: &mut World, e: Entity, inst: Inst) {
    w.entity_mut(e).remove::<Picking>();
    let next = w.get::<BerrySearch>(e).unwrap().next;
    let wait = 2 * (next + 1);
    let now = now_of(w, e);
    w.get_mut::<BerrySearch>(e).unwrap().due = now + wait as u64;
    crate::goals::arm(w, e, inst.slot, wait);
}
/// The tick of the module, run only while a berry-picking fox lives (no registered system).
fn tick(w: &mut World) {
    berries_run(w);
}
/// Every tick of a running picking: the block must still have berries, else the goal ends; else its tick.
fn berries_run(w: &mut World) {
    let list = crate::goals::entities::<With<Picking>>(w);
    for e in list {
        let Some(slot) = w.get::<BerryDef>(e).map(|d| d.slot) else { continue };
        let go_on = {
            let mw = w.resource::<Level>().mw.0;
            w.get::<MoveToBlock>(e).unwrap().can_continue(mw, has_berries)
        };
        if go_on {
            tick_once(w, e, slot);
        } else {
            end_goal(w, e, slot);
        }
    }
}
/// `EatBerriesGoal.tick` (`MoveToBlockGoal.tickWithin(2)`): at the block, the wait counts and then the pick happens (when
/// `mobGriefing` allows it), the walk is asked again every 40 ticks of walking; a pick ends the goal.
fn tick_once(w: &mut World, e: Entity, slot: u8) {
    let Some(mut p) = w.get::<Picking>(e).copied() else { return };
    let wait = w.get::<BerryDef>(e).unwrap().wait;
    let mut m = w.get::<MoveToBlock>(e).unwrap().clone();
    let at = w.get::<Pos>(e).unwrap().0;
    let mut picked = false;
    if m.reached {
        if p.waited >= wait {
            picked = mob_griefing(w);
        } else {
            p.waited += 1;
        }
    }
    let ask = m.tick([at.x, at.y, at.z], 2.0);
    let (target, speed, block) = (m.target(), m.speed, m.block);
    *w.get_mut::<MoveToBlock>(e).unwrap() = m;
    w.entity_mut(e).insert(p);
    if ask {
        go_to_at(w, e, target, speed, 1);
    }
    if picked {
        pick(w, e, block);
        end_goal(w, e, slot);
    }
}
/// `EatBerriesGoal.pick` (`mobs/fox.rs:1459-1514`): a sweet berry bush gives `1 + U(2)` berries, one more at age 3; the first one goes
/// into an empty mouth, the others fall at the block's centre; the bush goes back to age 1. A cave vine gives one glow berry and
/// loses its berries.
fn pick(w: &mut World, e: Entity, b: [i32; 3]) {
    let (sweet, age, plant) = {
        let mw = w.resource::<Level>().mw.0;
        let name = mw.block_name(b[0], b[1], b[2]);
        let age = mw.prop(b[0], b[1], b[2], "age").and_then(|a| a.parse::<i32>().ok()).unwrap_or(0);
        (name == "minecraft:sweet_berry_bush", age, name == "minecraft:cave_vines_plant")
    };
    let delay = w.get::<BerryDef>(e).unwrap().drop_delay;
    let center = Vec3::new(f64::from(b[0]) + 0.5, f64::from(b[1]) + 0.5, f64::from(b[2]) + 0.5);
    if sweet {
        let count = 1 + with_rng(w, e, |r| r.next_int(2)) + i32::from(age == 3);
        let mut rest = count;
        if main_hand(w, e).is_none() {
            set_main_hand(w, e, Some(ItemStack::new("minecraft:sweet_berries", 1)));
            rest -= 1;
        }
        if rest > 0 {
            toss(w, e, "minecraft:sweet_berries", rest as u32, center, delay);
        }
        set_state(w, b, "minecraft:sweet_berry_bush", &[("age", "1")]);
        w.entity_mut(e).insert(Picked(count));
    } else {
        toss(w, e, "minecraft:glow_berries", 1, center, delay);
        let name = if plant { "minecraft:cave_vines_plant" } else { "minecraft:cave_vines" };
        set_state(w, b, name, &[("berries", "false")]);
        w.entity_mut(e).insert(Picked(1));
    }
}
/// `spawn_item_own`: the stack thrown as `ItemEntity`'s constructor throws it, from the fox's own random.
fn toss(w: &mut World, e: Entity, name: &str, count: u32, at: Vec3, delay: i64) {
    let (a, b) = with_rng(w, e, |r| (r.next_f64(), r.next_f64()));
    crate::item::drop_item(w, ItemStack::new(name, count), at, Vec3::new(a * 0.2 - 0.1, 0.2, b * 0.2 - 0.1), delay);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::species::Config;

    static ENTRIES: [Entry; 1] = [Entry::new(&MODULE, 10, &Params::DEFAULT)];

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
    fn running(h: &Herd, e: Entity) -> bool {
        h.world.get::<GoalSet>(e).unwrap().is_running(&MODULE)
    }
    /// Without a ripe bush the fox searches (the search draws its delay, between 100 and 200 asks) and never starts.
    #[test]
    fn without_a_ripe_bush_the_fox_searches_and_never_starts() {
        let Some(mut h) = herd() else { return };
        h.set_block(5, 0, 0, "minecraft:sweet_berry_bush");
        let e = fox(&mut h);
        for _ in 0..800 {
            h.tick();
            assert!(!running(&h, e), "an unripe bush is not a target");
        }
        let next = h.world.get::<BerrySearch>(e).unwrap().next;
        assert!((100..=200).contains(&next), "the search ran and drew its delay: {next}");
    }
}
