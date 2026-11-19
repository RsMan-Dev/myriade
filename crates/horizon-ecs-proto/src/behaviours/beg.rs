//! `Wolf.BegGoal` (`mobs/tame.rs:610-668`): a wolf that sees a living, non-spectator player within 8 blocks holding a bone or
//! one of its foods tilts its head at him for `reduced(asks + nextInt(asks))` ticks (flag LOOK). Woken by the players' changes
//! only (`Online.seen`); nothing polls, and the goal has no timer of its own.
use super::*;
use crate::hostile::PlayerState;
use crate::{cow::*, goals::*};
use horizon_core::geometry::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The foods of the animal, by item name (the bone is always interesting).
    pub foods: &'static [&'static str],
    /// `d^2 <= 64`: the squared range of the player (at most 8 blocks, the wake-up's radius).
    pub range_sqr: f64,
    /// The duration: `reducedTickDelay(asks + nextInt(asks))` ticks.
    pub asks: i32,
}
impl Params {
    pub const DEFAULT: Self = Self { foods: &[], range_sqr: 64.0, asks: 40 };
}
/// The wake-up radius of the players' changes: the widest `range_sqr` a configuration may give.
const REACH: f64 = 8.0;

/// The state of the animal's beg goal: what it finds interesting (a bit per item id), its range, its duration parameter,
/// and the player `can_use` chose for `start`.
#[derive(Component, Clone, Copy, Debug)]
pub struct Begger {
    pub slot: u8,
    pub items: u32,
    pub range_sqr: f64,
    pub asks: i32,
    pick: u32,
}
/// The goal runs: the player it begs for and the ticks left.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Begging {
    pub player: u32,
    pub ticks: i32,
}

pub static MODULE: Module = Module {
    name: "beg",
    flags: LOOK,
    retry_on_end: true,
    equip,
    can_use,
    start,
    stop,
    systems,
    requires: &[Cap::Goals, Cap::Head],
    ..Module::DEFAULT
};

fn equip(w: &mut World, e: Entity, inst: Inst, p: &(dyn std::any::Any + Sync)) {
    let p = params::<Params>(p);
    let items = std::iter::once("minecraft:bone").chain(p.foods.iter().copied()).fold(0u32, |m, n| m | 1 << item_id(n));
    w.entity_mut(e).insert(Begger { slot: inst.slot, items, range_sqr: p.range_sqr, asks: p.asks, pick: 0 });
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems((wake_on_player, beg_run).chain().in_set(slot));
}
fn d3(a: Vec3, p: &Player) -> f64 {
    (p.x - a.x).powi(2) + (p.y - a.y).powi(2) + (p.z - a.z).powi(2)
}
/// `BegGoal.canUse`: alive, not a spectator, holding something interesting, within the range.
fn wanted(ps: &PlayerState, p: &Player, b: &Begger, at: Vec3) -> bool {
    ps.alive(p.id) && !ps.spectator(p.id) && p.holds_any(b.items) && d3(at, p) <= b.range_sqr
}
/// A player took an item, put it away, came or went: the wolves he newly draws start their beg (those that already saw him
/// holding it do not start again).
fn wake_on_player(on: Res<Online>, idx: Res<Index>, ps: Res<PlayerState>, q: Query<(&Pos, &Begger)>, any: Query<(), With<Begger>>, mut reqs: ResMut<Requests>) {
    if any.is_empty() {
        return;
    }
    for &(old, new) in &on.seen {
        let Some(n) = new else { continue };
        idx.for_radius(n.x, n.z, REACH, |c| {
            let Ok((pos, b)) = q.get(c) else { return };
            let was = old.is_some_and(|o| wanted(&ps, &o, b, pos.0));
            if wanted(&ps, &n, b, pos.0) && !was {
                reqs.0.push(Req::Start(c, b.slot));
            }
        });
    }
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let Some(b) = w.get::<Begger>(e).copied() else { return false };
    let Some(pos) = w.get::<Pos>(e).map(|p| p.0) else { return false };
    let ps = w.resource::<PlayerState>();
    let mut best: Option<(u32, f64)> = None;
    w.resource::<Online>().for_near(pos.x, pos.z, b.range_sqr.sqrt(), |p| {
        let d = d3(pos, &p);
        if wanted(ps, &p, &b, pos) && best.is_none_or(|x| d < x.1) {
            best = Some((p.id, d));
        }
    });
    let Some((id, _)) = best else { return false };
    w.get_mut::<Begger>(e).unwrap().pick = id;
    true
}
/// `reducedTickDelay(asks + nextInt(asks))` ticks, the first of them counted in the tick the goal starts.
fn start(w: &mut World, e: Entity, _: Inst) {
    let b = *w.get::<Begger>(e).unwrap();
    let n = with_rng(w, e, |r| r.next_int(b.asks));
    w.entity_mut(e).insert(Begging { player: b.pick, ticks: (b.asks + n + 1) / 2 });
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Begging>();
}
/// `BegGoal.canContinue` and `tick`: while the player still holds the item within the range and the time is not up, look at
/// his eyes (the head turns 10 degrees per tick) and count one tick; otherwise the goal ends.
fn beg_run(
    on: Res<Online>,
    ps: Res<PlayerState>,
    mut q: Query<(Aim, &mut Begging, &Begger)>,
    mut reqs: ResMut<Requests>,
    mut cmd: Commands,
) {
    for ((e, pos, mut look, looking, _, _), mut g, b) in &mut q {
        let p = on.get(g.player).filter(|p| ps.alive(p.id) && p.holds_any(b.items) && d3(pos.0, p) <= b.range_sqr);
        match p {
            Some(p) if g.ticks > 0 => {
                aim(&mut cmd, e, &mut look, looking, [p.x, p.eye(), p.z], Some(10.0));
                g.ticks -= 1;
            }
            _ => reqs.0.push(Req::End(e, b.slot)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::species::Config;

    static BEG: Params = Params { ..Params::DEFAULT };
    static ENTRIES: [Entry; 1] = [Entry::new(&MODULE, 9, &BEG)];

    fn herd_with(entries: &'static [Entry]) -> (Herd, Entity) {
        let mut h = Herd::with_slab(20, false, false);
        let mut sp = *species();
        sp.goals = Config { entries };
        let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 7);
        for _ in 0..20 {
            h.tick();
        }
        (h, e)
    }
    /// A player at `x`, holding `main` (0: nothing).
    fn put_player(h: &mut Herd, x: f64, main: u32) {
        h.world.resource_mut::<Online>().set(Player { id: 1, x, y: 1.0, z: 0.5, main, off: 0 });
    }
    fn bone() -> u32 {
        item_id("minecraft:bone")
    }

    #[test]
    fn begs_when_a_player_near_holds_a_bone() {
        let (mut h, e) = herd_with(&ENTRIES);
        put_player(&mut h, 5.5, bone());
        for _ in 0..5 {
            h.tick();
        }
        let b = h.world.get::<Begging>(e).copied().expect("no beg from a player 5 blocks away holding a bone");
        // Drawn 20 to 39 at the start, then one tick is counted per tick of the goal: at most 39 remain, and the goal is
        // still running after a few ticks.
        assert!((10..=39).contains(&b.ticks), "the duration left: {}", b.ticks);
    }

    #[test]
    fn does_not_beg_without_a_bone_and_begs_once_one_is_held() {
        let (mut h, e) = herd_with(&ENTRIES);
        put_player(&mut h, 5.5, 0);
        for _ in 0..30 {
            h.tick();
        }
        assert!(h.world.get::<Begging>(e).is_none(), "a beg from a player holding nothing");
        put_player(&mut h, 5.5, bone());
        for _ in 0..5 {
            h.tick();
        }
        assert!(h.world.get::<Begging>(e).is_some(), "no beg once the player holds a bone");
    }

    #[test]
    fn stops_when_the_player_puts_the_bone_away() {
        let (mut h, e) = herd_with(&ENTRIES);
        put_player(&mut h, 5.5, bone());
        for _ in 0..5 {
            h.tick();
        }
        assert!(h.world.get::<Begging>(e).is_some(), "no beg to begin with");
        put_player(&mut h, 5.5, 0);
        for _ in 0..5 {
            h.tick();
        }
        assert!(h.world.get::<Begging>(e).is_none(), "the beg went on after the bone was put away");
    }

    #[test]
    fn does_not_beg_from_beyond_eight_blocks() {
        let (mut h, e) = herd_with(&ENTRIES);
        put_player(&mut h, 9.5, bone());
        for _ in 0..30 {
            h.tick();
        }
        assert!(h.world.get::<Begging>(e).is_none(), "a beg from a player 9 blocks away");
    }
}
