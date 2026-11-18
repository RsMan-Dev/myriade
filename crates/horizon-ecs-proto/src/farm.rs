//! Farm-animal behaviours beyond the common goals, as data and events (suivi/ecs-ecarts.md): the sheep's grazing
//! (`EatBlockGoal`) and wool, and the hen's egg and wings. A species takes a behaviour by having its data in
//! `FarmDef`; nothing here knows a species by name.
use crate::cow::*;
use crate::goals::Kind;
use bevy_ecs::prelude::*;

/// What a species has of the behaviours of this file.
#[derive(Clone, Copy, Debug, Default)]
pub struct FarmDef {
    pub graze: Option<GrazeDef>,
    pub wool: bool,
}
/// `EatBlockGoal`: one chance in `adult_asks` (`baby_asks` for a baby) per ask, an ask every 2 ticks; the head is down
/// for `ticks`, and the block is eaten `eat_at` ticks into it.
#[derive(Clone, Copy, Debug)]
pub struct GrazeDef {
    pub adult_asks: i32,
    pub baby_asks: i32,
    pub ticks: u64,
    pub eat_at: u64,
}
pub fn farm_def(name: &str) -> FarmDef {
    match name {
        // `reduced(1000)` = 500 asks, `reduced(50)` = 25, `reduced(40)` = 20 selector ticks of animation of which the
        // block goes at the 18th (`tick == reduced(4)`): 34 ticks in.
        "minecraft:sheep" => FarmDef { graze: Some(GrazeDef { adult_asks: 500, baby_asks: 25, ticks: 40, eat_at: 34 }), wool: true },
        _ => FarmDef::default(),
    }
}
/// The data of the species' grazing, on the animal.
#[derive(Component, Clone, Copy, Debug)]
pub struct Graze(pub GrazeDef);
/// The animal is grazing: when the block goes, when the head comes up, whether the block went.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Grazing {
    pub eat_at: u64,
    pub end_at: u64,
    pub eaten: bool,
}
/// The wool: `sheared` until the animal eats again, and the colour (data, set at spawn).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Wool {
    pub sheared: bool,
    pub color: u8,
}
/// Adds to an animal the components of the farm behaviours of its species.
pub fn equip(world: &mut World, e: Entity, sp: &Species) {
    if let Some(g) = sp.farm.graze {
        world.entity_mut(e).insert(Graze(g));
    }
    if sp.farm.wool {
        world.entity_mut(e).insert(Wool::default());
    }
}
/// Shearing (`Sheep.shear`): an adult with wool loses it; the drops and the sound are out of scope. Returns whether it
/// was shorn.
pub fn shear(world: &mut World, e: Entity) -> bool {
    if world.get::<crate::cow::GrowAt>(e).is_some() {
        return false;
    }
    match world.get_mut::<Wool>(e) {
        Some(mut w) if !w.sheared => {
            w.sheared = true;
            true
        }
        _ => false,
    }
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
    let Some(Graze(d)) = w.get::<Graze>(e).copied() else { return };
    let baby = w.get::<GrowAt>(e).is_some();
    let n = if baby { d.baby_asks } else { d.adult_asks };
    let wait = {
        let mut r = w.get_mut::<Rng>(e).unwrap();
        2 * geometric(&mut r, n)
    };
    let now = w.resource::<Clock>().0;
    w.resource_mut::<Wheel>().push((now + wait.max(1) as u64).min(now + 4095), e, Due::Goal(Kind::EatBlock));
    w.get_mut::<crate::goals::GoalTimers>(e).unwrap().graze = true;
}
/// `EatBlockGoal.canUse` once the draw has come up: the block at its feet is short grass (or a fern), or the one under
/// it is a grass block. Otherwise the next ask is armed.
pub fn graze_can_use(w: &mut World, e: Entity) -> bool {
    let [x, y, z] = feet(w, e);
    let here = block_at(w, x, y, z);
    let ok = edible(w, here) || block_at(w, x, y - 1, z) == "minecraft:grass_block";
    if !ok {
        arm_graze(w, e);
    }
    ok
}
/// `EatBlockGoal.start`: the head goes down; its two deadlines go in the wheel.
pub fn graze_start(w: &mut World, e: Entity) {
    let now = w.resource::<Clock>().0;
    let d = w.get::<Graze>(e).unwrap().0;
    w.entity_mut(e).insert(Grazing { eat_at: now + d.eat_at, end_at: now + d.ticks, eaten: false });
    let mut wheel = w.resource_mut::<Wheel>();
    wheel.push(now + d.eat_at, e, Due::Goal(Kind::EatBlock));
    wheel.push(now + d.ticks, e, Due::Goal(Kind::EatBlock));
}
/// `EatBlockGoal.stop`.
pub fn graze_stop(w: &mut World, e: Entity) {
    w.entity_mut(e).remove::<Grazing>();
    arm_graze(w, e);
}
/// A deadline of a grazing animal: the block goes (`EatBlockGoal.tick`), or the head comes up.
pub fn graze_deadline(w: &mut World, e: Entity) {
    let now = w.resource::<Clock>().0;
    let Some(g) = w.get::<Grazing>(e).copied() else { return };
    if now >= g.end_at {
        crate::goals::end_goal(w, e, Kind::EatBlock);
    } else if now >= g.eat_at && !g.eaten {
        w.get_mut::<Grazing>(e).unwrap().eaten = true;
        eat(w, e);
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
    if let Some(mut wool) = w.get_mut::<Wool>(e) {
        wool.sheared = false;
    }
    let now = w.resource::<Clock>().0;
    if let Some(mut g) = w.get_mut::<GrowAt>(e) {
        g.0 = g.0.saturating_sub(1200).max(now + 1);
        let at = g.0;
        w.resource_mut::<Wheel>().push(at.min(now + 4095), e, Due::Grow);
    }
}
