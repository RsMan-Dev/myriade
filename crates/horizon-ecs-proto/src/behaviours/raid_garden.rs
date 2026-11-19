//! `Rabbit.RaidGardenGoal` (fiche 46, `rabbit.rs:80-195`), a `MoveToBlockGoal`: a rabbit that wants food walks to a ripe carrot
//! (age 7, on `#supports_crops`) within 16 blocks and eats one age of it, then is full for 40 ticks. The search is a deadline:
//! each search draws the next delay (`reduced(200 + U(200))` asks), a raid sets the next one to 10 asks.
//! Ported as the code says (DOUTE, see the notes): `can_continue` passes `can_raid` (true while the goal runs) to a predicate
//! that wants `!can_raid`, so the goal ends at its first check. The jump of the rabbit is not ported (`MoveKind::Custom`).
use super::*;
use crate::{cow::*, goals::*, move_to_block::MoveToBlock};
use horizon_client_lib::mobs::MobWorld;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub speed: f64,
    pub range: i32,
    pub vertical: i32,
    /// `moreCarrotTicks` after a raid.
    pub wait: i32,
    /// The asks before the next search after a raid (`block.next_start = 10`).
    pub delay: i32,
}
impl Params {
    pub const DEFAULT: Self = Self { speed: 0.7, range: 16, vertical: 1, wait: 40, delay: 10 };
}

/// The raid state: the search (`block`), what the last search chose (`wants`, `can_raid`), the asks before the next search
/// (`next`), the local time of its only valid deadline (`armed_at`), and the local time the rabbit is full until (`more_until`).
#[derive(Component, Clone, Debug)]
pub struct Raid {
    pub block: MoveToBlock,
    pub wants: bool,
    pub can_raid: bool,
    pub next: i32,
    pub armed_at: u64,
    pub more_until: u64,
    pub wait: i32,
    pub delay: i32,
    pub slot: u8,
}
/// The goal runs: its state is checked every tick.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Raiding;

static AGES: [&[(&str, &str)]; 8] = [
    &[("age", "0")],
    &[("age", "1")],
    &[("age", "2")],
    &[("age", "3")],
    &[("age", "4")],
    &[("age", "5")],
    &[("age", "6")],
    &[("age", "7")],
];
const CARROT_MAX_AGE: i32 = 7;

pub static MODULE: Module = Module {
    name: "raid_garden",
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

fn equip(w: &mut World, e: Entity, inst: Inst, p: &(dyn std::any::Any + Sync)) {
    let p = *params::<Params>(p);
    w.entity_mut(e).insert(Raid {
        block: MoveToBlock::new(p.speed, p.range, p.vertical),
        wants: false,
        can_raid: false,
        next: 0,
        armed_at: u64::MAX,
        more_until: 0,
        wait: p.wait,
        delay: p.delay,
        slot: inst.slot,
    });
}
/// The first search: the `Mob`'s `next_start` is 0 at creation, so the first ask searches.
fn arm_first(w: &mut World, e: Entity, _: Inst) {
    arm_in(w, e, 1);
}
fn arm_in(w: &mut World, e: Entity, ticks: i32) {
    let slot = w.get::<Raid>(e).unwrap().slot;
    let at = now_of(w, e) + ticks.max(1) as u64;
    w.get_mut::<Raid>(e).unwrap().armed_at = at;
    arm_at(w, e, at, Due::Goal(slot));
}
/// Arms the next search after `asks` asks (a selector asks every 2 ticks of a goal with no flag asking, here the MOVE flag
/// is free so it is every 2 ticks too).
fn arm_asks(w: &mut World, e: Entity, asks: i32) {
    arm_in(w, e, 2 * (asks.max(0) + 1));
}
/// `reduced(n)`.
fn reduced(n: i32) -> i32 {
    (n + 1) / 2
}
/// `RaidGarden.isValidTarget`: `#supports_crops` under a carrot at its maximum age, while the rabbit wants food and has not
/// found one (`can_raid` must be false here, as the `Mob` writes it).
fn raid_target(mw: &MobWorld, p: [i32; 3], wants: bool, can_raid: bool) -> bool {
    mw.has_tag("minecraft:supports_crops", mw.block_name(p[0], p[1], p[2]))
        && wants
        && !can_raid
        && mw.block_name(p[0], p[1] + 1, p[2]) == "minecraft:carrots"
        && mw.prop(p[0], p[1] + 1, p[2], "age").and_then(|a| a.parse::<i32>().ok()).is_some_and(|age| age >= CARROT_MAX_AGE)
}
fn feet(w: &World, e: Entity) -> [i32; 3] {
    let p = w.get::<Pos>(e).unwrap().0;
    [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32]
}
/// A deadline of the search (or of the asks that wait): true = ask `canUse`; the goal running or a stale deadline: nothing.
fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    if w.get::<Raiding>(e).is_some() {
        return false;
    }
    let now = now_of(w, e);
    w.get::<Raid>(e).is_some_and(|r| r.armed_at == now)
}
/// `RaidGarden.canUse`: while mob griefing is off nothing is searched (the `Mob` draws nothing); the search draws the next
/// delay and takes the nearest ripe carrot; a search that finds none is asked again after that delay.
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    if !mob_griefing(w) {
        return false;
    }
    let now = now_of(w, e);
    let wants = now >= w.get::<Raid>(e).unwrap().more_until;
    let next = with_rng(w, e, |r| reduced(200 + r.next_int(200)));
    let base = feet(w, e);
    let mut block = w.get::<Raid>(e).unwrap().block.clone();
    let found = {
        let mw = w.resource::<Level>().mw.0;
        // The palette sweep of the `Mob` (`may_find`): no carrot plant in the zone, no walk of its cells.
        block.may_find(base, mw, |s| &*mw.facts.get(s).name == "minecraft:carrots") && block.find_nearest(base, mw, |mw, p| raid_target(mw, p, wants, false))
    };
    {
        let mut r = w.get_mut::<Raid>(e).unwrap();
        r.block = block;
        r.wants = wants;
        r.can_raid = found;
        r.next = next;
    }
    if found {
        return true;
    }
    arm_asks(w, e, next);
    false
}
/// `start`: the walk to the block above starts; the goal ticks at once (the `Mob` ticks a started goal the same tick).
fn start(w: &mut World, e: Entity, inst: Inst) {
    let mut m = w.get::<Raid>(e).unwrap().block.clone();
    with_rng(w, e, |r| m.start(r));
    let (at, speed) = (m.target(), m.speed);
    w.get_mut::<Raid>(e).unwrap().block = m;
    w.entity_mut(e).insert(Raiding);
    go_to_at(w, e, at, speed, 1);
    tick_once(w, e, inst.slot);
}
/// The goal ends: the next search is `next` asks away (after a raid, 10).
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Raiding>();
    let next = w.get::<Raid>(e).unwrap().next;
    arm_asks(w, e, next);
}
/// The tick of the module: its system, run by `arbitrate` while an animal carries it (no system in the schedule).
fn tick(w: &mut World) {
    crate::goals::tick_state(w, raid_run);
}
/// Every tick of a running raid: the tick, then the end if the goal cannot continue (`canContinueToUse`).
fn raid_run(w: &mut World, q: &mut QueryState<Entity, With<Raiding>>) {
    let list: Vec<Entity> = q.iter(w).collect();
    for e in list {
        let slot = w.get::<Raid>(e).unwrap().slot;
        tick_once(w, e, slot);
        if !can_continue(w, e) {
            end_goal(w, e, slot);
        }
    }
}
/// `canContinueToUse`: `canRaid && block.canContinueToUse(raid_target(.., canRaid))`, as the `Mob` writes it.
fn can_continue(w: &World, e: Entity) -> bool {
    let r = w.get::<Raid>(e).unwrap();
    let mw = w.resource::<Level>().mw.0;
    let (wants, raid) = (r.wants, r.can_raid);
    r.can_raid && r.block.can_continue(mw, |mw, p| raid_target(mw, p, wants, raid))
}
/// `RaidGarden.tick`: the walk and the look (the look is not ported); once reached, the carrot is eaten (one age, or removed
/// at age 0), the rabbit is full for `wait` ticks, and the next search comes after `delay` asks.
fn tick_once(w: &mut World, e: Entity, _: u8) {
    let at = w.get::<Pos>(e).unwrap().0;
    let mut m = w.get::<Raid>(e).unwrap().block.clone();
    let ask = m.tick([at.x, at.y, at.z], 1.0);
    let (target, speed) = (m.target(), m.speed);
    let reached = m.reached;
    let block = m.block;
    w.get_mut::<Raid>(e).unwrap().block = m;
    if ask {
        go_to_at(w, e, target, speed, 1);
    }
    if !reached {
        return;
    }
    let (can_raid, wait, delay) = {
        let r = w.get::<Raid>(e).unwrap();
        (r.can_raid, r.wait, r.delay)
    };
    let crop = [block[0], block[1] + 1, block[2]];
    if can_raid && w.resource::<Level>().mw.0.block_name(crop[0], crop[1], crop[2]) == "minecraft:carrots" {
        let age = {
            let mw = w.resource::<Level>().mw.0;
            mw.prop(crop[0], crop[1], crop[2], "age").and_then(|a| a.parse::<i32>().ok()).unwrap_or(0)
        };
        if age == 0 {
            if mob_griefing(w) {
                w.resource_mut::<BlockChanges>().0.push((crop[0], crop[1], crop[2], "minecraft:air"));
            }
        } else if mob_griefing(w) {
            crate::blocks::set_state(w, crop, "minecraft:carrots", AGES[(age - 1) as usize]);
        }
        let now = now_of(w, e);
        w.get_mut::<Raid>(e).unwrap().more_until = now + wait as u64;
    }
    let mut r = w.get_mut::<Raid>(e).unwrap();
    r.can_raid = false;
    r.next = delay;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::species::Config;

    static ENTRIES: [Entry; 1] = [Entry::new(&MODULE, 5, &Params::DEFAULT)];

    fn herd() -> Option<Herd> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = Herd::with_slab(30, false, false);
        set_lazy_look(&mut h.world, false);
        Some(h)
    }
    fn rabbit(h: &mut Herd) -> Entity {
        let mut sp = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == "minecraft:cow").unwrap();
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
    /// A ripe carrot field: farmland at (5, 0, 0), carrots at age 7 above it.
    fn ripe(h: &mut Herd) {
        h.set_block(5, 0, 0, "minecraft:farmland");
        h.set_block_state(5, 1, 0, "minecraft:carrots", &[("age", "7")]);
    }

    /// Departure: a rabbit that searches with a ripe carrot near finds it and starts (witness: it runs and targets that
    /// block); with no ripe carrot, it does not start and draws its next delay (100 to 200 asks).
    #[test]
    fn a_rabbit_starts_toward_a_ripe_carrot_and_not_otherwise() {
        let Some(mut h) = herd() else { return };
        h.set_block(5, 0, 0, "minecraft:farmland");
        h.set_block_state(5, 1, 0, "minecraft:carrots", &[("age", "3")]);
        let e = rabbit(&mut h);
        h.world.resource_mut::<Requests>().0.push(Req::Start(e, 0));
        for _ in 0..20 {
            h.tick();
        }
        assert!(!running(&h, e), "an unripe carrot is no target");
        let next = h.world.get::<Raid>(e).unwrap().next;
        assert!((100..=200).contains(&next), "the search ran and drew its delay: {next}");
    }
    /// With a ripe carrot: the rabbit starts (witness) and the block it targets is the ripe one.
    #[test]
    fn a_rabbit_with_a_ripe_carrot_starts_toward_it() {
        let Some(mut h) = herd() else { return };
        ripe(&mut h);
        let e = rabbit(&mut h);
        h.tick();
        h.world.resource_mut::<Requests>().0.push(Req::Start(e, 0));
        h.tick();
        h.tick();
        assert!(h.world.get::<Raid>(e).unwrap().can_raid, "witness: the search found the ripe carrot");
        assert_eq!(h.world.get::<Raid>(e).unwrap().block.block, [5, 0, 0], "the target is the farmland under it");
    }
}
