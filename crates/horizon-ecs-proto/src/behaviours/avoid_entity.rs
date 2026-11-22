//! `AvoidEntityGoal`: an animal that sees a player (or an animal of a listed species) within `max_dist` runs to a random
//! spot away from it, walking, or sprinting while the target is within 7 blocks. Woken by events only: a player that comes
//! within range, an animal of a listed species that changes column of the index, and the animal's own column change.
use super::*;
use crate::rules::avoid_entity::{in_reach, run_speed};
use crate::{cow::*, goals::*};
use horizon_core::geometry::Vec3;

/// The range of the index scan: the widest `max_dist` a configuration may give (`Params` checks it).
const REACH: f64 = 24.0;

/// What the goal runs from: the nearest player, or the nearest animal of the listed species (`Species::id`).
#[derive(Clone, Copy)]
pub enum Avoid {
    Player,
    Species(&'static [u8]),
    /// Species by name (resolved once, at creation: the ids are the registry's order).
    Named(&'static [&'static str]),
    /// Every monster: the species whose type is in the `monster` category (`Avoid::Species(monsters)` of the `Mob`).
    Monsters,
}
#[derive(Clone, Copy)]
pub struct Params {
    pub what: Avoid,
    /// The range of the target, in blocks.
    pub max_dist: f64,
    pub walk: f64,
    pub sprint: f64,
    /// The animal's own condition (`when`).
    pub avoider_when: Option<fn(&World, Entity) -> bool>,
    /// The player's condition (`player_when`), for `Avoid::Player`.
    pub player_when: Option<fn(&crate::hostile::PlayerState, &Player) -> bool>,
    /// The listed animal's condition (`seen_when`), for `Avoid::Species`.
    pub target_when: Option<fn(&World, Entity) -> bool>,
    /// A condition that draws (`WolfAvoidEntityGoal`: the llama's strength against `nextInt(5)`): the uniform draw in [0, 1) is the third argument.
    pub target_roll: Option<fn(&World, Entity, f64) -> bool>,
}
impl Params {
    pub const DEFAULT: Self = Self { what: Avoid::Player, max_dist: 6.0, walk: 1.0, sprint: 1.2, avoider_when: None, player_when: None, target_when: None, target_roll: None };
}
/// One instance of the goal: its parameters, and what `can_use` chose for `start` (the point run from, the player if
/// one is the target, the spot to walk to).
#[derive(Clone, Copy)]
pub struct Avoider {
    what: Avoid,
    max_dist: f64,
    walk: f64,
    sprint: f64,
    avoider_when: Option<fn(&World, Entity) -> bool>,
    player_when: Option<fn(&crate::hostile::PlayerState, &Player) -> bool>,
    target_when: Option<fn(&World, Entity) -> bool>,
    target_roll: Option<fn(&World, Entity, f64) -> bool>,
    /// The species the goal avoids.
    mask: crate::species::SpeciesSet,
    pub slot: u8,
    from: Vec3,
    player: u32,
    spot: (f64, f64, f64),
}
/// The avoid goals of the animal, by `Inst::nth`.
#[derive(Component, Clone)]
pub struct Avoiders(pub Vec<Avoider>);
/// The animal has an avoid goal whose target is a listed species: the moves of the other animals matter to it (the
/// systems do nothing, not even scan the index, in a world with none).
#[derive(Component, Clone, Copy)]
#[component(storage = "SparseSet", on_insert = crate::cow::watch_in::<{ crate::cow::WATCH_AVOID }>, on_discard = crate::cow::watch_out::<{ crate::cow::WATCH_AVOID }>)]
pub struct AvoidsAnimals;
/// An avoid goal runs: which instance (`nth`).
#[derive(Component, Clone, Copy)]
#[component(storage = "SparseSet")]
pub struct Avoiding(pub u8);

pub static MODULE: Module = Module {
    name: "avoid_entity",
    flags: MOVE,
    ends_with_path: true,
    retry_on_end: true,
    equip,
    can_use,
    start,
    stop,
    systems,
    requires: &[Cap::Body, Cap::Goals, Cap::Locomotion],
    ..Module::DEFAULT
};
/// The union of the species any avoid goal of the process lists (a superset is harmless): the move of an animal of another species
/// matters to nobody, and its column change does not scan the index.
static LISTED_ANYWHERE: std::sync::OnceLock<Box<[std::sync::atomic::AtomicU64]>> = std::sync::OnceLock::new();
fn listed_anywhere() -> &'static [std::sync::atomic::AtomicU64] {
    LISTED_ANYWHERE.get_or_init(|| (0..crate::species::registry().species.len().div_ceil(64).max(1)).map(|_| std::sync::atomic::AtomicU64::new(0)).collect())
}
/// The species a `what` names.
pub(crate) fn species_mask(what: Avoid) -> crate::species::SpeciesSet {
    use crate::species::SpeciesSet;
    let reg = crate::species::registry();
    match what {
        Avoid::Player => SpeciesSet::empty(),
        Avoid::Species(ids) => SpeciesSet::of(ids.iter().map(|&i| i as usize)),
        Avoid::Named(names) => SpeciesSet::of(reg.species.iter().enumerate().filter(|(_, s)| names.contains(&s.name)).map(|(i, _)| i)),
        Avoid::Monsters => SpeciesSet::of(reg.species.iter().enumerate().filter(|(_, s)| crate::cow::is_monster(s.name)).map(|(i, _)| i)),
    }
}
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = params::<Params>(p);
    debug_assert!(p.max_dist <= REACH, "the index scan does not reach {} blocks", p.max_dist);
    let a = Avoider {
        what: p.what,
        max_dist: p.max_dist,
        walk: p.walk,
        sprint: p.sprint,
        avoider_when: p.avoider_when,
        player_when: p.player_when,
        target_when: p.target_when,
        target_roll: p.target_roll,
        mask: species_mask(p.what),
        slot: inst.slot,
        from: Vec3::ZERO,
        player: 0,
        spot: (0.0, 0.0, 0.0),
    };
    if !matches!(p.what, Avoid::Player) {
        for (i, w) in listed_anywhere().iter().enumerate() {
            let bits = (0..64).filter(|&b| i * 64 + b < 256 && a.mask.contains((i * 64 + b) as u8)).fold(0u64, |m, b| m | 1 << b);
            w.fetch_or(bits, std::sync::atomic::Ordering::Relaxed);
        }
        w.entity_mut(e).insert(AvoidsAnimals);
    }
    match w.get_mut::<Avoiders>(e) {
        Some(mut v) => {
            debug_assert_eq!(v.0.len(), inst.nth as usize);
            v.0.push(a);
        }
        None => {
            w.entity_mut(e).insert(Avoiders(vec![a]));
        }
    }
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    // Each wake-up system returns at once in a world with no avoider (a move of any animal would scan 24 blocks of index).
    s.add_systems((wake_on_player, cell_moves, avoid_run).chain().in_set(slot));
}
/// A player came within range (or was within it and moved out of it: no wake, the goal decides).
fn wake_on_player(on: Res<Online>, ps: Res<crate::hostile::PlayerState>, idx: Res<Index>, q: Query<(&Pos, &Avoiders)>, any: Query<(), With<Avoiders>>, mut reqs: ResMut<Requests>) {
    if any.is_empty() {
        return;
    }
    for &(old, new) in &on.seen {
        let Some(n) = new else { continue };
        let at = Vec3::new(n.x, n.y, n.z);
        idx.for_radius(n.x, n.z, REACH, |c| {
            let Ok((pos, avs)) = q.get(c) else { return };
            for a in avs.0.iter().filter(|a| matches!(a.what, Avoid::Player)) {
                let wanted = |p: Player| a.player_when.is_none_or(|f| f(&ps, &p));
                let was = old.is_some_and(|o| wanted(o) && in_reach(a.max_dist, pos.0, Vec3::new(o.x, o.y, o.z)));
                if wanted(n) && in_reach(a.max_dist, pos.0, at) && !was {
                    reqs.0.push(Req::Start(c, a.slot));
                }
            }
        });
    }
}
/// The animals that changed column: the animal itself (its goal looks again), and the avoiders within range of it when
/// its species is one they list.
fn cell_moves(
    moved: Res<CellMoves>,
    idx: Res<Index>,
    q: Query<(&Pos, &Avoiders)>,
    ids: Query<(&Pos, &SpeciesId)>,
    listing: Query<(), With<AvoidsAnimals>>,
    any: Query<(), With<Avoiders>>,
    mut reqs: ResMut<Requests>,
) {
    if any.is_empty() {
        return;
    }
    let listed = !listing.is_empty();
    let listed_mask = listed_anywhere();
    for &m in &moved.0 {
        if let Ok((_, avs)) = q.get(m) {
            reqs.0.extend(avs.0.iter().map(|a| Req::Start(m, a.slot)));
        }
        if !listed {
            continue;
        }
        let Ok((mp, sid)) = ids.get(m) else { continue };
        if listed_mask.get(sid.0 as usize / 64).is_none_or(|w| w.load(std::sync::atomic::Ordering::Relaxed) >> (sid.0 % 64) & 1 == 0) {
            continue;
        }
        let at = mp.0;
        if !idx.watched_near(crate::cow::WATCH_AVOID, idx.cell_of(at.x, at.z)) {
            continue;
        }
        idx.for_radius(at.x, at.z, REACH, |c| {
            let Ok((pos, avs)) = q.get(c) else { return };
            if c == m {
                return;
            }
            for a in &avs.0 {
                if !matches!(a.what, Avoid::Player)
                    && a.mask.contains(sid.0)
                    && in_reach(a.max_dist, pos.0, at)
                {
                    reqs.0.push(Req::Start(c, a.slot));
                }
            }
        });
    }
}
/// The tick of a running avoid goal: its speed, sprint within 7 blocks of the point it runs from (a player's position is
/// read now). Visits only the animals that run.
fn avoid_run(on: Res<Online>, mut q: Query<(&Pos, &Avoiding, &mut Avoiders, &mut Nav)>) {
    for (pos, run, mut avs, mut nav) in &mut q {
        let a = &mut avs.0[run.0 as usize];
        let from = if a.player != 0 { on.get(a.player).map_or(a.from, |p| Vec3::new(p.x, p.y, p.z)) } else { a.from };
        nav.speed = speed_at(a, d2(pos.0, from));
    }
}
/// `AvoidEntityGoal.avoidTarget`: the player (nearest, wanted and seen) or the first seen animal of a listed species, by
/// distance. Returns the point to run from and the player's id (0 for an animal).
fn avoid_target(w: &World, e: Entity, a: &Avoider, pos: Vec3, found: &mut Option<Entity>) -> Option<(Vec3, u32)> {
    let eye = w.get::<Look>(e).map_or(0.0, |l| l.eye);
    let from_eye = Vec3::new(pos.x, pos.y + eye, pos.z);
    let mw = w.resource::<Level>().mw.0;
    let sees = |to: Vec3| mw.line_of_sight_for(Some(pos.y), from_eye, to);
    match a.what {
        Avoid::Player => {
            let r = a.max_dist.max(2.0);
            let mut best: Option<(f64, Player)> = None;
            w.resource::<Online>().for_near(pos.x, pos.z, r, |p| {
                let d = d2(pos, Vec3::new(p.x, p.y, p.z));
                let near = (p.y - pos.y).abs() <= a.max_dist + 3.0 && d <= r * r;
                let wanted = a.player_when.is_none_or(|f| f(w.resource::<crate::hostile::PlayerState>(), &p));
                if near && wanted && best.is_none_or(|b| d < b.0) && sees(Vec3::new(p.x, p.eye(), p.z)) {
                    best = Some((d, p));
                }
            });
            best.map(|(_, p)| (Vec3::new(p.x, p.y, p.z), p.id))
        }
        Avoid::Species(_) | Avoid::Named(_) | Avoid::Monsters => {
            let mut c: Vec<(f64, Entity, Vec3)> = Vec::new();
            w.resource::<Index>().for_radius(pos.x, pos.z, a.max_dist, |t| {
                let (Some(tp), Some(sid)) = (w.get::<Pos>(t), w.get::<SpeciesId>(t)) else { return };
                let tp = tp.0;
                let boxed = t != e
                    && (tp.x - pos.x).abs() <= a.max_dist
                    && (tp.z - pos.z).abs() <= a.max_dist
                    && (tp.y - pos.y).abs() <= 3.0 + a.max_dist
                    && a.mask.contains(sid.0)
                    && a.target_when.is_none_or(|f| f(w, t));
                if boxed {
                    c.push((d2(pos, tp), t, tp));
                }
            });
            c.sort_by(|x, y| x.0.total_cmp(&y.0));
            let (_, t, tp) = c.into_iter().find(|&(_, t, tp)| {
                let eye_t = w.get::<Look>(t).map_or(0.0, |l| l.eye);
                sees(Vec3::new(tp.x, tp.y + eye_t, tp.z))
            })?;
            *found = Some(t);
            (d2(pos, tp) <= a.max_dist * a.max_dist).then_some((tp, 0))
        }
    }
}
/// `DefaultRandomPos.getPosAway(16, 7)` (the framework's position service: best of ten by the species' walk value).
fn random_spot(w: &mut World, e: Entity, _pos: Vec3, from: Vec3) -> Option<(f64, f64, f64)> {
    crate::spots::pick_for(w, e, crate::spots::Pick::DefaultAway { h: 16, v: 7, from: [from.x, from.y, from.z] }).map(|p| (p[0], p[1], p[2]))
}
fn can_use(w: &mut World, e: Entity, inst: Inst) -> bool {
    let nth = inst.nth as usize;
    let Some(a) = w.get::<Avoiders>(e).and_then(|v| v.0.get(nth).copied()) else { return false };
    let Some(pos) = w.get::<Pos>(e).map(|p| p.0) else { return false };
    if a.avoider_when.is_some_and(|f| !f(w, e)) {
        return false;
    }
    let mut found = None;
    let Some((from, player)) = avoid_target(w, e, &a, pos, &mut found) else { return false };
    // `target_roll`: a draw of the avoider against what it found (the wolf and the llama's strength), made once the target is there.
    if let (Some(roll), Some(t)) = (a.target_roll, found) {
        let u = with_rng(w, e, |r| r.next_f64());
        if !roll(w, t, u) {
            return false;
        }
    }
    // The target is there: when no spot is found, or the spot is not farther than the animal, `canUse` is asked again.
    let spot = take_spot(w, e, inst.slot, pos.y.floor()).map(|p| (p[0], p[1], p[2])).or_else(|| random_spot(w, e, pos, from));
    let Some((x, y, z)) = spot.filter(|&(x, y, z)| d2(Vec3::new(x, y, z), from) >= d2(pos, from)) else {
        arm(w, e, inst.slot, 2);
        return false;
    };
    let mut avs = w.get_mut::<Avoiders>(e).unwrap();
    let chosen = &mut avs.0[nth];
    chosen.from = from;
    chosen.player = player;
    chosen.spot = (x, y, z);
    true
}
fn speed_at(a: &Avoider, d2_to_target: f64) -> f64 {
    run_speed(a.sprint, a.walk, d2_to_target)
}
fn start(w: &mut World, e: Entity, inst: Inst) {
    let a = w.get::<Avoiders>(e).unwrap().0[inst.nth as usize];
    let pos = w.get::<Pos>(e).unwrap().0;
    let from = if a.player != 0 { w.resource::<Online>().get(a.player).map_or(a.from, |p| Vec3::new(p.x, p.y, p.z)) } else { a.from };
    w.entity_mut(e).insert(Avoiding(inst.nth));
    // The `Mob` ticks a goal in the tick it starts: the speed is the one of this distance already; `nav_move_to_path` of
    // `AvoidEntityGoal` asks a path to the block of the spot (reach 0).
    crate::goals::go_to_at(w, e, [a.spot.0, a.spot.1, a.spot.2], speed_at(&a, d2(pos, from)), 0);
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Avoiding>();
    // Back to the reach of the other goals (stroll asks its paths with the default).
    crate::goals::set_reach(w, e, 1);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::species::Config;

    static FLEE: Params = Params { max_dist: 8.0, sprint: 1.2, ..Params::DEFAULT };
    static ENTRIES: [Entry; 1] = [Entry::new(&MODULE, 3, &FLEE)];

    /// A walled slab with one animal of the process's species whose only goal is this one (a test configuration: the
    /// species table is not changed), settled on the ground.
    fn herd_with(entries: &'static [Entry]) -> (Herd, Entity) {
        let mut h = Herd::with_slab_awake(20, false, false);
        let mut sp = *species();
        sp.goals = Config { entries };
        let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 7);
        // "Whose only goal is this one": the species' brain (the breeze's slide, the creaking's stroll) would walk it elsewhere.
        h.world.entity_mut(e).remove::<crate::brain::Brain>();
        for _ in 0..20 {
            h.tick();
        }
        (h, e)
    }
    fn put_player(h: &mut Herd, x: f64) {
        h.world.resource_mut::<Online>().set(Player { id: 1, x, y: 1.0, z: 0.5, main: 0, off: 0, ..Default::default() });
    }
    fn gap(h: &Herd, e: Entity, x: f64) -> f64 {
        let p = h.world.get::<Pos>(e).unwrap().0;
        (p.x - x).hypot(p.z - 0.5)
    }

    #[test]
    fn runs_from_a_player_in_range_and_walks_away() {
    if crate::cow::skip_unless_walker("runs_from_a_player_in_range_and_walks_away") {
        return;
    }
        let (mut h, e) = herd_with(&ENTRIES);
        put_player(&mut h, 6.5);
        for _ in 0..10 {
            h.tick();
        }
        assert!(h.world.get::<Avoiding>(e).is_some(), "no run from a player 6 blocks away");
        for _ in 0..100 {
            h.tick();
        }
        assert!(gap(&h, e, 6.5) > 6.5, "the animal did not move away: {:.2} blocks", gap(&h, e, 6.5));
    }

    #[test]
    fn does_not_run_from_a_player_out_of_range() {
    if crate::cow::skip_unless_walker("does_not_run_from_a_player_out_of_range") {
        return;
    }
        let (mut h, e) = herd_with(&ENTRIES);
        put_player(&mut h, 9.5);
        for _ in 0..30 {
            h.tick();
        }
        assert!(h.world.get::<Avoiding>(e).is_none(), "a run from a player 9 blocks away");
    }

    #[test]
    fn stops_when_the_player_goes_away() {
    if crate::cow::skip_unless_walker("stops_when_the_player_goes_away") {
        return;
    }
        let (mut h, e) = herd_with(&ENTRIES);
        put_player(&mut h, 6.5);
        for _ in 0..10 {
            h.tick();
        }
        assert!(h.world.get::<Avoiding>(e).is_some(), "no run to begin with");
        put_player(&mut h, 19.5);
        for _ in 0..400 {
            h.tick();
        }
        assert!(h.world.get::<Avoiding>(e).is_none(), "the run went on after the player left");
    }
}
