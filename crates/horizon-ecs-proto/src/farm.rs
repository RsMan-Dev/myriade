//! Farm-animal behaviours beyond the common goals, as data and events (suivi/ecs-ecarts.md): the sheep's grazing
//! (`EatBlockGoal`) and wool, and the hen's egg and wings. A species takes a behaviour by having its data in
//! `FarmDef`; nothing here knows a species by name.
use crate::cow::*;
use crate::goals::Kind;
use bevy_ecs::prelude::*;

/// What a species has of the behaviours of this file.
#[derive(Clone, Copy, Debug)]
pub struct FarmDef {
    pub graze: Option<GrazeDef>,
    pub wool: bool,
    /// The egg timer of the hen.
    pub egg: Option<EggDef>,
    /// A fall in the air has its downward speed multiplied by this each tick (the hen's glide; 1 for the others).
    pub slow_fall: f64,
    /// `causeFallDamage` answers no (the hen).
    pub fall_immune: bool,
    /// The wing state of the hen (`flap`, `flapSpeed`, `flapping`), render-only.
    pub flaps: bool,
}
impl Default for FarmDef {
    fn default() -> Self {
        Self { graze: None, wool: false, egg: None, slow_fall: 1.0, fall_immune: false, flaps: false }
    }
}
/// `Chicken.eggTime`: `base + nextInt(range)` ticks, counted only while an adult.
#[derive(Clone, Copy, Debug)]
pub struct EggDef {
    pub base: i32,
    pub range: i32,
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
        "minecraft:sheep" => FarmDef { graze: Some(GrazeDef { adult_asks: 500, baby_asks: 25, ticks: 40, eat_at: 34 }), wool: true, ..FarmDef::default() },
        "minecraft:chicken" => FarmDef { egg: Some(EggDef { base: 6000, range: 6000 }), slow_fall: 0.6, fall_immune: true, flaps: true, ..FarmDef::default() },
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
    if let Some(d) = sp.farm.egg {
        let left = {
            let mut r = world.get_mut::<Rng>(e).unwrap();
            r.next_int(d.range) + d.base
        };
        world.entity_mut(e).insert(Egg { def: d, left: left as u64, at: u64::MAX });
        // The timer counts only for an adult: a newborn's is armed when it grows (`Grown`).
        if world.get::<GrowAt>(e).is_none() {
            arm_egg(world, e);
        }
    }
    if sp.farm.flaps {
        world.entity_mut(e).insert(Flap::default());
    }
}
/// The hen's egg: `left` ticks of an adult's life until the next one; `at` is the tick it is due (MAX: not counting).
#[derive(Component, Clone, Copy, Debug)]
pub struct Egg {
    pub def: EggDef,
    pub left: u64,
    pub at: u64,
}
/// The eggs laid (animal, tick): the egg item itself and the sound are out of scope (no item entities here).
#[derive(Resource, Default)]
pub struct LaidEggs(pub Vec<(Entity, u64)>);
/// The babies that grew up this tick (written by `due`).
#[derive(Resource, Default)]
pub struct Grown(pub Vec<Entity>);
fn arm_egg(w: &mut World, e: Entity) {
    let now = w.resource::<Clock>().0;
    let Some(mut egg) = w.get_mut::<Egg>(e) else { return };
    egg.at = now + egg.left;
    let at = egg.at;
    w.resource_mut::<Wheel>().push(at.min(now + 4095), e, Due::Goal(Kind::LayEgg));
}
/// A deadline of the egg timer: lay (an adult), start again, or wait for the growth (a baby does not count).
pub fn lay_deadline(w: &mut World, e: Entity) {
    let now = w.resource::<Clock>().0;
    let Some(egg) = w.get::<Egg>(e).copied() else { return };
    if egg.at == u64::MAX || now < egg.at {
        // A deadline of the wheel's reach, not the egg's.
        if egg.at != u64::MAX && now < egg.at {
            let at = egg.at;
            w.resource_mut::<Wheel>().push(at.min(now + 4095), e, Due::Goal(Kind::LayEgg));
        }
        return;
    }
    if w.get::<GrowAt>(e).is_some() {
        w.get_mut::<Egg>(e).unwrap().at = u64::MAX;
        return;
    }
    w.resource_mut::<LaidEggs>().0.push((e, now));
    let left = {
        let d = egg.def;
        let mut r = w.get_mut::<Rng>(e).unwrap();
        r.next_int(d.range) + d.base
    };
    w.get_mut::<Egg>(e).unwrap().left = left as u64;
    arm_egg(w, e);
}
/// The babies that grew up start counting their egg.
pub fn on_grown(w: &mut World) {
    for e in std::mem::take(&mut w.resource_mut::<Grown>().0) {
        if w.get::<Egg>(e).is_some_and(|g| g.at == u64::MAX) {
            arm_egg(w, e);
        }
    }
}
/// The wings: `flapSpeed` rises in the air and falls on the ground, `flapping` restarts at 1 in the air and decays, and
/// `flap` turns by it (`Chicken.aiStep`); render-only state. Kept while the physics of the hen is active; when it goes
/// to rest, what the decay of the wings would still add is added at once (`flapping` is geometric: the sum is exact).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Flap {
    pub flap: f32,
    pub speed: f32,
    pub flapping: f32,
    /// The wings have been settled (not stepped since the last rest).
    pub settled: bool,
}
fn flap_one(f: &mut Flap, on_ground: bool) {
    f.speed = (f.speed + if on_ground { -1.0 } else { 4.0 } * 0.3).clamp(0.0, 1.0);
    if !on_ground && f.flapping < 1.0 {
        f.flapping = 1.0;
    }
    f.flapping *= 0.9;
    f.flap += f.flapping * 2.0;
    f.settled = false;
}
/// The hen's wings, one step, before the physics of the tick (`Chicken.aiStep` runs before `travel`).
pub fn flap_system(
    mut q: Query<(&Contact, &mut Flap), With<Moving>>,
    mut removed: RemovedComponents<Moving>,
    mut rest: Query<(&Contact, &mut Flap), Without<Moving>>,
) {
    for (c, mut f) in &mut q {
        flap_one(&mut f, c.on_ground);
    }
    // A hen whose physics went inactive: the rest of its wings' decay at once.
    for e in removed.read() {
        if let Ok((c, mut f)) = rest.get_mut(e)
            && c.on_ground
            && !f.settled
        {
            f.speed = 0.0;
            f.flap += f.flapping * 0.9 * 2.0 / (1.0 - 0.9);
            f.flapping = 0.0;
            f.settled = true;
        }
    }
}
/// Registers the systems of this file.
pub fn register(s: &mut bevy_ecs::schedule::Schedule) {
    s.add_systems(flap_system.after(crate::cow::move_control).before(crate::cow::travel));
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

/// Sets the ticks of an adult until the hen's next egg (a test imposes the `Mob`'s draw).
pub fn set_egg(w: &mut World, e: Entity, left: u64) {
    w.get_mut::<Egg>(e).unwrap().left = left;
    if w.get::<GrowAt>(e).is_none() {
        arm_egg(w, e);
    }
}
