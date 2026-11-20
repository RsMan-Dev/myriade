//! The happy ghast's look (`happy_ghast.rs` `look_control`, 301-329): while a player is on it, or within the still countdown, it squares
//! to the world (yaw brought to the nearest 90 degrees, head with it); otherwise it faces its motion. Holds the LOOK flag.
//! DOUTE : le choix entre cette visée et `ghast_look` (les deux prennent LOOK) n'est pas tranché : le socle devra les départager.
//! DOUTE : `atan2` du cadre (`horizon_core::geometry::trig`) supposé en radians ; à vérifier à la compilation.
use super::still_under_riders::{Above, StillUntil};
use super::*;
use crate::cow::*;
use crate::goals::*;

#[derive(Clone, Copy, Debug)]
pub struct Params;
impl Params {
    pub const DEFAULT: Self = Self;
}

pub static MODULE: Module = Module {
    name: "square_or_face",
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

fn equip(_: &mut World, _: Entity, _: Inst, _: &'static (dyn std::any::Any + Sync)) {}
fn arm(w: &mut World, e: Entity, inst: Inst) {
    crate::goals::arm(w, e, inst.slot, 1);
}
fn can_use(_: &mut World, _: Entity, _: Inst) -> bool {
    true
}
fn start(_: &mut World, _: Entity, _: Inst) {}
fn stop(w: &mut World, e: Entity, inst: Inst) {
    arm(w, e, inst);
}
/// The tick of the module: its system, run by `arbitrate` while an animal carries it.
fn tick(w: &mut World) {
    crate::goals::tick_state(w, look_run);
}
/// Squared to the nearest quarter turn while still; else the motion's heading.
fn look_run(clock: Res<Clock>, mut q: Query<(Entity, &Vel, &mut Yaw, &mut Head, Option<&StillUntil>, Option<&Above>)>) {
    for (e, vel, mut yaw, mut head, still, above) in &mut q {
        let stilled = above.is_some() || still.is_some_and(|u| clock.of(e) < u.0);
        if stilled {
            let mut close = yaw.0 % 90.0;
            if close >= 45.0 {
                close -= 90.0;
            }
            if close < -45.0 {
                close += 90.0;
            }
            yaw.0 -= close;
        } else {
            let to_deg = 180.0_f32 / std::f32::consts::PI;
            yaw.0 = -(horizon_core::geometry::trig::atan2(vel.0.x, vel.0.z) as f32) * to_deg;
        }
        head.yaw = yaw.0;
        head.body = yaw.0;
    }
}
