//! `CatRelaxOnOwnerGoal` (fiche 62, `Mob` `tame.rs:1262-1404`, priorité 3 au code): while the owner sleeps, a tame cat that is not
//! sitting walks to the foot of the bed (the block behind the bed's `facing`), lies down there after `reduced(16)` ticks within 1.58
//! blocks of the owner, and looks at the owner before that. It ends when the owner wakes, the cat is ordered to sit, or another cat lies
//! within the box of the bed. Wake-up: the owner entering the 10-block watch (the fallback of the fiche; the sleep is not an event yet).
//! The morning gift is NOT ported (its items and its teleport are absent from the frame; the draw of `next_float` is missing with it).
use super::cat_state::{self as cs, OwnerAsleep};
use super::*;
use crate::{cow::*, goals::*, tame, targeting::Who};
use horizon_core::geometry::{Aabb, Vec3};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// `distanceToSqr(owner) <= 100` to start.
    pub near_sqr: f64,
    /// `distanceToSqr(owner) < 2.5` to count the ticks on the bed.
    pub stop_sqr: f64,
    /// `reduced(16)` ticks on the bed before the cat lies down.
    pub relax_asks: i32,
}
impl Params {
    pub const DEFAULT: Self = Self { near_sqr: 100.0, stop_sqr: 2.5, relax_asks: 16 };
}

#[derive(Component, Clone, Copy, Debug)]
pub struct Relax {
    p: &'static Params,
    slot: u8,
    goal: Option<[i32; 3]>,
    on_bed: i32,
    refresh: i32,
}
/// The goal runs: its system visits this cat each tick.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Relaxing;

pub static MODULE: Module = Module {
    name: "cat_relax_on_owner",
    flags: 0,
    equip,
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
    w.entity_mut(e).insert(Relax { p, slot: inst.slot, goal: None, on_bed: 0, refresh: 0 });
    if w.get::<PlayerWatch>(e).is_none() {
        w.entity_mut(e).insert(PlayerWatch::new(10.0));
    }
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems((wake_events, relax_run).chain().in_set(slot));
}
/// The owner came within 10 blocks: the cat asks to relax (the selector decides whether the owner sleeps).
fn wake_events(ev: Res<Events>, q: Query<(&Relax, &tame::Owner)>, mut reqs: ResMut<Requests>) {
    for (e, id) in ev.player_entered.iter() {
        if let Ok((r, o)) = q.get(e)
            && o.0 == Some(Who::Player(id))
        {
            reqs.0.push(Req::Start(e, r.slot));
        }
    }
}
fn relax_run(q: Query<Entity, With<Relaxing>>, mut reqs: ResMut<Requests>) {
    for e in &q {
        reqs.0.push(Req::Call(e, relax_step));
    }
}

/// `spaceIsOccupied`: another cat lying within the box of two blocks around the goal (`Mob` `others_near`, `Bounds` intersecting).
fn occupied(w: &World, e: Entity, g: [i32; 3]) -> bool {
    let (x, y, z) = (f64::from(g[0]), f64::from(g[1]), f64::from(g[2]));
    let area = Aabb::new(Vec3::new(x - 2.0, y - 2.0, z - 2.0), Vec3::new(x + 3.0, y + 3.0, z + 3.0));
    let mut hit = false;
    w.resource::<Index>().for_radius(x + 0.5, z + 0.5, 3.0, |o| {
        if o != e && cs::pose(w, o).lying && w.get::<Bounds>(o).is_some_and(|b| b.0.intersects(area)) {
            hit = true;
        }
    });
    hit
}

/// `canUse`: an owner asleep (the marker, see BESOIN), a tame cat not sitting, the owner within 10 blocks, its block a bed of
/// `cats_can_lie_on`; the goal is the block behind the bed's `facing`.
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    if !cs::owner_asleep(w, e) || !cs::tamed(w, e) || is_occupied(w, e, OCC_SITTING) {
        return false;
    }
    let Some(p) = tame::owner_player(w, e) else { return false };
    if tame::owner_distance_sqr(w, e).is_none_or(|d| d > w.get::<Relax>(e).unwrap().p.near_sqr) {
        return false;
    }
    let at = [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32];
    let (tagged, facing) = {
        let mw = cs::level(w);
        (mw.has_tag("minecraft:cats_can_lie_on", mw.block_name(at[0], at[1], at[2])), crate::blocks::property(mw, at, "facing"))
    };
    if !tagged {
        return false;
    }
    let back = match facing.as_deref() {
        Some("north") => [0, 0, 1],
        Some("south") => [0, 0, -1],
        Some("east") => [-1, 0, 0],
        Some("west") => [1, 0, 0],
        Some("up") => [0, -1, 0],
        Some("down") => [0, 1, 0],
        _ => [0, 0, 0],
    };
    let g = [at[0] + back[0], at[1] + back[1], at[2] + back[2]];
    if let Some(mut r) = w.get_mut::<Relax>(e) {
        r.goal = Some(g);
    }
    !occupied(w, e, g)
}
fn goal_f(g: [i32; 3]) -> [f64; 3] {
    [f64::from(g[0]), f64::from(g[1]), f64::from(g[2])]
}
fn start(w: &mut World, e: Entity, _: Inst) {
    cs::set(w, e, |c| c.sitting_pose = false);
    let g = w.get::<Relax>(e).and_then(|r| r.goal);
    if let Some(g) = g {
        go_to_at(w, e, goal_f(g), 1.1, 1);
    }
    if let Some(mut r) = w.get_mut::<Relax>(e) {
        r.refresh = 0;
    }
    w.entity_mut(e).insert(Relaxing);
}
/// The end: not lying, the count of the bed cleared, the walk stopped. (The gift of the morning is not ported.)
fn stop(w: &mut World, e: Entity, _: Inst) {
    cs::set(w, e, |c| {
        c.lying = false;
        c.relax = false;
    });
    if let Some(mut r) = w.get_mut::<Relax>(e) {
        r.on_bed = 0;
    }
    w.entity_mut(e).remove::<Relaxing>();
    stop_nav(w, e);
}

/// `canContinueToUse` and `tick`, each tick of the goal.
fn relax_step(w: &mut World, e: Entity) {
    let Some(r) = w.get::<Relax>(e).copied() else { return };
    let ends = match r.goal {
        None => true,
        Some(g) => !cs::owner_asleep(w, e) || is_occupied(w, e, OCC_SITTING) || occupied(w, e, g),
    };
    if ends {
        w.resource_mut::<Requests>().0.push(Req::End(e, r.slot));
        return;
    }
    let (Some(g), Some(p)) = (r.goal, tame::owner_player(w, e)) else { return };
    cs::set(w, e, |c| c.sitting_pose = false);
    // The walk is asked again every 20 ticks (the path frame's recompute delay), not at every tick as the `Mob` does.
    let mut rr = r;
    if rr.refresh <= 0 {
        go_to_at(w, e, goal_f(g), 1.1, 1);
        rr.refresh = 20;
    } else {
        rr.refresh -= 1;
    }
    let Some(d) = tame::owner_distance_sqr(w, e) else { return };
    if d < rr.p.stop_sqr {
        rr.on_bed += 1;
        if rr.on_bed > cs::reduced(rr.p.relax_asks) {
            cs::set(w, e, |c| {
                c.lying = true;
                c.relax = false;
            });
        } else {
            cs::set(w, e, |c| c.relax = true);
            crate::brain::look_at(w, e, [p.x, p.y + 1.62, p.z]);
        }
    } else {
        cs::set(w, e, |c| c.lying = false);
    }
    *w.get_mut::<Relax>(e).unwrap() = rr;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behaviours::Entry;
    use crate::cow::{ActiveUntil, SpeciesTable, spawn_species};
    use crate::species::Config;

    fn config() -> Config {
        let entries: &'static [Entry] = Box::leak(Box::new([Entry::new(&MODULE, 3, &Params::DEFAULT)]));
        Config { entries }
    }
    fn running(h: &crate::cow::Herd, e: Entity) -> bool {
        h.world.get::<GoalSet>(e).unwrap().is_running(&MODULE)
    }
    /// A tame wolf (owner player 1) with this goal alone, on the slab, awake; the owner stands at (10.5, 0.5).
    fn world() -> Option<(crate::cow::Herd, Entity)> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = crate::cow::Herd::with_slab_awake(200, false, false);
        crate::goals::set_lazy_look(&mut h.world, false);
        let table = h.world.resource::<SpeciesTable>().0.clone();
        let mut sp = *table.iter().find(|s| s.name == "minecraft:wolf").unwrap();
        sp.goals = config();
        let c = spawn_species(&mut h.world, &sp, 6.5, 0.5, 77);
        h.world.get_mut::<ActiveUntil>(c).unwrap().0 = u64::MAX;
        h.world.resource_mut::<crate::goals::Entered>().0.push(c);
        crate::tame::set_owner(&mut h.world, c, Who::Player(1));
        h.world.resource_mut::<Online>().set(Player { id: 1, x: 10.5, y: 1.0, z: 0.5, main: 0, off: 0, ..Default::default() });
        h.tick();
        Some((h, c))
    }
    /// A white bed at the owner's feet, its head facing north: the cat's goal is the block north of it (z + 1).
    fn bed(h: &mut crate::cow::Herd) {
        h.set_block_state(10, 1, 0, "minecraft:white_bed", &[("facing", "north"), ("part", "foot")]);
        h.tick();
    }
    fn ticks(h: &mut crate::cow::Herd, n: usize) {
        for _ in 0..n {
            h.tick();
        }
    }

    /// The owner asleep beside a bed: the cat walks to the foot of the bed, lies down after the count, and is near the owner.
    #[test]
    fn a_cat_relaxes_beside_its_sleeping_owner_and_lies_down() {
        let Some((mut h, c)) = world() else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        bed(&mut h);
        ticks(&mut h, 3);
        assert!(!running(&h, c), "témoin négatif : pas de départ sans sommeil du propriétaire");
        h.world.entity_mut(c).insert(OwnerAsleep);
        // Witness: the owner comes in view (the wake-up the frame gives on `player_entered`) and the cat starts.
        crate::goals::wake(&mut h.world, c, &MODULE);
        let mut started = false;
        for _ in 0..30 {
            h.tick();
            started |= running(&h, c);
        }
        assert!(started, "témoin : le départ à la visite du propriétaire endormi");
        ticks(&mut h, 200);
        let p = h.world.get::<Pos>(c).unwrap().0;
        assert!((p.x - 10.5).powi(2) + (p.z - 1.0).powi(2) < 4.0, "le chat est au pied du lit (x {}, z {})", p.x, p.z);
        assert!(crate::behaviours::cat_state::pose(&h.world, c).lying, "le chat se couche après le compte");
    }

    /// Waking the owner ends the goal: the cat is no longer lying nor running.
    #[test]
    fn the_goal_ends_when_the_owner_wakes() {
        let Some((mut h, c)) = world() else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        bed(&mut h);
        h.world.entity_mut(c).insert(OwnerAsleep);
        crate::goals::wake(&mut h.world, c, &MODULE);
        let mut started = false;
        for _ in 0..30 {
            h.tick();
            started |= running(&h, c);
        }
        assert!(started, "témoin : le départ");
        ticks(&mut h, 100);
        h.world.entity_mut(c).remove::<OwnerAsleep>();
        ticks(&mut h, 4);
        assert!(!running(&h, c), "le but s'arrête au réveil du propriétaire");
        assert!(!crate::behaviours::cat_state::pose(&h.world, c).lying, "le chat se relève");
    }
}
