//! `BackUpIfTooClose` (`mobs/piglin.rs:1287-1327`, `PiglinAi.initFightActivity` behind a crossbow): with no walk to make and the
//! target in sight within `range` blocks, the head looks at the target's eye, the body follows the head, and the animal strafes
//! `strafe` forwards every tick (negative: backwards). No flags, so the crossbow goal keeps aiming meanwhile; its marker is what ticks it.
use super::*;
use crate::rules::back_up_if_too_close::too_close as in_sight_too_close;
use crate::targeting::{self, Target};
use crate::{cow::*, goals::*};
use horizon_core::geometry::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The distance under which the target makes the animal back up, the forward input of the strafe.
    pub range: f64,
    pub strafe: f32,
}
impl Params {
    pub const DEFAULT: Self = Self { range: 5.0, strafe: -0.75 };
}
/// The goal's state: its parameters, its slot, and the walkable cache of `strafe_drive`.
#[derive(Component, Clone, Copy, Debug)]
pub struct BackUp {
    p: &'static Params,
    slot: u8,
    walkable: Option<((i32, i32, i32), bool, u64)>,
}
/// The goal runs.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Backing;

pub static MODULE: Module = Module {
    name: "back_up_if_too_close",
    flags: 0,
    equip,
    resources,
    fired,
    can_use,
    start,
    stop,
    tick: Some(tick),
    requires: &[Cap::Body, Cap::Goals, Cap::Head, Cap::Locomotion, Cap::Targets],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(BackUp { p: params::<Params>(p), slot: inst.slot, walkable: None });
}
/// A new target asks the goal at once; `fired` then asks again at every tick while the animal has a target (the `Mob` asks
/// the behaviour at every tick of its brain).
fn resources(w: &mut World) {
    w.resource_mut::<targeting::TargetWatchers>().0.push(|w, e| {
        if let Some(b) = w.get::<BackUp>(e) {
            let slot = b.slot;
            crate::goals::arm(w, e, slot, 1);
        }
    });
}
fn fired(w: &mut World, e: Entity, inst: Inst) -> bool {
    if w.get::<Target>(e).is_none() {
        return false;
    }
    crate::goals::arm(w, e, inst.slot, 1);
    true
}
/// The target is in sight and closer than `range` blocks (`closer_than` of the target's feet, `Mob.sees`).
fn too_close(p: &Params, pos: Vec3, t: &Target, i: &targeting::Info) -> bool {
    in_sight_too_close(p.range, t.seen, pos.distance_squared(i.pos))
}
/// `canUse`: no walk to make, and the target too close.
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let (Some(b), Some(t), Some(pos)) = (w.get::<BackUp>(e), w.get::<Target>(e), w.get::<Pos>(e)) else { return false };
    let Some(i) = targeting::info(w, t.who) else { return false };
    w.get::<Following>(e).is_none() && w.get::<NeedsPath>(e).is_none() && too_close(b.p, pos.0, t, &i)
}
fn start(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).insert(Backing);
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Backing>();
    // `MoveControl.update` takes the strafe back each tick: the last forward input does not outlast the goal.
    if let Some(mut d) = w.get_mut::<Drive>(e) {
        d.zza = 0.0;
        d.xxa = 0.0;
    }
}
/// The tick of the module: its system, run by `arbitrate` while an animal carries it.
fn tick(w: &mut World) {
    crate::goals::tick_state(w, back_run);
}
/// `BackUpIfTooClose.tick`, every tick of the goal: the goal ends when `canContinueToUse` fails.
#[allow(clippy::type_complexity)]
fn back_run(
    tg: targeting::Targets,
    level: Res<Level>,
    clock: Res<Clock>,
    mut q: Query<(Aim, &mut BackUp, &mut Drive, &mut Yaw, &Head, &Body, Option<&Target>), With<Backing>>,
    mut reqs: ResMut<Requests>,
    mut cmd: Commands,
) {
    for ((e, pos, mut look, looking, following, asked), mut b, mut drive, mut yaw, head, body, target) in &mut q {
        let i = match target.and_then(|t| tg.info(t.who).map(|i| (t, i))) {
            Some((t, i)) if !(following || asked) && too_close(b.p, pos.0, t, &i) => i,
            _ => {
                reqs.0.push(Req::End(e, b.slot));
                continue;
            }
        };
        aim(&mut cmd, e, &mut look, looking, [i.pos.x, i.eye, i.pos.z], None);
        let mut walkable = b.walkable;
        strafe_drive(&level, &Pos(pos.0), &yaw, body, &mut drive, b.p.strafe, 0.0, &mut walkable, clock.of(e));
        b.walkable = walkable;
        // `Mob.rotateIfNecessary(yaw, headYaw, 0)`: the body takes the head's yaw.
        yaw.0 = head.yaw;
    }
}
