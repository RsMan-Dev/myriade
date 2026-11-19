//! `Ghast.GhastLookGoal` (fiche 37, `nether.rs:307-336`): a LOOK goal that can always start and ticks every tick. Without a
//! target within 64 blocks the body faces the motion; with one, it faces the target. It holds the LOOK flag, so the goal
//! runs from its first deadline; when a higher goal takes the flag it is asked again at the next tick.
use super::*;
use crate::cow::*;
use crate::goals::*;
use crate::targeting::{Target, Targets};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// `distance_sqr < 4096` (64 blocks) to the target.
    pub range_sqr: f64,
}
impl Params {
    pub const DEFAULT: Self = Self { range_sqr: 4096.0 };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct GhastGaze {
    pub range_sqr: f64,
    pub slot: u8,
}
/// The goal runs.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct GhastLooking;

pub static MODULE: Module = Module {
    name: "ghast_look",
    flags: LOOK,
    equip,
    arm,
    can_use,
    start,
    stop,
    tick: Some(tick),
    requires: &[Cap::Body, Cap::Goals, Cap::Head, Cap::Locomotion],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, inst: Inst, p: &(dyn std::any::Any + Sync)) {
    let p = params::<Params>(p);
    w.entity_mut(e).insert(GhastGaze { range_sqr: p.range_sqr, slot: inst.slot });
}
fn arm(w: &mut World, e: Entity, inst: Inst) {
    crate::goals::arm(w, e, inst.slot, 1);
}
fn can_use(_: &mut World, _: Entity, _: Inst) -> bool {
    true
}
fn start(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).insert(GhastLooking);
}
fn stop(w: &mut World, e: Entity, inst: Inst) {
    w.entity_mut(e).remove::<GhastLooking>();
    crate::goals::arm(w, e, inst.slot, 1);
}
/// The tick of the module: its system, run by `arbitrate` while an animal carries it (no system in the schedule).
fn tick(w: &mut World) {
    crate::goals::tick_state(w, look_run);
}
fn deg() -> f32 {
    180.0_f32 / std::f32::consts::PI
}
/// `GhastLookGoal.tick`: the body (and the yaw) faces the target within range, else the motion.
fn look_run(
    tg: Targets,
    mut q: Query<(&Pos, &Vel, &mut Yaw, &mut Head, &GhastGaze, Option<&Target>), With<GhastLooking>>,
) {
    for (pos, vel, mut yaw, mut head, g, target) in &mut q {
        let info = target.and_then(|t| tg.info(t.who));
        let angle = match info {
            None => Some(horizon_core::geometry::trig::atan2(vel.0.x, vel.0.z)),
            Some(i) => {
                let (dx, dy, dz) = (i.pos.x - pos.0.x, i.pos.y - pos.0.y, i.pos.z - pos.0.z);
                (dx * dx + dy * dy + dz * dz < g.range_sqr).then(|| horizon_core::geometry::trig::atan2(dx, dz))
            }
        };
        if let Some(a) = angle {
            yaw.0 = -(a as f32) * deg();
            head.body = yaw.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cow::{Herd, SpeciesTable, spawn_species};
    use crate::species::Config;
    use crate::targeting::{Target, set_target};

    static ENTRIES: [Entry; 1] = [Entry::new(&MODULE, 7, &Params::DEFAULT)];

    fn herd() -> Option<Herd> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = Herd::with_slab(30, false, false);
        set_lazy_look(&mut h.world, false);
        Some(h)
    }
    fn ghast(h: &mut Herd, x: f64, z: f64) -> Entity {
        let mut sp = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == "minecraft:cow").unwrap();
        sp.name = "minecraft:ghast";
        sp.goals = Config { entries: &ENTRIES };
        let e = spawn_species(&mut h.world, &sp, x, z, 51);
        h.world.flush();
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        h.world.get_mut::<crate::targeting::FollowRange>(e).unwrap().0 = 128.0;
        arm_timers(&mut h.world, e);
        e
    }
    fn ticks(h: &mut Herd, n: usize) {
        for _ in 0..n {
            h.tick();
        }
    }
    fn running(h: &Herd, e: Entity) -> bool {
        h.world.get::<GoalSet>(e).unwrap().is_running(&MODULE)
    }

    /// With no target the yaw follows the motion (a velocity to +x gives -90 degrees); a target 30 blocks away on the
    /// diagonal turns it to -45 degrees; a target beyond 64 blocks leaves the yaw alone.
    #[test]
    fn the_ghast_faces_its_target_within_64_blocks_and_its_motion_otherwise() {
        let Some(mut h) = herd() else { return };
        let e = ghast(&mut h, 0.5, 0.5);
        ticks(&mut h, 2);
        assert!(running(&h, e), "the look goal runs from its first deadline");
        h.world.get_mut::<Vel>(e).unwrap().0 = horizon_core::geometry::Vec3::new(1.0, 0.0, 0.0);
        ticks(&mut h, 1);
        let yaw = h.world.get::<Yaw>(e).unwrap().0;
        assert!((yaw + 90.0).abs() < 1e-3, "motion to +x: yaw -90, got {yaw}");
        let t = ghast(&mut h, 30.5, 30.5);
        let mut tt = Target::new(crate::targeting::Who::Entity(t), 100, u8::MAX);
        tt.must_see = false;
        set_target(&mut h.world, e, tt);
        ticks(&mut h, 1);
        let yaw = h.world.get::<Yaw>(e).unwrap().0;
        assert!((yaw + 45.0).abs() < 1e-3, "target 42 blocks away on the diagonal: yaw -45, got {yaw}");
        h.world.get_mut::<Yaw>(e).unwrap().0 = 123.0;
        let far = ghast(&mut h, 80.5, 80.5);
        let mut tf = Target::new(crate::targeting::Who::Entity(far), 100, u8::MAX);
        tf.must_see = false;
        set_target(&mut h.world, e, tf);
        ticks(&mut h, 1);
        assert_eq!(h.world.get::<Yaw>(e).unwrap().0, 123.0, "a target beyond 64 blocks: the yaw is unchanged");
    }
}
