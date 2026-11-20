//! `DrownedGoToBeachGoal` (`mobs/undead.rs:390-436`, priority 5, `MoveToBlock(1.0, 8, 2)`): at night, a drowned in the water with
//! the sea surface at most 3 blocks above it walks to the nearest beach block (solid top, two air blocks above, no fluid there).
//! The search is a countdown: each search draws `reduced(200 + nextInt(200))` before it tests anything, and the next search comes
//! `2 * (countdown + 1)` ticks later, whatever the result (the `Mob` counts its asks down). One deadline, no chain of requests.
//! The walk: its flags are MOVE | JUMP; `flag2` (searching for land) is cleared at its start, so the marker `WantsToSwim` goes.
use super::*;
use crate::locomotion::swim::WantsToSwim;
use crate::move_to_block::MoveToBlock;
use crate::hostile::Daylight;
use crate::{cow::*, goals::*};
use horizon_client_lib::blocks::Fluid;
use horizon_client_lib::mobs::MobWorld;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub speed: f64,
    pub range: i32,
    pub vertical: i32,
}
impl Params {
    /// `new MoveToBlockGoal(1.0, 8, 2)` (`undead.rs:117`).
    pub const DEFAULT: Self = Self { speed: 1.0, range: 8, vertical: 2 };
}

/// The state: the walk, the countdown drawn at the last search, whether a deadline is armed.
#[derive(Component, Clone, Debug)]
pub struct Beach {
    pub walk: MoveToBlock,
    pub countdown: i32,
    pub armed: bool,
    pub slot: u8,
}
/// The walk runs: its system-free tick (`tick_all`) visits it.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Beaching;

pub static MODULE: Module = Module {
    name: "drowned_go_to_beach",
    tick: Some(tick),
    flags: MOVE | JUMP,
    equip,
    arm: arm_first,
    fired,
    can_use,
    start,
    stop,
    requires: &[Cap::Body, Cap::Goals, Cap::Locomotion],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let def = *params::<Params>(p);
    let walk = MoveToBlock::new(def.speed, def.range, def.vertical);
    w.entity_mut(e).insert(Beach { walk, countdown: 0, armed: false, slot: inst.slot });
}
/// Arms the next search, once: a second arming would start a second countdown for the animal.
fn schedule(w: &mut World, e: Entity, wait: i32) {
    let slot = {
        let Some(mut b) = w.get_mut::<Beach>(e) else { return };
        if b.armed {
            return;
        }
        b.armed = true;
        b.slot
    };
    crate::goals::arm(w, e, slot, wait);
}
fn arm_first(w: &mut World, e: Entity, _: Inst) {
    schedule(w, e, 1);
}
fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    if let Some(mut b) = w.get_mut::<Beach>(e) {
        b.armed = false;
    }
    w.get::<Beaching>(e).is_none()
}
fn reduced(n: i32) -> i32 {
    (n + 1) / 2
}
fn feet(w: &World, e: Entity) -> [i32; 3] {
    let p = w.get::<Pos>(e).unwrap().0;
    [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32]
}
/// `DrownedGoToBeachGoal`'s `beach_target`: a full top face, two empty cells above with no fluid.
fn beach_valid(mw: &MobWorld, p: [i32; 3]) -> bool {
    let empty = |y: i32| {
        let s = mw.world.block(p[0], y, p[2]);
        mw.facts.is_air(s) && mw.facts.get(s).fluid == Fluid::None
    };
    empty(p[1] + 1) && empty(p[1] + 2) && mw.facts.get(mw.world.block(p[0], p[1], p[2])).sturdy_full & (1 << 1) != 0
}
/// `canUse`: draw the countdown, search; the goal starts only if the night, the water and the depth hold. Otherwise the next
/// search is armed after the countdown.
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let countdown = with_rng(w, e, |r| reduced(200 + r.next_int(200)));
    let base = feet(w, e);
    let mut b = w.get::<Beach>(e).unwrap().clone();
    let ok = {
        let mw = w.resource::<Level>().mw.0;
        let found = b.walk.find_nearest(base, mw, beach_valid);
        let p = w.get::<Pos>(e).unwrap().0;
        let in_water = w.get::<FluidState>(e).is_some_and(|f| f.in_water);
        found && !w.resource::<Daylight>().0 && in_water && p.y >= f64::from(mw.sea_level - 3)
    };
    b.countdown = countdown;
    *w.get_mut::<Beach>(e).unwrap() = b;
    if !ok {
        schedule(w, e, 2 * (countdown + 1));
    }
    ok
}
fn start(w: &mut World, e: Entity, _: Inst) {
    let mut b = w.get::<Beach>(e).unwrap().clone();
    with_rng(w, e, |r| b.walk.start(r));
    go_to_at(w, e, b.walk.target(), b.walk.speed, 1);
    w.entity_mut(e).insert(Beaching);
    w.entity_mut(e).remove::<WantsToSwim>();
    *w.get_mut::<Beach>(e).unwrap() = b;
}
/// The end of the walk: the countdown drawn at the last search runs again from now.
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Beaching>();
    let n = w.get::<Beach>(e).map_or(0, |b| b.countdown);
    schedule(w, e, 2 * (n + 1));
}
/// The walk, each tick of the goal (`MoveToBlockGoal.canContinue`, then `tick`): it ends when its block is lost; the path is
/// asked again each time `MoveToBlock.tick` says so. Runs from the selector, not as a system.
fn tick(w: &mut World) {
    static ID: std::sync::OnceLock<Option<usize>> = std::sync::OnceLock::new();
    let id = *ID.get_or_init(|| crate::species::registry().module_id(&MODULE));
    let Some(i) = id else { return };
    if w.resource::<Carriers>().0.get(i).is_none_or(|n| *n == 0) {
        return;
    }
    let walking: Vec<Entity> = w.query_filtered::<Entity, With<Beaching>>().iter(w).collect();
    for e in walking {
        let mut b = w.get::<Beach>(e).unwrap().clone();
        let can = {
            let mw = w.resource::<Level>().mw.0;
            b.walk.can_continue(mw, beach_valid)
        };
        if !can {
            *w.get_mut::<Beach>(e).unwrap() = b;
            end(w, e, &MODULE);
            continue;
        }
        let p = w.get::<Pos>(e).unwrap().0;
        if b.walk.tick([p.x, p.y, p.z], 1.0) {
            go_to_at(w, e, b.walk.target(), b.walk.speed, 1);
        }
        *w.get_mut::<Beach>(e).unwrap() = b;
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
    fn step(h: &mut Herd) {
        h.tick();
    }
    fn run(h: &mut Herd, n: usize) {
        for _ in 0..n {
            step(h);
        }
    }
    /// A drowned in the sea-level pool: a platform of stone at `sea - 4`, water from `sea - 3` to `sea + 1`, the drowned at
    /// `sea - 3` (the depth the goal needs). The beach: stone at row `sea - 4` over x 6..=7, z -1..=1, with air above it.
    fn world() -> Option<(Herd, Entity, i32)> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = Herd::with_slab(200, false, false);
        crate::goals::set_lazy_look(&mut h.world, false);
        let table = h.world.resource::<SpeciesTable>().0.clone();
        let mut sp = *table.iter().find(|s| s.name == "minecraft:drowned").unwrap();
        sp.goals = config();
        let sea = h.world.resource::<Level>().mw.0.sea_level;
        let c = spawn_species(&mut h.world, &sp, 0.5, 0.5, 77);
        h.world.get_mut::<ActiveUntil>(c).unwrap().0 = u64::MAX;
        h.world.resource_mut::<crate::goals::Entered>().0.push(c);
        for x in -8..=8 {
            for z in -8..=8 {
                h.set_block(x, sea - 4, z, "minecraft:stone");
                for y in sea - 3..=sea + 1 {
                    h.set_block(x, y, z, "minecraft:water");
                }
            }
        }
        for x in 6..=7 {
            for z in -1..=1 {
                h.set_block(x, sea - 4, z, "minecraft:stone");
                h.set_block(x, sea - 3, z, "minecraft:air");
                h.set_block(x, sea - 2, z, "minecraft:air");
            }
        }
        run(&mut h, 2);
        h.teleport(c, Vec3::new(0.5, f64::from(sea - 3), 0.5));
        run(&mut h, 4);
        Some((h, c, sea))
    }
    use horizon_core::geometry::Vec3;
    fn skip(name: &str) -> bool {
        if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
            eprintln!("skipped {name}: HORIZON_TEST_RESOURCES unset");
            return true;
        }
        false
    }

    /// At night, in the water, with a beach in reach: the goal starts after the first countdown, and the walk aims at the beach.
    #[test]
    fn at_night_a_drowned_in_the_water_starts_for_a_beach_in_reach() {
        if skip("at_night_a_drowned_in_the_water_starts_for_a_beach_in_reach") {
            return;
        }
        let (mut h, c, _) = world().unwrap();
        h.set_day(false);
        let mut started = false;
        for _ in 0..1500 {
            step(&mut h);
            started |= running(&h, c);
            if started {
                break;
            }
        }
        assert!(started, "témoin : le départ vers la plage, de nuit, dans l'eau");
        let b = h.world.get::<Beach>(c).unwrap().walk.block;
        assert!((6..=7).contains(&b[0]) && (-1..=1).contains(&b[2]), "la plage trouvée est celle du rivage : {b:?}");
        assert!(h.world.get::<crate::locomotion::swim::WantsToSwim>(c).is_none(), "la marche vers la plage ne nage pas");
    }

    /// By day nothing starts; the same drowned starts at the next night: the night is the witness.
    #[test]
    fn by_day_the_drowned_does_not_go_to_the_beach() {
        if skip("by_day_the_drowned_does_not_go_to_the_beach") {
            return;
        }
        let (mut h, c, _) = world().unwrap();
        h.set_day(true);
        run(&mut h, 1200);
        assert!(!running(&h, c), "pas de départ le jour");
        h.set_day(false);
        let mut started = false;
        for _ in 0..1500 {
            step(&mut h);
            started |= running(&h, c);
            if started {
                break;
            }
        }
        assert!(started, "témoin : le départ à la nuit suivante");
    }

    /// The walk ends when its beach is gone: `can_continue` (MoveToBlock's block is still valid) fails at the next tick.
    #[test]
    fn the_walk_ends_when_the_beach_is_gone() {
        if skip("the_walk_ends_when_the_beach_is_gone") {
            return;
        }
        let (mut h, c, sea) = world().unwrap();
        h.set_day(false);
        let mut started = false;
        for _ in 0..1500 {
            step(&mut h);
            started |= running(&h, c);
            if started {
                break;
            }
        }
        assert!(started, "témoin : le départ");
        let b = h.world.get::<Beach>(c).unwrap().walk.block;
        h.set_block(b[0], b[1], b[2], "minecraft:water");
        step(&mut h);
        step(&mut h);
        assert!(!running(&h, c), "la marche s'arrête quand la plage disparaît (plage à y {})", sea - 4);
    }
}
