//! `Trader.MoveTowardsRestrictionGoal` (`trader.rs:277-325`), also the guardian's priority-5 goal (`guardian.rs:52`, with LOOK): an
//! animal kept in a restriction (a block and a radius: `ai.home`) that is out of it walks to a point drawn toward home
//! (`DefaultRandomPos.getPosTowards`: 16 by 7, within a quarter turn). The walk is a path that ends the goal; while the animal stays
//! out, the goal is asked again two ticks after it ends (the selector's ask, as the `Mob`'s `canUse` every other tick). Inside the
//! restriction nothing is armed: the animal is not asked again until a new deadline (see DOUTE in the notes: no event says it left).
//! Two modules with the same code, the flags differing (a `Module` has static flags): `MODULE` (MOVE, the trader) and `MODULE_LOOK`
//! (MOVE | LOOK, the guardian).
use super::*;
use crate::rules::move_towards_restriction::{home_target, within_home};
use crate::{cow::*, goals::*, spots::Pick};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The speed of the walk (`nav_move_to`).
    pub speed: f64,
}
impl Params {
    /// The wandering trader's speed (`trader.rs:85`).
    pub const DEFAULT: Self = Self { speed: 0.35 };
}
/// The restriction of the animal (`ai.home`): the block it is kept near and the radius in blocks. Posed by whoever restricts the
/// animal (not ported: DOUTE); without it the goal never starts (`Mob`: `m.ai.home` absent, `can_use` false).
#[derive(Component, Clone, Copy, Debug)]
pub struct Restriction {
    pub center: [i32; 3],
    pub radius: i32,
    /// The point the goal walks to (`wanted`), drawn by `can_use` and read by `start`.
    pub wanted: Option<[f64; 3]>,
}
/// The walk speed of the entry (from `Params`).
#[derive(Component, Clone, Copy, Debug)]
struct Speed(f64);

pub static MODULE: Module = Module {
    name: "move_towards_restriction",
    flags: MOVE,
    ends_with_path: true,
    equip,
    arm,
    fired,
    can_use,
    start,
    stop,
    requires: &[Cap::Body, Cap::Goals, Cap::Locomotion],
    ..Module::DEFAULT
};
pub static MODULE_LOOK: Module = Module {
    name: "move_towards_restriction_look",
    flags: MOVE | LOOK,
    ends_with_path: true,
    equip,
    arm,
    fired,
    can_use,
    start,
    stop,
    requires: &[Cap::Body, Cap::Goals, Cap::Locomotion, Cap::Head],
    ..Module::DEFAULT
};

fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Speed(params::<Params>(p).speed));
}
/// `Mob.isWithinHome` (`goals::within_home` of the `Mob`): the floored position against the restriction, `d^2 < radius^2`.
fn out_of_home(w: &World, e: Entity) -> bool {
    let (Some(r), Some(p)) = (w.get::<Restriction>(e), w.get::<Pos>(e)) else { return false };
    !within_home(r.center, r.radius, [p.0.x, p.0.y, p.0.z])
}
/// Asks `canUse` again two ticks on, while the animal is out of its restriction.
fn rearm(w: &mut World, e: Entity, inst: Inst) {
    if out_of_home(w, e) {
        crate::goals::arm(w, e, inst.slot, 2);
    }
}
fn arm(w: &mut World, e: Entity, inst: Inst) {
    rearm(w, e, inst);
}
/// A deadline of the module (the two-tick ask) always asks `canUse`.
fn fired(_: &mut World, _: Entity, _: Inst) -> bool {
    true
}
/// `canUse`: out of the restriction, a point drawn toward home. The draw is `pick_for(DefaultTowards)`, the same draw order as the `Mob`.
fn can_use(w: &mut World, e: Entity, inst: Inst) -> bool {
    let Some(r) = w.get::<Restriction>(e).copied() else { return false };
    let Some(p) = w.get::<Pos>(e).map(|p| p.0) else { return false };
    if within_home(r.center, r.radius, [p.x, p.y, p.z]) {
        return false;
    }
    let to = home_target(r.center);
    let pick = crate::spots::pick_for(w, e, Pick::DefaultTowards { h: 16, v: 7, to, max_rad: std::f64::consts::FRAC_PI_2 });
    match pick {
        Some(point) => {
            w.get_mut::<Restriction>(e).unwrap().wanted = Some(point);
            true
        }
        None => {
            rearm(w, e, inst);
            false
        }
    }
}
/// `nav_move_to(point, speed)`: a path to the point, ending within one block (`reach` 1, as `nav_move_to`).
fn start(w: &mut World, e: Entity, _: Inst) {
    let (Some(point), Some(speed)) = (w.get::<Restriction>(e).and_then(|r| r.wanted), w.get::<Speed>(e).map(|s| s.0)) else { return };
    go_to_at(w, e, point, speed, 1);
}
/// The goal ends: the point is forgotten; still out of the restriction, the ask comes again two ticks on.
fn stop(w: &mut World, e: Entity, inst: Inst) {
    if let Some(mut r) = w.get_mut::<Restriction>(e) {
        r.wanted = None;
    }
    rearm(w, e, inst);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behaviours::Entry;
    use crate::cow::{ActiveUntil, Herd, SpeciesTable, spawn_species};
    use crate::species::Config;
    use horizon_core::geometry::Vec3;

    fn config(look: bool) -> Config {
        let entries: &'static [Entry] = if look {
            Box::leak(Box::new([Entry::new(&MODULE_LOOK, 5, &Params { speed: 1.0 })]))
        } else {
            Box::leak(Box::new([Entry::new(&MODULE, 5, &Params::DEFAULT)]))
        };
        Config { entries }
    }
    /// A zombie with this goal alone, restricted to a block at the origin of the slab with a radius of 6; `None` without the resources.
    fn world(look: bool, dx: f64) -> Option<(Herd, Entity)> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = Herd::with_slab_awake(200, true, false);
        crate::goals::set_lazy_look(&mut h.world, false);
        let table = h.world.resource::<SpeciesTable>().0.clone();
        let mut sp = *table.iter().find(|s| s.name == "minecraft:zombie").unwrap();
        sp.goals = config(look);
        let b = spawn_species(&mut h.world, &sp, 0.5 + dx, 0.5, 91);
        h.world.get_mut::<ActiveUntil>(b).unwrap().0 = u64::MAX;
        let y = h.world.get::<Pos>(b).unwrap().0.y;
        h.world.entity_mut(b).insert(Restriction { center: [0, y.floor() as i32, 0], radius: 6, wanted: None });
        crate::goals::arm_all(&mut h.world);
        h.tick();
        Some((h, b))
    }
    fn dist(h: &Herd, b: Entity) -> f64 {
        h.world.get::<Pos>(b).unwrap().0.x.hypot(h.world.get::<Pos>(b).unwrap().0.z)
    }
    fn running(h: &Herd, b: Entity) -> bool {
        h.world.get::<GoalSet>(b).unwrap().is_running(&MODULE) || h.world.get::<GoalSet>(b).unwrap().is_running(&MODULE_LOOK)
    }

    /// Twenty blocks out: the goal starts and walks the animal back; at two blocks out (inside the radius of 6) it never starts.
    /// The first case is the witness of the second.
    #[test]
    fn an_animal_out_of_its_restriction_walks_back_and_one_inside_does_not() {
        let Some((mut h, b)) = world(false, 20.0) else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        let start = dist(&h, b);
        let mut started = false;
        for _ in 0..400 {
            h.tick();
            started |= running(&h, b);
        }
        assert!(started, "the goal never started 20 blocks out");
        assert!(dist(&h, b) < start - 1.0, "the animal did not walk back: {start} -> {}", dist(&h, b));

        let Some((mut h2, b2)) = world(false, 2.0) else { return };
        let mut started2 = false;
        for _ in 0..200 {
            h2.tick();
            started2 |= running(&h2, b2);
        }
        assert!(!started2, "the goal started inside the restriction");
    }

    /// A guardian entry (MOVE | LOOK) starts the same walk back: the look variant is the same goal with the flag.
    #[test]
    fn the_look_variant_starts_the_same_walk() {
        let Some((mut h, b)) = world(true, 20.0) else { return };
        let mut started = false;
        for _ in 0..400 {
            h.tick();
            started |= running(&h, b);
        }
        assert!(started, "the look variant never started 20 blocks out");
    }
}
