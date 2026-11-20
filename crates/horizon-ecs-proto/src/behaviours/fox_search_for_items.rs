//! `Fox.FoxSearchForItemsGoal` (`mobs/fox.rs:1662-1700`, the point: `game/mobs/foxes.rs:58-75`), fiche 39: a MOVE goal. A fox with an
//! empty mouth, no target, no recent hurt and no sleep, sitting or faceplant walks to the first pickable item of its box grown by
//! 8. The start is a draw of `nextInt(reduced(10)) == 0` per ask, every 2 ticks: one deadline, `2 x geometric`, instead of a draw
//! per tick. While it runs, each selector tick asks again: it stops when the mouth fills, when the item is gone, or when its
//! draw says so (`can_continue` is `can_use`: it stops with probability 4/5 per ask).
use super::fox_state::{self as fs, FACEPLANTED, SITTING, SLEEPING};
use super::fox_pick_up_loot;
use super::*;
use crate::cow::*;
use crate::goals::*;
use crate::item;
use crate::targeting::{self, Target, Who};
use horizon_core::geometry::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub speed: f64,
    /// The box of the fox grown by this much looks for the item.
    pub scan: f64,
    /// `ALLOWED`: the Mob draws `nextInt(reduced(asks))`.
    pub asks: i32,
}
impl Params {
    pub const DEFAULT: Self = Self { speed: 1.2_f32 as f64, scan: 8.0, asks: 10 };
}

/// The goal's data: its rank, its speed and scan, `odds` = `reduced(asks)`. `pending`: a deadline of this goal is in the wheel
/// (the wheel does not replace one: a second would fire twice). `redraw`: the deadline was the end of the memory of a hurt,
/// so the next one is a fresh draw, not a start.
#[derive(Component, Clone, Copy, Debug)]
pub struct Seek {
    pub slot: u8,
    pub speed: f64,
    pub scan: f64,
    pub odds: i32,
    pub pending: bool,
    pub redraw: bool,
}
/// The goal runs: the own time it ends at (the draws that stop it, pre-drawn).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Seeking {
    pub until: u64,
}

pub static MODULE: Module = Module {
    name: "fox_search_for_items",
    flags: MOVE,
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
    fs::ensure(w, e);
    let p = *params::<Params>(p);
    w.entity_mut(e).insert(Seek { slot: inst.slot, speed: p.speed, scan: p.scan, odds: reduced(p.asks), pending: false, redraw: false });
}
/// `reduced(n) = (n + 1) / 2` (`Goal.reducedTickDelay`).
fn reduced(n: i32) -> i32 {
    (n + 1) / 2
}
fn arm_first(w: &mut World, e: Entity, _: Inst) {
    arm_draw(w, e);
}
fn set_pending(w: &mut World, e: Entity, on: bool) {
    if let Some(mut s) = w.get_mut::<Seek>(e) {
        s.pending = on;
    }
}
/// The next draw that can start the goal: `2 x geometric(odds)` ticks from now, unless a deadline is already in the wheel.
fn arm_draw(w: &mut World, e: Entity) {
    let Some(s) = w.get::<Seek>(e).copied() else { return };
    if s.pending || w.get::<Seeking>(e).is_some() {
        return;
    }
    let wait = with_rng(w, e, |r| 2 * geometric(r, s.odds));
    set_pending(w, e, true);
    crate::goals::arm(w, e, s.slot, wait);
}
/// A deadline came up: it is no longer pending. True = ask `canUse` (only while the goal does not run).
fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    set_pending(w, e, false);
    w.get::<Seeking>(e).is_none()
}

enum Gate {
    Open,
    /// The mouth holds something: nothing to do until it is empty (the wake comes from whoever empties it).
    Hand,
    Target,
    /// Hurt recently by a living attacker: the memory ends at this own time.
    Hurt(u64),
    /// Asleep, sitting or faceplanted: a state that ends by itself, so the draws go on.
    Moving,
}
/// `canUse`'s early refusals, in the Mob's order: the mouth, the target, the last attacker (`base_tick` forgets it after 100 ticks
/// or when the attacker is dead), `canMove`.
fn gate(w: &World, e: Entity, now: u64) -> Gate {
    if main_hand(w, e).is_some() {
        return Gate::Hand;
    }
    if w.get::<Target>(e).is_some() {
        return Gate::Target;
    }
    if let Some(h) = w.get::<LastHurtBy>(e)
        && now <= h.at + 100
        && attacker_alive(w, h.who)
    {
        return Gate::Hurt(h.at + 101);
    }
    if fs::has(w, e, SLEEPING | SITTING | FACEPLANTED) {
        return Gate::Moving;
    }
    Gate::Open
}
/// `fox_state::set` cleared sleeping, sitting or faceplanted: the draws go on from now (the `Mob` asks at every selector tick, so
/// the first ask after the wake draws with the same odds).
pub fn flags_cleared(w: &mut World, e: Entity) {
    if w.get::<Seek>(e).is_some() && w.get::<Seeking>(e).is_none() {
        arm_draw(w, e);
    }
}
fn attacker_alive(w: &World, who: Who) -> bool {
    match who {
        Who::Entity(c) => w.get::<Health>(c).is_some_and(|h| h.0 > 0.0),
        Who::Player(_) => targeting::info(w, who).is_some(),
    }
}
/// The first pickable item of the box grown by `scan` (`Fox.ALLOWED_ITEMS` order: the order they joined).
fn point(w: &World, e: Entity, scan: f64) -> Option<Entity> {
    let b = w.get::<Bounds>(e)?.0;
    let area = b.inflate(Vec3::new(scan, scan, scan));
    item::items_in(w, area).into_iter().find(|&it| item::pickable(w, it))
}
/// `canUse`: the deadline came up. A refused gate draws nothing; a hurt memory arms its end; a fox with no point draws again.
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let now = now_of(w, e);
    let s = *w.get::<Seek>(e).unwrap();
    if s.redraw {
        if let Some(mut m) = w.get_mut::<Seek>(e) {
            m.redraw = false;
        }
        arm_draw(w, e);
        return false;
    }
    match gate(w, e, now) {
        Gate::Open => {}
        Gate::Hurt(until) => {
            if let Some(mut m) = w.get_mut::<Seek>(e) {
                m.redraw = true;
            }
            set_pending(w, e, true);
            crate::goals::arm(w, e, s.slot, (until - now).max(1) as i32);
            return false;
        }
        // A state that ends by an event (`fox_state::set` clears the flag and calls `flags_cleared`): no periodic draw meanwhile.
        Gate::Moving => return false,
        Gate::Hand | Gate::Target => return false,
    }
    if point(w, e, s.scan).is_some() {
        true
    } else if w.resource::<item::ItemIndex>().is_empty() {
        // No item lies anywhere: no draw can start the search until one lands (`Item`'s `on_add` wakes it, `wake_parked`).
        w.entity_mut(e).insert(Idle);
        false
    } else {
        arm_draw(w, e);
        false
    }
}
/// The search sleeps: no item exists.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
struct Idle;
/// An item joined the ground: the sleeping searches draw again.
pub fn wake_parked(w: &mut World) {
    let es: Vec<Entity> = w.query_filtered::<Entity, With<Idle>>().iter(w).collect();
    for e in es {
        w.entity_mut(e).remove::<Idle>();
        arm_draw(w, e);
    }
}
/// `start`: the walk to the point, and the own time the goal stops at. Each ask that the Mob makes while the goal runs continues
/// with probability 1/5, so the stop is `2 x (k + 1)` ticks where `k` counts the continues (P(k) = (1/5)^k x 4/5).
fn start(w: &mut World, e: Entity, _: Inst) {
    let s = *w.get::<Seek>(e).unwrap();
    let Some(it) = point(w, e, s.scan) else { return };
    let k = with_rng(w, e, |r| {
        let u = r.next_f64().max(1.0e-12);
        (u.ln() / 0.2_f64.ln()).floor() as u64
    });
    let now = now_of(w, e);
    w.entity_mut(e).insert(Seeking { until: now + 2 * (k + 1) });
    let p = w.get::<Pos>(it).unwrap().0;
    go_to_at(w, e, [p.x, p.y, p.z], s.speed, 0);
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Seeking>();
    arm_draw(w, e);
}
/// The tick of the module, run only while a fox that searches lives (no registered system).
fn tick(w: &mut World) {
    seek_run(w);
    crate::goals::tick_state(w, target_lost);
}
/// A target was lost: the draws that the target stopped start again (a fresh one, unless a deadline is pending).
fn target_lost(ev: Res<Events>, mut reqs: ResMut<Requests>, q: Query<(), With<Seek>>) {
    if q.is_empty() {
        return;
    }
    for e in ev.target_lost.iter().filter(|&e| q.contains(e)) {
        reqs.0.push(Req::Call(e, arm_draw));
    }
}
/// Every running search, on the selector ticks: the gates and the own-time end, then the point, then the walk to it. The
/// fox looks at its items too (the looting rule, in the same tick).
fn seek_run(w: &mut World) {
    let list = crate::goals::entities::<With<Seeking>>(w);
    for e in list {
        if !fs::selector_tick(w, e) {
            continue;
        }
        let Some(slot) = w.get::<Seek>(e).map(|s| s.slot) else { continue };
        let now = now_of(w, e);
        let until = w.get::<Seeking>(e).unwrap().until;
        fox_pick_up_loot::look(w, e);
        if now >= until || !matches!(gate(w, e, now), Gate::Open) {
            end_goal(w, e, slot);
            continue;
        }
        let scan = w.get::<Seek>(e).unwrap().scan;
        match point(w, e, scan) {
            Some(it) => {
                let p = w.get::<Pos>(it).unwrap().0;
                let speed = w.get::<Seek>(e).unwrap().speed;
                go_to_at(w, e, [p.x, p.y, p.z], speed, 0);
            }
            None => end_goal(w, e, slot),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::species::Config;
    use horizon_client_lib::items::ItemStack;

    static ENTRIES: [Entry; 1] = [Entry::new(&MODULE, 11, &Params::DEFAULT)];

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
    fn berry(h: &mut Herd) {
        item::drop_item(&mut h.world, ItemStack::new("minecraft:sweet_berries", 1), Vec3::new(6.5, 1.0, 0.5), Vec3::ZERO, 0);
        h.world.flush();
    }

    /// A fox with an empty mouth and a berry six blocks away starts to search within 600 ticks (witness: it does).
    #[test]
    fn an_empty_mouth_starts_the_search_for_an_item_in_reach() {
        let Some(mut h) = herd() else { return };
        let e = fox(&mut h);
        berry(&mut h);
        let mut started = false;
        for _ in 0..600 {
            h.tick();
            started |= running(&h, e);
        }
        assert!(started, "a fox with an empty mouth walks to a berry in reach");
    }

    /// A fox with a full mouth never starts; the same setup with an empty mouth does (the witness is the empty fox).
    #[test]
    fn a_full_mouth_does_not_search() {
        let Some(mut h) = herd() else { return };
        let full = fox(&mut h);
        set_main_hand(&mut h.world, full, Some(ItemStack::new("minecraft:stick", 1)));
        berry(&mut h);
        let mut started = false;
        for _ in 0..600 {
            h.tick();
            started |= running(&h, full);
        }
        assert!(!started, "a full mouth does not start the search");
    }

    /// A running search ends when the mouth fills (witness: it was running before).
    #[test]
    fn a_running_search_stops_when_the_mouth_fills() {
        let Some(mut h) = herd() else { return };
        let e = fox(&mut h);
        berry(&mut h);
        let mut started = false;
        for _ in 0..600 {
            h.tick();
            if running(&h, e) {
                started = true;
                break;
            }
        }
        assert!(started, "the search started");
        set_main_hand(&mut h.world, e, Some(ItemStack::new("minecraft:sweet_berries", 1)));
        for _ in 0..10 {
            h.tick();
        }
        assert!(!running(&h, e), "a full mouth ends the search");
    }
}
