//! `FollowMobGoal(1.0, 3.0, 7.0)` (the parrot, `tame.rs:118`): an animal of ANOTHER species whose box, grown by 7 blocks, meets
//! the box of the follower is followed. The follower walks to it while it is more than 3 blocks away (a path toward it, reach 1, asked
//! every 5 ticks), stops when it is within 3 blocks, and steps back when `distSqr <= 3` (unsquared, as the `Mob` writes it).
//! It looks at the animal every tick. What wakes it: an animal that changes column near it (`CellMoves`), the follower's own column
//! change, its arrival in the active zone. A goal that ends asks for itself again (the selector's phase), as the `Mob` asks `canUse` again.
use super::*;
use crate::{cow::*, goals::*};
use horizon_client_lib::mobs::path::PathType;
use horizon_core::geometry::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub speed: f64,
    /// The distance (blocks) under which the follower stops walking toward the animal.
    pub stop: f64,
    /// The growth of the follower's box in which an animal of another species starts the goal (at most `SCAN_AREA`).
    pub area: f64,
    /// `recalc` of the `Mob`: the next path ask is `reduced(recalc)` ticks away.
    pub recalc: i32,
}
impl Params {
    pub const DEFAULT: Self = Self { speed: 1.0, stop: 3.0, area: 7.0, recalc: 10 };
}
/// The widest `area` a configuration may give: an animal's move scans the index this far (plus its own half width).
const SCAN_AREA: f64 = 7.0;
/// The path type whose malus the goal sets to 0 while it runs (the follower flies over water).
const WATER: usize = PathType::Water as usize;

#[derive(Component, Clone, Copy, Debug)]
#[component(on_insert = crate::cow::watch_in::<{ crate::cow::WATCH_FOLLOW_MOB }>, on_discard = crate::cow::watch_out::<{ crate::cow::WATCH_FOLLOW_MOB }>)]
pub struct Follower {
    p: Params,
    slot: u8,
    recalc: i32,
    /// The tick the goal started in: its first step runs before any `canContinueToUse`, as the `Mob`'s `tick` does.
    fresh: bool,
    /// The animal `can_use` chose.
    target: Option<Entity>,
    /// The water malus of the entity before the goal (`old_water`), put back when it ends.
    old_water: Option<f32>,
}
/// The goal runs: its step is asked at every tick.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Tailing;

pub static MODULE: Module = Module {
    name: "follow_mob",
    flags: MOVE | LOOK,
    equip,
    arm,
    can_use,
    start,
    stop,
    systems,
    requires: &[Cap::Body, Cap::Goals, Cap::Head, Cap::Locomotion],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, inst: Inst, p: &(dyn std::any::Any + Sync)) {
    let p = *params::<Params>(p);
    debug_assert!(p.area <= SCAN_AREA, "the index scan does not reach {} blocks", p.area);
    w.entity_mut(e).insert(Follower { p, slot: inst.slot, recalc: 0, fresh: false, target: None, old_water: None });
}
/// The first ask: at creation and on entering the active zone (an animal that stood still never moved a column).
fn arm(w: &mut World, e: Entity, inst: Inst) {
    w.resource_mut::<Requests>().0.push(Req::Start(e, inst.slot));
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems((wake_on_moves, follow_run).chain().in_set(slot));
}
/// An animal changed column: a follower of another species whose grown box meets its box is asked; a follower that moved is asked
/// too (its own surroundings changed). Visits nothing in a world without a follower.
fn wake_on_moves(
    moved: Res<CellMoves>,
    idx: Res<Index>,
    any: Query<(), With<Follower>>,
    ids: Query<(&Pos, &Bounds, &SpeciesId)>,
    fols: Query<(&Follower, &Bounds, &SpeciesId, Has<Tailing>)>,
    mut reqs: ResMut<Requests>,
) {
    if any.is_empty() {
        return;
    }
    for &m in &moved.0 {
        let Ok((mp, mb, msid)) = ids.get(m) else { continue };
        let (at, mbox, sid) = (mp.0, mb.0, msid.0);
        if let Ok((f, ..)) = fols.get(m) {
            reqs.0.push(Req::Start(m, f.slot));
        }
        if !idx.watched_near(crate::cow::WATCH_FOLLOW_MOB, idx.cell_of(at.x, at.z)) {
            continue;
        }
        let reach = SCAN_AREA + mbox.size().x / 2.0 + 1.0;
        idx.for_radius(at.x, at.z, reach, |c| {
            if c == m {
                return;
            }
            let Ok((f, fb, fsid, tailing)) = fols.get(c) else { return };
            let area = fb.0.inflate(Vec3::new(f.p.area, f.p.area, f.p.area));
            if !tailing && fsid.0 != sid && area.intersects(mbox) {
                reqs.0.push(Req::Start(c, f.slot));
            }
        });
    }
}
/// The tick of every running follower: one step each (`Req::Call`, in the order of the requests).
fn follow_run(q: Query<Entity, With<Tailing>>, mut reqs: ResMut<Requests>) {
    for e in &q {
        reqs.0.push(Req::Call(e, follow_step));
    }
}
/// `FollowMobGoal.canUse`: the first animal of another species (in the index's order) whose box meets the follower's grown box.
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let Some(f) = w.get::<Follower>(e).copied() else { return false };
    let (Some(pos), Some(bx)) = (w.get::<Pos>(e).map(|p| p.0), w.get::<Bounds>(e).map(|b| b.0)) else { return false };
    let me = w.get::<SpeciesId>(e).map(|s| s.0);
    let area = bx.inflate(Vec3::new(f.p.area, f.p.area, f.p.area));
    let reach = SCAN_AREA + bx.size().x / 2.0 + 1.0;
    let world: &World = w;
    let mut found = None;
    world.resource::<Index>().for_radius(pos.x, pos.z, reach, |c| {
        if found.is_some() || c == e {
            return;
        }
        let (Some(b), Some(s), Some(_)) = (world.get::<Bounds>(c), world.get::<SpeciesId>(c), world.get::<Pos>(c)) else { return };
        if Some(s.0) != me && b.0.intersects(area) {
            found = Some(c);
        }
    });
    let Some(c) = found else { return false };
    w.get_mut::<Follower>(e).unwrap().target = Some(c);
    true
}
/// `FollowMobGoal.start`: the recalc is reset, the water malus is 0 (the old one is kept), and the first step runs now (the `Mob`
/// ticks a goal in the tick it starts).
fn start(w: &mut World, e: Entity, _: Inst) {
    let old = w.get::<NavParams>(e).and_then(|n| n.malus[WATER]);
    if let Some(mut n) = w.get_mut::<NavParams>(e) {
        n.malus[WATER] = Some(0.0);
    }
    {
        let mut f = w.get_mut::<Follower>(e).unwrap();
        (f.recalc, f.fresh, f.old_water) = (0, true, old);
    }
    w.entity_mut(e).insert(Tailing);
    follow_step(w, e);
}
/// `FollowMobGoal.stop`: the target is forgotten, the water malus put back, the walk stopped.
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Tailing>();
    let Some(mut f) = w.get_mut::<Follower>(e) else { return };
    f.target = None;
    let old = f.old_water;
    drop(f);
    if let Some(mut n) = w.get_mut::<NavParams>(e) {
        n.malus[WATER] = old;
    }
    stop_nav(w, e);
}
/// `LookControl.setLookAt` at 10 degrees a tick, the head's bound by default.
fn look_at(w: &mut World, e: Entity, at: [f64; 3], y_speed: f32) {
    let looking = w.get::<Looking>(e).is_some();
    if let Some(mut l) = w.get_mut::<Look>(e) {
        l.target = at;
        l.cooldown = 2;
        l.y_speed = y_speed;
        l.x_max = l.x_default;
    }
    if !looking {
        w.entity_mut(e).insert(Looking);
    }
}
/// One step of the goal of one entity. `canContinueToUse` (not on the first step): the path is busy and the animal is beyond the stop
/// distance; else the goal ends and asks for itself again (`Req::Start` waits for the selector's phase, as `canUse` does).
/// `tick`: the head turns toward the animal; every 5 ticks, a path to it, or a stop and a step back when it is too close.
fn follow_step(w: &mut World, e: Entity) {
    let Some(f) = w.get::<Follower>(e).copied() else { return };
    let end = |w: &mut World| {
        w.resource_mut::<Requests>().0.extend([Req::End(e, f.slot), Req::Start(e, f.slot)]);
    };
    let Some(target) = f.target else { return end(w) };
    let (Some(tp), Some(pos)) = (w.get::<Pos>(target).map(|p| p.0), w.get::<Pos>(e).map(|p| p.0)) else { return end(w) };
    let d = d2(pos, tp);
    let stop2 = f.p.stop * f.p.stop;
    let busy = w.get::<Following>(e).is_some() || w.get::<NeedsPath>(e).is_some();
    if !f.fresh && (!busy || d <= stop2) {
        return end(w);
    }
    // `FollowMobGoal.tick`: a creature on a lead does not turn nor walk to the one it follows.
    if crate::leash::is_leashed(w, e) {
        return;
    }
    let eye = w.get::<Look>(target).map_or(0.0, |l| l.eye);
    look_at(w, e, [tp.x, tp.y + eye, tp.z], 10.0);
    {
        let mut st = w.get_mut::<Follower>(e).unwrap();
        st.fresh = false;
        st.recalc -= 1;
        if st.recalc > 0 {
            return;
        }
        st.recalc = (f.p.recalc + 1) / 2;
    }
    if d > stop2 {
        go_to_entity(w, e, target, f.p.speed, 1);
    } else {
        stop_nav(w, e);
        if d <= f.p.stop {
            let (dx, dz) = (tp.x - pos.x, tp.z - pos.z);
            go_to_at(w, e, [pos.x - dx, pos.y, pos.z - dz], f.p.speed, 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::species::Config;
    use horizon_core::geometry::Aabb;

    static FOLLOW: Params = Params::DEFAULT;
    static ENTRIES: [Entry; 1] = [Entry::new(&MODULE, 3, &FOLLOW)];

    /// A follower (the process species, whose goals are this one) at x = 0.5 that is armed, and an animal of another species
    /// standing still (no goal) at `x`. A test configuration: the species table is not changed.
    fn pair(x: f64) -> (Herd, Entity, Entity) {
        let mut h = Herd::with_slab(20, false, false);
        let mut sp = *species();
        sp.goals = Config { entries: &ENTRIES };
        let parrot = spawn_armed(&mut h.world, &sp, 0.5, 0.5, 7);
        let mut other = species_table().0.into_iter().find(|s| s.id != sp.id).expect("a second species");
        other.goals = Config { entries: &[] };
        let animal = spawn_species(&mut h.world, &other, x, 0.5, 9);
        (h, parrot, animal)
    }
    fn gap(h: &Herd, a: Entity, b: Entity) -> f64 {
        let (p, q) = (h.world.get::<Pos>(a).unwrap().0, h.world.get::<Pos>(b).unwrap().0);
        (p.x - q.x).hypot(p.z - q.z)
    }
    /// Moves the animal to `x` (its box keeps its size): the index is told by the change of its box.
    fn move_animal(h: &mut Herd, animal: Entity, x: f64) {
        let size = h.world.get::<Bounds>(animal).unwrap().0.size();
        let y = h.world.get::<Pos>(animal).unwrap().0.y;
        h.world.get_mut::<Pos>(animal).unwrap().0 = Vec3::new(x, y, 0.5);
        h.world.get_mut::<Bounds>(animal).unwrap().0 = Aabb::new(
            Vec3::new(x - size.x / 2.0, y, 0.5 - size.z / 2.0),
            Vec3::new(x + size.x / 2.0, y + size.y, 0.5 + size.z / 2.0),
        );
    }

    /// Departure: an animal of another species 5 blocks away (inside the grown box) starts the goal within a few ticks.
    #[test]
    fn it_starts_for_an_animal_of_another_species_in_its_area() {
    if crate::cow::skip_unless_walker("it_starts_for_an_animal_of_another_species_in_its_area") {
        return;
    }
        let (mut h, parrot, _) = pair(5.5);
        for _ in 0..6 {
            h.tick();
        }
        assert!(h.world.get::<Tailing>(parrot).is_some(), "no follow with an animal 5 blocks away");
    }

    /// Negative control of the departure: 12 blocks away (outside the grown box) nothing starts.
    #[test]
    fn it_does_not_start_for_an_animal_out_of_its_area() {
    if crate::cow::skip_unless_walker("it_does_not_start_for_an_animal_out_of_its_area") {
        return;
    }
        let (mut h, parrot, _) = pair(12.5);
        for _ in 0..30 {
            h.tick();
        }
        assert!(h.world.get::<Tailing>(parrot).is_none(), "a follow with an animal 12 blocks away");
    }

    /// The wake-up by an event: the animal moves into the area (its column changes) and the follower starts.
    #[test]
    fn an_animal_that_comes_into_the_area_wakes_it() {
    if crate::cow::skip_unless_walker("an_animal_that_comes_into_the_area_wakes_it") {
        return;
    }
        let (mut h, parrot, animal) = pair(12.5);
        for _ in 0..10 {
            h.tick();
        }
        assert!(h.world.get::<Tailing>(parrot).is_none(), "a follow before the animal came");
        move_animal(&mut h, animal, 5.5);
        for _ in 0..6 {
            h.tick();
        }
        assert!(h.world.get::<Tailing>(parrot).is_some(), "the move of the animal into its area did not wake the follower");
    }

    /// Pursuit: it walks toward the animal and comes within the stop distance.
    #[test]
    fn it_walks_toward_the_animal() {
    if crate::cow::skip_unless_walker("it_walks_toward_the_animal") {
        return;
    }
        let (mut h, parrot, animal) = pair(5.5);
        let first = gap(&h, parrot, animal);
        let mut closest = first;
        for _ in 0..200 {
            h.tick();
            closest = closest.min(gap(&h, parrot, animal));
        }
        assert!(closest <= 3.5 && closest < first - 1.5, "no walk toward the animal: from {first:.2} to {closest:.2} blocks");
    }

    /// Stop: the goal ends when the follower is within the stop distance. An end while it is still in the air (no path is possible
    /// there: `GroundPathNavigation`) comes first and is asked again, as the `Mob` does: the witness is an end within 3 blocks.
    #[test]
    fn it_stops_within_three_blocks_of_the_animal() {
    if crate::cow::skip_unless_walker("it_stops_within_three_blocks_of_the_animal") {
        return;
    }
        let (mut h, parrot, animal) = pair(5.5);
        let (mut was_running, mut ends) = (false, Vec::new());
        for _ in 0..200 {
            h.tick();
            let running = h.world.get::<Tailing>(parrot).is_some();
            if was_running && !running {
                ends.push(gap(&h, parrot, animal));
            }
            was_running |= running;
        }
        assert!(ends.iter().any(|&d| d <= 3.5), "no end within 3 blocks of the animal; ends at {ends:.2?}");
    }
}
