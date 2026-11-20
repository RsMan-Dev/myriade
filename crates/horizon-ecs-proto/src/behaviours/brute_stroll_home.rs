//! `StrollToPoi(HOME, 0.6, 2, 100)` of the piglin brute (fiche 33, `mobs/piglin.rs:1484-1541`): a brute more than 10 and less
//! than 100 blocks from the spot it was made at walks back to within 2 blocks of it, one try per 80 ticks. The wake is the
//! animal's column change: a brute that does not move asks nothing.
use super::*;
use crate::{cow::*, goals::*};
use horizon_core::geometry::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub speed: f64,
    /// Blocks: the brute walks home only beyond this distance...
    pub near: f64,
    /// ...and not beyond this one.
    pub far: f64,
    /// The path ends within this many blocks of home.
    pub reach: i32,
    /// Ticks between two tries (`nextOkStartTime`).
    pub retry: u64,
}
impl Params {
    pub const DEFAULT: Self = Self { speed: 0.6, near: 10.0, far: 100.0, reach: 2, retry: 80 };
}
/// The spot the animal was made at (its block), its configuration, and the time of the next try.
#[derive(Component, Clone, Copy, Debug)]
pub struct Home {
    pub block: [i32; 3],
    pub def: &'static Params,
    pub next_ok: u64,
}
/// The goal runs: the walk home is under way.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct GoingHome;

pub static MODULE: Module = Module {
    name: "brute_stroll_home",
    flags: MOVE,
    ends_with_path: true,
    equip,
    fired,
    can_use,
    start,
    stop,
    systems,
    requires: &[Cap::Body, Cap::Goals, Cap::Locomotion],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let def = params::<Params>(p);
    let p = w.get::<Pos>(e).unwrap().0;
    let block = [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32];
    w.entity_mut(e).insert(Home { block, def, next_ok: 0 });
}
/// `Vec3.atBottomCenterOf` of the home block: the point the distance is measured from.
fn center(block: [i32; 3]) -> Vec3 {
    Vec3::new(f64::from(block[0]) + 0.5, f64::from(block[1]), f64::from(block[2]) + 0.5)
}
/// Squared distance from the animal to home, and whether it lies in the band where the walk home applies.
fn in_band(w: &World, e: Entity, h: &Home) -> bool {
    in_band_at(w.get::<Pos>(e).unwrap().0, h)
}
fn in_band_at(p: Vec3, h: &Home) -> bool {
    let c = center(h.block);
    let d2 = (p.x - c.x).powi(2) + (p.y - c.y).powi(2) + (p.z - c.z).powi(2);
    d2 > h.def.near * h.def.near && d2 < h.def.far * h.def.far
}
/// `canUse`: in the band and past the time of the last try; the try sets the next one `retry` ticks on. The path is
/// not checked here (no service tells whether one exists: DOUTE); a path that ends at once ends the goal at once.
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let Some(h) = w.get::<Home>(e).copied() else { return false };
    if !in_band(w, e, &h) {
        return false;
    }
    let now = now_of(w, e);
    if now < h.next_ok {
        return false;
    }
    w.get_mut::<Home>(e).unwrap().next_ok = now + h.def.retry;
    true
}
fn start(w: &mut World, e: Entity, _: Inst) {
    let h = *w.get::<Home>(e).unwrap();
    let b = h.block;
    go_to_at(w, e, [f64::from(b[0]) + 0.5, f64::from(b[1]), f64::from(b[2]) + 0.5], h.def.speed, h.def.reach);
    w.entity_mut(e).insert(GoingHome);
}
/// `nav_stop`; the next try is armed for the time left before the gate opens (at least one tick).
fn stop(w: &mut World, e: Entity, inst: Inst) {
    w.entity_mut(e).remove::<GoingHome>();
    stop_nav(w, e);
    let Some(h) = w.get::<Home>(e).copied() else { return };
    let left = h.next_ok.saturating_sub(now_of(w, e)).max(1);
    crate::goals::arm(w, e, inst.slot, left as i32);
}
/// A deadline that is not the goal's own (the replay of a start) asks `canUse`; a running goal has none.
fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    w.get::<GoingHome>(e).is_none()
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems(wake_on_move.in_set(slot));
}
/// A brute that changed its column may now be in the band: its goal is asked (through the selector, at its phase). Only the
/// animals that moved are visited (`CellMoves`): a world where nobody moves costs nothing here.
fn wake_on_move(moved: Res<CellMoves>, clock: Res<Clock>, homes: Query<(&Home, &Pos, &GoalSet), Without<GoingHome>>, mut reqs: ResMut<Requests>) {
    for &e in &moved.0 {
        let Ok((h, p, g)) = homes.get(e) else { continue };
        if clock.of(e) >= h.next_ok && in_band_at(p.0, h) && let Some(slot) = g.slot_of(&MODULE) {
            reqs.0.push(Req::Start(e, slot));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behaviours::Entry;
    use crate::cow::{ActiveUntil, Herd, SpeciesTable, spawn_species};
    use crate::species::Config;

    fn config() -> Config {
        let entries: &'static [Entry] = Box::leak(Box::new([Entry::new(&MODULE, 5, &Params::DEFAULT)]));
        Config { entries }
    }
    fn running(h: &Herd, e: Entity) -> bool {
        h.world.get::<GoalSet>(e).unwrap().is_running(&MODULE)
    }
    /// A brute with this goal alone, made at the origin of the slab.
    fn world() -> Option<(Herd, Entity)> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = Herd::with_slab(200, true, false);
        crate::goals::set_lazy_look(&mut h.world, false);
        let table = h.world.resource::<SpeciesTable>().0.clone();
        let mut sp = *table.iter().find(|s| s.name == "minecraft:zombie").unwrap();
        sp.goals = config();
        let b = spawn_species(&mut h.world, &sp, 0.5, 0.5, 51);
        h.world.get_mut::<ActiveUntil>(b).unwrap().0 = u64::MAX;
        h.tick();
        Some((h, b))
    }
    fn away(h: &mut Herd, b: Entity, dx: f64) {
        let y = h.world.get::<Pos>(b).unwrap().0.y;
        h.teleport(b, Vec3::new(0.5 + dx, y, 0.5));
    }
    fn ticks(h: &mut Herd, n: usize) {
        for _ in 0..n {
            h.tick();
        }
    }

    /// À 30 blocks de sa maison : départ et retour à moins de 2 blocks ; à 5 blocs : aucun départ (témoin : à 30, il part).
    #[test]
    fn a_brute_thirty_blocks_from_home_walks_back_and_one_at_five_does_not() {
        let Some((mut h, b)) = world() else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        away(&mut h, b, 5.0);
        ticks(&mut h, 20);
        assert!(!running(&h, b), "pas de départ à 5 blocs");
        away(&mut h, b, 30.0);
        let mut started = false;
        for _ in 0..200 {
            h.tick();
            started |= running(&h, b);
        }
        assert!(started, "témoin : départ à 30 blocs");
        ticks(&mut h, 600);
        let x = h.world.get::<Pos>(b).unwrap().0.x;
        assert!(x - 0.5 <= 2.5, "retour à moins de 2 blocs de la maison (x = {x})");
    }
}
