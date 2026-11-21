//! `Pufferfish.PufferfishPuffGoal` and `Pufferfish.tick` (fiche 45, `aquatic.rs:983-1012`, `aquatic.rs:214-238`): a pufferfish
//! with a player or a creature in its box (inflated by 2) inflates: size 1 at once, size 2 forty ticks later; when nothing is
//! near any more, it deflates: 2 to 1 sixty-one ticks after, 1 to 0 a hundred and one ticks after. The times are deadlines of
//! the wheel, not counts. Not ported: the sting (effects), the body size (`puff_scale`), the species tag is a parameter.
use super::*;
use crate::{cow::*, goals::*, hostile::PlayerState};
use horizon_core::geometry::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The box grows by this much for the search (`inflate(2)`).
    pub range: f64,
    /// `#not_scary_for_pufferfish`: the species (by name) that do not frighten it. Given by the species, never by this module.
    pub fearless: &'static [&'static str],
}
impl Params {
    pub const DEFAULT: Self = Self { range: 2.0, fearless: &[] };
}

/// The puffing state: the size (0, 1, 2), the goal's rank, the local time of the only valid deadline (`armed_at`, older ones
/// match nothing), and the local time the goal ended (`stopped_at`) the deflation counts from.
#[derive(Component, Clone, Copy, Debug)]
#[component(on_insert = crate::cow::watch_in::<{ crate::cow::WATCH_PUFFER }>, on_discard = crate::cow::watch_out::<{ crate::cow::WATCH_PUFFER }>)]
pub struct Puff {
    pub size: u8,
    pub range: f64,
    pub fearless: &'static [&'static str],
    pub slot: u8,
    pub armed_at: u64,
    pub stopped_at: u64,
    /// The box and eye height at scale 1 (`Pufferfish.getDefaultDimensions` scales them by the size).
    pub base: (f32, f32, f64),
}
/// The goal runs: the puffer's area is checked every tick.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Puffing;

pub static MODULE: Module = Module {
    name: "pufferfish_puff",
    flags: 0,
    equip,
    fired,
    can_use,
    start,
    stop,
    systems,
    requires: &[Cap::Body, Cap::Goals],
    ..Module::DEFAULT
};

fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = *params::<Params>(p);
    let b = *w.get::<Body>(e).unwrap();
    let eye = w.get::<crate::targeting::Eye>(e).map_or(0.0, |x| x.0);
    w.entity_mut(e).insert((
        Puff { size: 0, range: p.range, fearless: p.fearless, slot: inst.slot, armed_at: u64::MAX, stopped_at: 0, base: (b.width, b.height, eye) },
        PlayerWatch::new(3.0),
    ));
    rescale(w, e);
}
/// `Pufferfish.getScale`: 0.5, 0.7, 1.0 by puff state, over the box and the eye height (`refreshDimensions`: the box keeps its feet and its axis).
fn rescale(w: &mut World, e: Entity) {
    let p = *w.get::<Puff>(e).unwrap();
    let s = match p.size {
        0 => 0.5_f32,
        1 => 0.7,
        _ => 1.0,
    };
    let (bw, bh, eye) = (p.base.0 * s, p.base.1 * s, p.base.2 * f64::from(s));
    let mut b = w.get_mut::<Body>(e).unwrap();
    b.width = bw;
    b.height = bh;
    if let Some(mut f) = w.get_mut::<FluidParams>(e) {
        f.eye_height = eye;
    }
    if let Some(mut x) = w.get_mut::<crate::targeting::Eye>(e) {
        x.0 = eye;
    }
    if let Some(at) = w.get::<Pos>(e).map(|p| p.0) {
        w.get_mut::<Bounds>(e).unwrap().0 = box_at(at, bw, bh);
        w.entity_mut(e).insert(Recheck);
    }
}
fn set_puff(w: &mut World, e: Entity, size: u8) {
    w.get_mut::<Puff>(e).unwrap().size = size;
    rescale(w, e);
}
fn arm_at_local(w: &mut World, e: Entity, at: u64) {
    let slot = w.get::<Puff>(e).unwrap().slot;
    w.get_mut::<Puff>(e).unwrap().armed_at = at;
    arm_at(w, e, at, Due::Goal(slot));
}
/// A deadline of this puffer came up: the transitions of its size when it is its own (the time it set), else a replayed
/// start (true: ask `canUse`).
fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    let now = now_of(w, e);
    let p = *w.get::<Puff>(e).unwrap();
    if p.armed_at != now {
        return true;
    }
    if w.get::<Puffing>(e).is_some() {
        // `counter > 40 && size == 1` -> 2 (the forty-first tick of the goal).
        if p.size == 1 {
            set_puff(w, e, 2);
        }
    } else if p.size == 2 {
        // `counter2 > 60 && size == 2` -> 1; the next step is 101 ticks after the end.
        set_puff(w, e, 1);
        arm_at_local(w, e, p.stopped_at + 101);
    } else if p.size == 1 {
        // `counter2 > 100 && size == 1` -> 0.
        set_puff(w, e, 0);
    }
    false
}
/// Whether a player or a creature (not frightened by the puffer) is in the puffer's box inflated by its range.
fn somebody_near(w: &World, e: Entity) -> bool {
    let p = *w.get::<Puff>(e).unwrap();
    let Some(pos) = w.get::<Pos>(e).map(|q| q.0) else { return false };
    let Some(b) = w.get::<Bounds>(e).map(|b| b.0) else { return false };
    let area = b.inflate(Vec3::new(p.range, p.range, p.range));
    let mut near = Vec::new();
    w.resource::<Index>().for_radius(pos.x, pos.z, p.range + 1.0, |c| near.push(c));
    let table = &w.resource::<SpeciesTable>().0;
    for c in near {
        if c == e {
            continue;
        }
        let (Some(cb), Some(sp), Some(h)) = (w.get::<Bounds>(c), w.get::<SpeciesId>(c), w.get::<Health>(c)) else { continue };
        if h.0 > 0.0 && cb.0.intersects(area) && !p.fearless.contains(&table[sp.0 as usize].name) {
            return true;
        }
    }
    let players = w.resource::<PlayerState>();
    let mut found = false;
    w.resource::<Online>().for_near(pos.x, pos.z, p.range + 2.0, |pl| {
        let pb = box_at(Vec3::new(pl.x, pl.y, pl.z), 0.6, 1.8);
        if !found && !players.creative(pl.id) && !players.spectator(pl.id) && players.alive(pl.id) && pb.intersects(area) {
            found = true;
        }
    });
    found
}
/// `PufferfishPuffGoal.canUse`.
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    somebody_near(w, e)
}
/// `start`: the size goes to 1 at once; the grow to 2 is due forty ticks later.
fn start(w: &mut World, e: Entity, _: Inst) {
    let now = now_of(w, e);
    if w.get::<Puff>(e).unwrap().size == 0 {
        set_puff(w, e, 1);
    }
    w.entity_mut(e).insert(Puffing);
    arm_at_local(w, e, now + 40);
}
/// `stop`: the deflation starts from the size it has: 2 is due 61 ticks later, 1 is due 101 ticks later.
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Puffing>();
    let now = now_of(w, e);
    let size = {
        let mut p = w.get_mut::<Puff>(e).unwrap();
        p.stopped_at = now;
        p.size
    };
    match size {
        2 => arm_at_local(w, e, now + 61),
        1 => arm_at_local(w, e, now + 101),
        _ => w.get_mut::<Puff>(e).unwrap().armed_at = u64::MAX,
    }
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems((puff_players, puff_cells, puff_run).chain().in_set(slot));
}
/// A player entered the watch of a pufferfish: it is asked (the goal does not run yet).
fn puff_players(w: &mut World) {
    let watchers: Vec<Entity> = w.resource::<Events>().player_entered.iter().map(|(e, _)| e).collect();
    for e in watchers {
        if w.get::<Puff>(e).is_some() && w.get::<Puffing>(e).is_none() {
            wake(w, e, &MODULE);
        }
    }
}
/// A creature changed column: the idle pufferfish whose box it now touches are asked. Nothing is done while no
/// pufferfish exists.
fn puff_cells(w: &mut World, q: &mut QueryState<Entity, With<Puff>>) {
    if q.iter(w).next().is_none() {
        return;
    }
    let moved: Vec<Entity> = w.resource::<CellMoves>().0.clone();
    for m in moved {
        let (Some(mp), Some(mb), Some(sp), Some(h)) = (w.get::<Pos>(m).map(|p| p.0), w.get::<Bounds>(m).map(|b| b.0), w.get::<SpeciesId>(m), w.get::<Health>(m)) else {
            continue;
        };
        if h.0 <= 0.0 || !w.resource::<Index>().watched_near(crate::cow::WATCH_PUFFER, w.resource::<Index>().cell_of(mp.x, mp.z)) {
            continue;
        }
        let table = &w.resource::<SpeciesTable>().0;
        let name = table[sp.0 as usize].name;
        let mut near = Vec::new();
        w.resource::<Index>().for_radius(mp.x, mp.z, 4.0, |c| near.push(c));
        let mut wake_list = Vec::new();
        for c in near {
            if c == m || w.get::<Puffing>(c).is_some() {
                continue;
            }
            let (Some(p), Some(b)) = (w.get::<Puff>(c), w.get::<Bounds>(c)) else { continue };
            if p.fearless.contains(&name) {
                continue;
            }
            if b.0.inflate(Vec3::new(p.range, p.range, p.range)).intersects(mb) {
                wake_list.push(c);
            }
        }
        for c in wake_list {
            wake(w, c, &MODULE);
        }
    }
}
/// A running puffer whose area is empty ends its goal (`canContinueToUse` = `canUse`).
fn puff_run(w: &mut World, q: &mut QueryState<Entity, With<Puffing>>) {
    let list: Vec<Entity> = q.iter(w).collect();
    for e in list {
        if !somebody_near(w, e) {
            let slot = w.get::<Puff>(e).unwrap().slot;
            end_goal(w, e, slot);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::species::Config;

    static ENTRIES: [Entry; 1] = [Entry::new(&MODULE, 1, &Params { range: 2.0, fearless: &["minecraft:zombie"] })];

    fn herd() -> Option<Herd> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = Herd::with_slab_awake(30, false, false);
        set_lazy_look(&mut h.world, false);
        Some(h)
    }
    /// The puffer: the cow's body renamed, with this module alone (the species of the cow is the id; the name of
    /// a creature is read from the table, so the bystanders are real species of the table).
    fn puffer(h: &mut Herd) -> Entity {
        let mut sp = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == "minecraft:cow").unwrap();
        sp.goals = Config { entries: &ENTRIES };
        let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 13);
        h.world.flush();
        h.world.resource_mut::<Requests>().0.push(Req::Arm(e));
        h.world.flush();
        e
    }
    /// A creature of the table named `name`, without goals (it only moves where the test puts it).
    fn bystander(h: &mut Herd, name: &str, x: f64, z: f64) -> Entity {
        let mut sp = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == name).unwrap();
        sp.goals = Config { entries: &[] };
        spawn_species(&mut h.world, &sp, x, z, 91)
    }
    fn size(h: &Herd, e: Entity) -> u8 {
        h.world.get::<Puff>(e).unwrap().size
    }
    fn running(h: &Herd, e: Entity) -> bool {
        h.world.get::<GoalSet>(e).unwrap().is_running(&MODULE)
    }

    /// A fearless creature (zombie, in the configuration) next to the puffer does not inflate it; a frightening one
    /// (cow) does (witness, same puffer).
    #[test]
    fn a_fearless_creature_does_not_inflate_and_a_frightening_one_does() {
        let Some(mut h) = herd() else { return };
        let p = puffer(&mut h);
        let z = bystander(&mut h, "minecraft:zombie", 0.5, 0.5);
        h.tick();
        h.teleport(z, Vec3::new(0.6, 1.0, 0.6));
        for _ in 0..10 {
            h.tick();
        }
        assert!(!running(&h, p) && size(&h, p) == 0, "a fearless creature near: nothing");
        let c = bystander(&mut h, "minecraft:cow", 20.5, 20.5);
        h.tick();
        h.teleport(c, Vec3::new(0.6, 1.0, 0.6));
        h.tick();
        h.tick();
        assert!(size(&h, p) == 1, "witness: a frightening creature inflates the puffer at once");
    }

    /// A frightening creature: the puffer starts, size 1 then 2 at 40 ticks, and deflates after it leaves.
    #[test]
    fn a_frightening_creature_inflates_and_leaving_deflates_in_steps() {
        let Some(mut h) = herd() else { return };
        let p = puffer(&mut h);
        let c = bystander(&mut h, "minecraft:cow", 20.5, 20.5);
        h.tick();
        h.teleport(c, Vec3::new(0.6, 1.0, 0.6));
        for _ in 0..4 {
            h.tick();
        }
        assert!(running(&h, p), "witness: the creature is in its box, the goal starts");
        assert_eq!(size(&h, p), 1, "size 1 at once");
        for _ in 0..40 {
            h.tick();
        }
        assert_eq!(size(&h, p), 2, "size 2 forty ticks after the start");
        h.teleport(c, Vec3::new(20.5, 1.0, 20.5));
        // The end is the tick the goal is first seen not running: the time of the end, counted from there.
        let mut seen_end = false;
        for _ in 0..6 {
            h.tick();
            if !running(&h, p) {
                seen_end = true;
                break;
            }
        }
        assert!(seen_end, "the creature left: the goal ends");
        assert_eq!(size(&h, p), 2, "still inflated at the end");
        for _ in 0..60 {
            h.tick();
        }
        assert_eq!(size(&h, p), 2, "not yet 61 ticks after the end");
        h.tick();
        assert_eq!(size(&h, p), 1, "size 1 sixty-one ticks after the end");
        for _ in 0..39 {
            h.tick();
        }
        assert_eq!(size(&h, p), 1, "not yet 101 ticks after the end");
        h.tick();
        assert_eq!(size(&h, p), 0, "size 0 a hundred and one ticks after the end");
    }
}
