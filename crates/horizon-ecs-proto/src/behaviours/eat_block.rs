//! `EatBlockGoal` (the sheep): now and then, on short grass or a grass block, lower the head for `ticks`; the block goes
//! `eat_at` ticks in (short grass is broken, a grass block becomes dirt), the wool grows back and a baby grows by a minute.
//! The ask and the two phases of the goal are deadlines of the wheel.
use super::*;
use crate::{cow::*, goals::*};

/// One chance in `adult_asks` (`baby_asks` for a baby) per ask, an ask every 2 ticks; the head is down for `ticks`, and
/// the block is eaten `eat_at` ticks into it. The vanilla values: `reduced(1000)` = 500 asks, `reduced(50)` = 25,
/// `reduced(40)` = 20 selector ticks of animation of which the block goes at the 18th: 34 ticks in.
#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub adult_asks: i32,
    pub baby_asks: i32,
    pub ticks: u64,
    pub eat_at: u64,
}
impl Params {
    pub const DEFAULT: Self = Self { adult_asks: 500, baby_asks: 25, ticks: 40, eat_at: 34 };
}
/// The data of the grazing, on the animal; `armed`: its ask is in the wheel.
#[derive(Component, Clone, Copy, Debug)]
pub struct Graze {
    pub def: &'static Params,
    pub armed: bool,
    pub slot: u8,
}
/// The animal is grazing: when the block goes, when the head comes up, whether the block went.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Grazing {
    pub eat_at: u64,
    pub end_at: u64,
    pub eaten: bool,
}

pub static MODULE: Module = Module {
    name: "eat_block",
    flags: MOVE | LOOK | JUMP,
    blocked: Some(blocked),
    equip,
    arm: arm_graze_inst,
    fired,
    can_use,
    start,
    stop,
    requires: &[Cap::Locomotion],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Graze { def: params::<Params>(p), armed: false, slot: inst.slot });
}
fn arm_graze_inst(w: &mut World, e: Entity, _: Inst) {
    arm_graze(w, e);
}
fn blocked(w: &mut World, e: Entity, _: Inst) {
    arm_graze(w, e);
}
/// A deadline of a grazing animal (the block, the end) or an ask of one that does not.
fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    if w.get::<Grazing>(e).is_some() {
        graze_deadline(w, e);
        return false;
    }
    if let Some(mut g) = w.get_mut::<Graze>(e) {
        g.armed = false;
    }
    true
}
fn block_at(w: &World, x: i32, y: i32, z: i32) -> &'static str {
    let mw = w.resource::<Level>().mw.0;
    &mw.facts.get(mw.world.block(x, y, z)).name
}
fn feet(w: &World, e: Entity) -> [i32; 3] {
    let p = w.get::<Pos>(e).unwrap().0;
    [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32]
}
fn edible(w: &World, name: &str) -> bool {
    w.resource::<Level>().mw.0.has_tag("minecraft:edible_for_sheep", name)
}
/// Arms the next ask of the grazing goal (a geometric wait of asks of 2 ticks: the law of one draw per ask).
pub fn arm_graze(w: &mut World, e: Entity) {
    let Some(g) = w.get::<Graze>(e).copied() else { return };
    if g.armed {
        return;
    }
    let d = g.def;
    let baby = w.get::<GrowAt>(e).is_some();
    let n = if baby { d.baby_asks } else { d.adult_asks };
    let wait = {
        let mut r = w.get_mut::<Rng>(e).unwrap();
        2 * geometric(&mut r, n)
    };
    arm(w, e, g.slot, wait);
    w.get_mut::<Graze>(e).unwrap().armed = true;
}
/// `EatBlockGoal.canUse` once the draw has come up: the block at its feet is short grass (or a fern), or the one under
/// it is a grass block. Otherwise the next ask is armed.
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let [x, y, z] = feet(w, e);
    let here = block_at(w, x, y, z);
    let ok = edible(w, here) || block_at(w, x, y - 1, z) == "minecraft:grass_block";
    if !ok {
        arm_graze(w, e);
    }
    ok
}
/// `EatBlockGoal.start`: the navigation stops, the head goes down; its two deadlines go in the wheel.
fn start(w: &mut World, e: Entity, inst: Inst) {
    stop_nav(w, e);
    let now = crate::cow::now_of(w, e);
    let d = w.get::<Graze>(e).unwrap().def;
    w.entity_mut(e).insert(Grazing { eat_at: now + d.eat_at, end_at: now + d.ticks, eaten: false });
    occupy(w, e, OCC_GRAZING);
    let clock = w.resource::<Clock>();
    let (a, b) = (clock.at(e, now + d.eat_at), clock.at(e, now + d.ticks));
    let mut wheel = w.resource_mut::<Wheel>();
    wheel.push(a, e, Due::Goal(inst.slot));
    wheel.push(b, e, Due::Goal(inst.slot));
}
/// `EatBlockGoal.stop`.
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Grazing>();
    release(w, e, OCC_GRAZING);
    arm_graze(w, e);
}
/// A deadline of a grazing animal: the block goes (`EatBlockGoal.tick`), or the head comes up.
fn graze_deadline(w: &mut World, e: Entity) {
    let now = crate::cow::now_of(w, e);
    let Some(g) = w.get::<Grazing>(e).copied() else { return };
    if now >= g.end_at {
        let slot = w.get::<Graze>(e).unwrap().slot;
        end_goal(w, e, slot);
    } else if now >= g.eat_at && !g.eaten {
        w.get_mut::<Grazing>(e).unwrap().eaten = true;
        eat(w, e);
        w.resource_mut::<Events>().ate.push(e);
    }
}
/// The block goes: short grass is broken, a grass block under it becomes dirt (both through the block-change event of
/// the world); then `Mob.ate`: the wool grows back and a baby grows by a minute.
fn eat(w: &mut World, e: Entity) {
    let [x, y, z] = feet(w, e);
    let here = block_at(w, x, y, z);
    let change = if edible(w, here) {
        Some((x, y, z, "minecraft:air"))
    } else if block_at(w, x, y - 1, z) == "minecraft:grass_block" {
        Some((x, y - 1, z, "minecraft:dirt"))
    } else {
        None
    };
    if let Some(c) = change {
        w.resource_mut::<BlockChanges>().0.push(c);
        ate(w, e);
    }
}
/// `Sheep.ate`: the wool grows back; a baby ages up 60 seconds (`AgeableMob.ageUp(60)`).
fn ate(w: &mut World, e: Entity) {
    if let Some(mut wool) = w.get_mut::<crate::farm::Wool>(e) {
        wool.sheared = false;
    }
    let now = crate::cow::now_of(w, e);
    if let Some(mut g) = w.get_mut::<GrowAt>(e) {
        g.0 = g.0.saturating_sub(1200).max(now + 1);
        let at = g.0;
        crate::cow::arm_at(w, e, at, Due::Grow);
    }
}
