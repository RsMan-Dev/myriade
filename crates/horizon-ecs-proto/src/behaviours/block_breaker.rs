//! The wither's block breaking (`WitherBoss.customServerAiStep`, `counter2`): an accepted hurt that finds the counter at 0 starts a
//! 20-tick countdown (one wheel deadline, `Due::Goal`, no tick). At its end, if the level lets it grief, the animal breaks the
//! cells of its box: x and z from `floor(w/2 + 1)` each side, y from its feet up `floor(h)`. Air and `_air` are skipped, the
//! `#wither_immune` blocks only when the level's answer says so. A liquid does not count as a destroyed block (the sound).
use super::*;
use crate::cow::*;
use crate::goals::*;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub delay: i32,
}
impl Params {
    pub const WITHER: Self = Self { delay: 20 };
}
/// The countdown's deadline (0 when none is running) and the goal's slot.
#[derive(Component, Clone, Copy, Debug)]
pub struct Breaker {
    slot: u8,
    p: Params,
    due: u64,
}
pub static MODULE: Module = Module {
    name: "block_breaker",
    flags: 0,
    equip,
    fired,
    can_use,
    start,
    requires: &[Cap::Body],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, inst: Inst, p: &(dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Breaker { slot: inst.slot, p: *params::<Params>(p), due: 0 });
}
/// An accepted hurt (`WitherBoss.accepts`): the countdown starts unless one runs.
pub fn hurt(w: &mut World, e: Entity) {
    let now = now_of(w, e);
    let Some(b) = w.get::<Breaker>(e).copied() else { return };
    if b.due > now {
        return;
    }
    let due = now + b.p.delay.max(0) as u64;
    w.get_mut::<Breaker>(e).unwrap().due = due;
    arm_at(w, e, due, Due::Goal(b.slot));
}
fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    w.get::<Breaker>(e).is_some_and(|b| b.due > 0 && now_of(w, e) >= b.due)
}
fn can_use(_: &mut World, _: Entity, _: Inst) -> bool {
    true
}
fn start(w: &mut World, e: Entity, inst: Inst) {
    let Some(b) = w.get::<Breaker>(e).copied() else { return };
    if b.due == 0 || now_of(w, e) < b.due {
        return;
    }
    w.get_mut::<Breaker>(e).unwrap().due = 0;
    if crate::level::can_grief(w, e) {
        breaks(w, e);
    }
    w.resource_mut::<Requests>().0.push(Req::End(e, inst.slot));
}
fn breaks(w: &mut World, e: Entity) {
    let (pos, width, height) = (
        w.get::<Pos>(e).map_or(horizon_core::geometry::Vec3::ZERO, |p| p.0),
        w.get::<Bounds>(e).map_or(0.0, |b| b.0.size().x),
        w.get::<Bounds>(e).map_or(0.0, |b| b.0.size().y),
    );
    let half = (width / 2.0 + 1.0).floor() as i32;
    let high = height.floor() as i32;
    let b = [pos.x.floor() as i32, pos.y.floor() as i32, pos.z.floor() as i32];
    let mut destroyed = false;
    for x in b[0] - half..=b[0] + half {
        for y in b[1]..=b[1] + high {
            for z in b[2] - half..=b[2] + half {
                let name = crate::level::block_name(w, [x, y, z]);
                if name == "minecraft:air" || name.ends_with("_air") {
                    continue;
                }
                let immune = crate::level::has_tag(w, "minecraft:wither_immune", name);
                if !crate::level::can_entity_destroy(w, e, [x, y, z], !immune) {
                    continue;
                }
                crate::level::break_block(w, [x, y, z], false, None, Some(e));
                destroyed |= !crate::level::is_liquid(w, [x, y, z]);
            }
        }
    }
    if destroyed {
        let at = [pos.x, pos.y, pos.z];
        w.resource_mut::<Events>().host.sound("entity.wither.break_block", at, 1.0, 1.0);
    }
}
