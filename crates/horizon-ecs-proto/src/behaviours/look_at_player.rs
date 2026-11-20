//! `LookAtPlayerGoal`: now and then (one chance in `period` per ask, one ask every 2 ticks) look at the nearest player
//! within range for 40 to 80 ticks (with `species`: the nearest creature of those species within sight, `LookAtPlayerGoal(Class, range)`).
//! The ask is a deadline of the wheel.
use super::*;
use crate::{cow::*, goals::*, species::SpeciesSet, targeting::{Eye, Who}};
use horizon_core::geometry::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub range: f64,
    pub period: i32,
    /// Empty: players. Else: the nearest creature of these species (not itself) in the box inflated by (range, 3, range).
    pub species: &'static [&'static str],
}
impl Params {
    pub const DEFAULT: Self = Self { range: 6.0, period: 50, species: &[] };
}
/// `LookAtPlayerGoal`'s range and ask period; `armed`: its deadline is in the wheel; `pick`: the player `can_use` chose.
#[derive(Component, Clone, Copy, Debug)]
pub struct Glance {
    pub range: f64,
    pub period: i32,
    pub armed: bool,
    pub slot: u8,
    pick: Who,
    species: SpeciesSet,
}
/// The second instance of the animal (`Inst::nth` 1: the strider looks at players at 8 and at striders at 9); the first is `Glance`, so an
/// animal with one look carries nothing more than before.
#[derive(Component, Clone, Copy, Debug)]
pub struct Glance2(pub Glance);
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Glancing {
    pub who: Who,
    pub until: u64,
    pub nth: u8,
}
fn get(w: &World, e: Entity, inst: Inst) -> Option<Glance> {
    if inst.nth == 0 { w.get::<Glance>(e).copied() } else { w.get::<Glance2>(e).map(|g| g.0) }
}
fn with<R>(w: &mut World, e: Entity, inst: Inst, f: impl FnOnce(&mut Glance) -> R) -> Option<R> {
    if inst.nth == 0 { w.get_mut::<Glance>(e).map(|mut g| f(&mut g)) } else { w.get_mut::<Glance2>(e).map(|mut g| f(&mut g.0)) }
}

pub static MODULE: Module = Module {
    name: "look_at_player",
    flags: LOOK,
    blocked: Some(blocked),
    equip,
    arm,
    fired,
    can_use,
    start,
    stop,
    systems,
    requires: &[Cap::Body, Cap::Head],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, inst: Inst, p: &(dyn std::any::Any + Sync)) {
    let p = params::<Params>(p);
    let species = SpeciesSet::of(p.species.iter().filter_map(|n| crate::species::registry().id(n)).map(|i| i as usize));
    let g = Glance { range: p.range, period: p.period, armed: false, slot: inst.slot, pick: Who::Player(0), species };
    if inst.nth == 0 {
        w.entity_mut(e).insert(g);
    } else {
        w.entity_mut(e).insert(Glance2(g));
    }
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems((lazy_wake, glance_run).chain().in_set(slot));
}
/// Arms the ask of an animal that has none and that a player can see.
fn arm(w: &mut World, e: Entity, inst: Inst) {
    let g = get(w, e, inst).unwrap();
    let p = w.get::<Pos>(e).unwrap().0;
    if !g.armed && w.resource::<LazyLook>().watched(p.x, p.z) {
        let wait = with_rng(w, e, |r| 2 * geometric(r, g.period));
        crate::goals::arm(w, e, inst.slot, wait);
        with(w, e, inst, |g| g.armed = true);
    }
}
/// The ask was refused: the next one is armed.
fn rearm(w: &mut World, e: Entity, inst: Inst) {
    with(w, e, inst, |g| g.armed = false);
    arm(w, e, inst);
}
fn blocked(w: &mut World, e: Entity, inst: Inst) {
    rearm(w, e, inst);
}
fn fired(w: &mut World, e: Entity, inst: Inst) -> bool {
    let Some(g) = with(w, e, inst, |g| {
        g.armed = false;
        *g
    }) else {
        return false;
    };
    // With the lazy look, a timer that comes up for an animal no player sees lapses (nothing re-arms it until a
    // player's range reaches it again).
    let p = w.get::<Pos>(e).unwrap().0;
    if !w.resource::<LazyLook>().watched(p.x, p.z) {
        return false;
    }
    // Nobody within the range of the look: the ask fails; the next one is armed without the selector. (Creatures: `can_use` searches.)
    if !g.species.is_empty() {
        return true;
    }
    let mut any = false;
    w.resource::<Online>().for_near(p.x, p.z, g.range, |q| any |= (q.x - p.x).powi(2) + (q.z - p.z).powi(2) <= g.range * g.range);
    if !any {
        let wait = with_rng(w, e, |r| 2 * geometric(r, g.period));
        crate::goals::arm(w, e, inst.slot, wait);
        with(w, e, inst, |g| g.armed = true);
        return false;
    }
    true
}
fn can_use(w: &mut World, e: Entity, inst: Inst) -> bool {
    let Some(pos) = w.get::<Pos>(e).map(|p| p.0) else { return false };
    let Some(g) = get(w, e, inst) else { return false };
    let mut best: Option<(Who, f64)> = None;
    if g.species.is_empty() {
        w.resource::<Online>().for_near(pos.x, pos.z, g.range, |p| {
            let d = d2(pos, Vec3::new(p.x, p.y, p.z));
            if d <= g.range * g.range && best.is_none_or(|b| d < b.1) {
                best = Some((Who::Player(p.id), d));
            }
        });
    } else {
        // `level.getNearestEntity(entities in the box inflated by (range, 3, range))`: the nearest by the eyes, in sight, within
        // `max(range, 2)` of the feet.
        let eye = w.get::<Eye>(e).map_or(1.0, |l| l.0);
        let from = Vec3::new(pos.x, pos.y + eye, pos.z);
        let reach = g.range.max(2.0);
        let mw = w.resource::<Level>().mw.0;
        let Some(area) = w.get::<Bounds>(e).map(|b| b.0.inflate(Vec3::new(g.range, 3.0, g.range))) else { return false };
        w.resource::<Index>().for_radius(pos.x, pos.z, g.range, |c| {
            let (Some(cp), Some(cs)) = (w.get::<Pos>(c), w.get::<SpeciesId>(c)) else { return };
            if c == e || !g.species.contains(cs.0) || d2(pos, cp.0) > reach * reach || !w.get::<Bounds>(c).is_some_and(|b| b.0.intersects(area)) {
                return;
            }
            let ceye = w.get::<Eye>(c).map_or(1.0, |l| l.0);
            let at = Vec3::new(cp.0.x, cp.0.y + ceye, cp.0.z);
            let d = d2(from, at);
            if best.is_none_or(|b| d < b.1) && mw.line_of_sight_for(Some(pos.y), from, at) {
                best = Some((Who::Entity(c), d));
            }
        });
    }
    match best {
        Some(b) => {
            with(w, e, inst, |g| g.pick = b.0);
            true
        }
        None => {
            rearm(w, e, inst);
            false
        }
    }
}
fn start(w: &mut World, e: Entity, inst: Inst) {
    let now = crate::cow::now_of(w, e);
    let id = get(w, e, inst).unwrap().pick;
    // `reducedTickDelay(40 + nextInt(40))` asks of 2 ticks.
    let n = with_rng(w, e, |r| r.next_int(40));
    w.entity_mut(e).insert(Glancing { who: id, nth: inst.nth, until: now + 2 * ((40 + n as u64 + 1) / 2) });
}
fn stop(w: &mut World, e: Entity, inst: Inst) {
    w.entity_mut(e).remove::<Glancing>();
    rearm(w, e, inst);
}
/// An animal that walks into a watched bin (the lazy look) takes its look timers again.
fn lazy_wake(moves: Res<CellMoves>, lazy: Res<LazyLook>, q: Query<(&Pos, &Glance, Option<&Glance2>, Option<&Scan>)>, mut reqs: ResMut<Requests>) {
    if !lazy.enabled {
        return;
    }
    for &m in &moves.0 {
        if let Ok((p, g, g2, s)) = q.get(m)
            && (!g.armed || g2.is_some_and(|g| !g.0.armed) || s.is_some_and(|s| !s.armed))
            && lazy.watched(p.0.x, p.0.z)
        {
            reqs.0.push(Req::Arm(m));
        }
    }
}
/// `LookAtPlayerGoal.canContinue` and `tick`.
fn glance_run(
    clock: Res<Clock>,
    on: Res<Online>,
    others: Query<(&Pos, &Eye)>,
    mut q: Query<(Aim, &Glancing, &Glance, Option<&Glance2>)>,
    mut reqs: ResMut<Requests>,
    mut cmd: Commands,
) {
    for ((e, pos, mut look, looking, _, _), g, g1, g2) in &mut q {
        let data = if g.nth == 0 { g1 } else { &g2.unwrap().0 };
        // The feet (the range test) and the point looked at.
        let t = match g.who {
            Who::Player(id) => on.get(id).map(|p| (Vec3::new(p.x, p.y, p.z), [p.x, p.eye(), p.z])),
            Who::Entity(c) => others.get(c).ok().map(|(p, l)| (p.0, [p.0.x, p.0.y + l.0, p.0.z])),
        };
        let at = t.filter(|t| d2(t.0, pos.0) <= data.range * data.range).map(|t| t.1);
        match at {
            Some(a) if clock.of(e) < g.until => aim(&mut cmd, e, &mut look, looking, a, None),
            _ => reqs.0.push(Req::End(e, data.slot)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cow::{species, spawn_species, Herd};
    use crate::species::Config;

    /// With `species` the look takes the nearest creature of that species (here the same one as the looker), never a player; without
    /// it a creature near does not count and the player does.
    #[test]
    fn the_species_option_looks_at_creatures_not_players() {
        if crate::cow::skip_unless_walker("look at species") {
            return;
        }
        // The process species is set by the first world built.
        drop(Herd::with_slab(60, true, false));
        let own: &'static [&'static str] = Box::leak(vec![species().name].into_boxed_slice());
        let others: &'static Params = Box::leak(Box::new(Params { species: own, period: 1, ..Params::DEFAULT }));
        static PLAYERS: Params = Params { period: 1, ..Params::DEFAULT };
        for (p, who_expected) in [(others, true), (&PLAYERS, false)] {
            let mut h = Herd::with_slab(60, true, false);
            let mut sp = *species();
            sp.goals = Config { entries: Box::leak(vec![Entry::new(&MODULE, 1, p)].into_boxed_slice()) };
            let a = spawn_species(&mut h.world, &sp, 0.5, 0.5, 80);
            let b = spawn_species(&mut h.world, &sp, 3.5, 0.5, 81);
            h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5, y: 1.0, z: 3.5, main: 0, off: 0 });
            let mut seen = None;
            for _ in 0..400 {
                h.tick();
                if let Some(g) = h.world.get::<Glancing>(a) {
                    seen = Some(g.who);
                    break;
                }
            }
            let _ = b;
            match seen {
                Some(Who::Entity(_)) => assert!(who_expected, "a creature was looked at without the option"),
                Some(Who::Player(1)) => assert!(!who_expected, "a player was looked at with the option"),
                other => panic!("no look started: {other:?}"),
            }
        }
    }
}
