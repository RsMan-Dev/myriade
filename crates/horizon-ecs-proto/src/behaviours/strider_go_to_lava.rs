//! `Strider.StriderGoToLavaGoal` (fiche 36, `strider.rs:107-173`): a `MoveToBlockGoal` toward the nearest lava block with room
//! above it (8 blocks, 2 rows). Flags MOVE|JUMP. The countdown `reduced(200 + nextInt(200))` is drawn at each search and
//! at each stop (the `Mob` counts it down once per tick, which is a wheel deadline here). The walk aims at the lava block's
//! centre; the path is asked every 20 ticks of walking. DOUTE: the countdown does not freeze while the strider is in lava;
//! a deadline that comes up in lava waits for the exit (`left_fluid`) and then searches at once. Missing: the strider's
//! path map (lava walkable): the walk stops at the shore.
use super::*;
use crate::move_to_block::MoveToBlock;
use crate::{cow::*, goals::*};
use horizon_client_lib::mobs::MobWorld;
use horizon_client_lib::mobs::data::PathComputationType;
use horizon_core::geometry::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub speed: f64,
    pub range: i32,
    pub vertical: i32,
}
impl Params {
    pub const DEFAULT: Self = Self { speed: 1.0, range: 8, vertical: 2 };
}
/// The search state: the block found, and `blocked`: a deadline came up in lava (the search waits for the exit).
#[derive(Component, Clone, Debug)]
pub struct Lava {
    pub block: MoveToBlock,
    pub slot: u8,
    pub blocked: bool,
}
/// The goal runs.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct GoingToLava;

pub static MODULE: Module = Module {
    name: "strider_go_to_lava",
    flags: MOVE | JUMP,
    equip,
    arm,
    can_use,
    start,
    stop,
    tick: Some(tick),
    requires: &[Cap::Body, Cap::Goals, Cap::Locomotion],
    ..Module::DEFAULT
};
fn reduced(n: i32) -> i32 {
    (n + 1) / 2
}
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = params::<Params>(p);
    w.entity_mut(e).insert(Lava { block: MoveToBlock::new(p.speed, p.range, p.vertical), slot: inst.slot, blocked: false });
}
/// The first search is at once (the `Mob`'s countdown starts at zero).
fn arm(w: &mut World, e: Entity, inst: Inst) {
    crate::goals::arm(w, e, inst.slot, 1);
}
fn countdown(w: &mut World, e: Entity, slot: u8) {
    let wait = with_rng(w, e, |r| reduced(200 + r.next_int(200)));
    crate::goals::arm(w, e, slot, wait);
}
/// `StriderGoToLavaGoal.isValidTarget`: lava with room to stand above.
fn lava_target(mw: &MobWorld, p: [i32; 3]) -> bool {
    mw.block_name(p[0], p[1], p[2]) == "minecraft:lava" && mw.data.pathfindable(mw.world.block(p[0], p[1] + 1, p[2]), PathComputationType::Land)
}
fn block_of(p: Vec3) -> [i32; 3] {
    [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32]
}
fn can_use(w: &mut World, e: Entity, inst: Inst) -> bool {
    if w.get::<FluidState>(e).is_some_and(|f| f.in_lava) {
        w.get_mut::<Lava>(e).unwrap().blocked = true;
        return false;
    }
    let base = block_of(w.get::<Pos>(e).unwrap().0);
    let mw = w.resource::<Level>().mw.0;
    let mut block = w.get::<Lava>(e).unwrap().block.clone();
    let found = block.may_find(base, mw, |s| &*mw.facts.get(s).name == "minecraft:lava") && block.find_nearest(base, mw, lava_target);
    w.get_mut::<Lava>(e).unwrap().block = block;
    if found {
        return true;
    }
    countdown(w, e, inst.slot);
    false
}
fn start(w: &mut World, e: Entity, _: Inst) {
    let mut block = w.get::<Lava>(e).unwrap().block.clone();
    with_rng(w, e, |r| block.start(r));
    let b = block.block;
    let speed = block.speed;
    w.get_mut::<Lava>(e).unwrap().block = block;
    go_to_at(w, e, [f64::from(b[0]) + 0.5, f64::from(b[1] + 1), f64::from(b[2]) + 0.5], speed, 1);
    w.entity_mut(e).insert(GoingToLava);
}
fn stop(w: &mut World, e: Entity, inst: Inst) {
    w.entity_mut(e).remove::<GoingToLava>();
    countdown(w, e, inst.slot);
}
/// The ticks of the module (no system in the schedule), in the order the chain had: run by `arbitrate` while an animal carries the goal.
fn tick(w: &mut World) {
    crate::goals::tick_state(w, lava_exit);
    crate::goals::tick_state(w, lava_run);
}
/// A strider that left a fluid and had a search pending in it searches at once.
fn lava_exit(ev: Res<Events>, q: Query<&Lava>, mut cmd: Commands) {
    for e in ev.left_fluid.iter() {
        if q.get(e).is_ok_and(|l| l.blocked) {
            let slot = q.get(e).unwrap().slot;
            cmd.queue(move |w: &mut World| {
                w.get_mut::<Lava>(e).unwrap().blocked = false;
                crate::goals::arm(w, e, slot, 1);
            });
        }
    }
}
/// `StriderGoToLavaGoal.canContinueToUse` and `tick` (the centre of the lava block; a path every 20 ticks).
fn lava_run(level: Res<Level>, mut q: Query<(Entity, &Pos, &FluidState, &mut Lava), With<GoingToLava>>, mut reqs: ResMut<Requests>, mut cmd: Commands) {
    let mw = level.mw.0;
    for (e, pos, fluid, mut l) in &mut q {
        let b = l.block.block;
        if fluid.in_lava || !lava_target(mw, b) {
            reqs.0.push(Req::End(e, l.slot));
            continue;
        }
        let c = [f64::from(b[0]) + 0.5, f64::from(b[1]) + 0.5, f64::from(b[2]) + 0.5];
        let d2 = (pos.0.x - c[0]).powi(2) + (pos.0.y - c[1]).powi(2) + (pos.0.z - c[2]).powi(2);
        if d2 >= 1.0 {
            l.block.reached = false;
            l.block.try_ticks += 1;
            if l.block.try_ticks % 20 == 0 {
                let at = [c[0], f64::from(b[1]), c[2]];
                let speed = l.block.speed;
                cmd.queue(move |w: &mut World| go_to_at(w, e, at, speed, 1));
            }
        } else {
            l.block.reached = true;
            l.block.try_ticks -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cow::{Herd, SpeciesTable, spawn_species};
    use crate::species::Config;

    static ENTRIES: [Entry; 1] = [Entry::new(&MODULE, 4, &Params::DEFAULT)];

    fn herd(hazards: bool) -> Option<Herd> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = Herd::with_slab_awake(30, false, hazards);
        set_lazy_look(&mut h.world, false);
        Some(h)
    }
    /// A cow named a strider, at x on the floor, with the goal; the lava pool is one block at (lx, y, lz) in the floor.
    fn strider(h: &mut Herd, x: f64) -> Entity {
        let mut sp = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == "minecraft:cow").unwrap();
        sp.name = "minecraft:strider";
        sp.goals = Config { entries: &ENTRIES };
        let e = spawn_species(&mut h.world, &sp, x, 0.5, 41);
        h.world.flush();
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        arm_timers(&mut h.world, e);
        e
    }
    fn running(h: &Herd, e: Entity) -> bool {
        h.world.get::<GoalSet>(e).unwrap().is_running(&MODULE)
    }
    fn ticks(h: &mut Herd, n: usize) {
        for _ in 0..n {
            h.tick();
        }
    }
    /// The floor is at the strider's row: the lava block sits in the floor 6 blocks away.
    fn floor_y(h: &Herd, e: Entity) -> i32 {
        h.world.get::<Pos>(e).unwrap().0.y.floor() as i32 - 1
    }

    /// A lava block 6 blocks away with air above: the strider starts walking to it at once, and its path leads to the
    /// block above the lava; with the lava gone the goal ends (the positive witness is the start).
    #[test]
    fn a_strider_walks_to_the_nearest_lava_and_stops_when_it_is_gone() {
        let Some(mut h) = herd(false) else { return };
        let e = strider(&mut h, 0.5);
        let y = floor_y(&h, e);
        h.set_block(6, y, 0, "minecraft:lava");
        ticks(&mut h, 4);
        assert!(running(&h, e), "a lava block with room above: the walk starts");
        let lava = h.world.get::<Lava>(e).unwrap().block.block;
        assert_eq!(lava, [6, y, 0], "the nearest lava is the one placed");
        let s = h.world.get::<Stroll>(e).unwrap();
        assert!((s.tx - 6.5).abs() < 1e-6 && (s.tz - 0.5).abs() < 1e-6, "the path leads to the lava: {} {}", s.tx, s.tz);
        h.set_block(6, y, 0, "minecraft:air");
        ticks(&mut h, 4);
        assert!(!running(&h, e), "lava gone: the goal ends");
    }

    /// The walk starts out of lava (the positive witness), and ends as soon as the strider stands in lava.
    #[test]
    fn a_walk_ends_when_the_strider_stands_in_lava() {
        let Some(mut h) = herd(true) else { return };
        let e = strider(&mut h, 0.5);
        let y = floor_y(&h, e);
        h.set_block(6, y, 0, "minecraft:lava");
        ticks(&mut h, 4);
        assert!(running(&h, e), "out of lava, lava 6 blocks away: the walk starts");
        h.set_block(0, y + 1, 0, "minecraft:lava");
        ticks(&mut h, 4);
        assert!(!running(&h, e), "in lava: the walk ends");
    }
}
