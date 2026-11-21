//! `PanicGoal`: after a damage of the types of `#panic_causes`, or while burning, run to a random spot (to the water
//! when burning and one is within 5 blocks).
use super::*;
use crate::{cow::*, goals::*};
use horizon_core::geometry::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The speed modifier of the run.
    pub speed: f64,
    /// The spot's range around the animal: horizontal and vertical.
    pub h: i32,
    pub v: i32,
    /// How long after the last damage the goal can start (ticks).
    pub window: u64,
    /// `looking_for_water(range)` (the turtle's 7): water within that many blocks is the spot, burning or not.
    pub water: Option<i32>,
    /// Only the environment panics it (`environmental`: cactus, fire, lava, freezing: the armadillo and the panda); a hit by a creature does not.
    pub environmental: bool,
}
impl Params {
    pub const DEFAULT: Self = Self { speed: 2.0, h: 5, v: 4, window: 40, water: None, environmental: false };
}
/// The parameters, and the end of the window after the last damage; `pick` is what `can_use` chose for `start`:
/// (x, y, z) of the target and whether it is water.
#[derive(Component, Clone, Copy, Debug)]
pub struct Panic {
    /// The species' parameters, referenced (read through `Deref`): never copied into the entity.
    p: &'static Params,
    pub until: u64,
    pub slot: u8,
    pick: (f64, i32, f64),
}
impl std::ops::Deref for Panic {
    type Target = Params;
    fn deref(&self) -> &Params {
        self.p
    }
}
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Panicking;

pub static MODULE: Module = Module {
    name: "panic",
    flags: MOVE,
    ends_with_path: true,
    retry_on_end: true,
    resources,
    equip,
    can_use,
    start,
    stop,
    systems,
    requires: &[Cap::Living, Cap::Locomotion],
    ..Module::DEFAULT
};
/// The animals hurt this tick, in the order the damage was applied (written by `apply_damage`).
#[derive(Resource, Default)]
pub struct Hurts(pub Vec<Entity>);
fn resources(w: &mut World) {
    w.insert_resource(Hurts::default());
}
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = params::<Params>(p);
    w.entity_mut(e).insert(Panic { p, until: 0, slot: inst.slot, pick: (0.0, 1, 0.0) });
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems(hurt_events.after(apply_damage).in_set(slot));
}
/// Damage that makes the animal panic (`DamageType.causes_panic`): call from the damage code.
pub fn hurt(world: &mut World, e: Entity) {
    world.resource_mut::<Hurts>().0.push(e);
}
/// `PanicGoal.canUse` after a damage: only the types of `#panic_causes` (a fall, a drowning, a wall do not), then a
/// window of 40 ticks; a burning animal panics as long as it burns.
pub(crate) fn hurt_events(
    clock: Res<Clock>,
    mut h: ResMut<Hurts>,
    causes: Res<HurtCauses>,
    mut q: Query<&mut Panic>,
    mut reqs: ResMut<Requests>,
) {
    for (i, e) in std::mem::take(&mut h.0).into_iter().enumerate() {
        let panics = causes.0.get(i).filter(|c| c.0 == e).is_none_or(|c| !matches!(c.1, Cause::Fall | Cause::Drown | Cause::InWall | Cause::Cramming));
        let environment = causes.0.get(i).filter(|c| c.0 == e).is_none_or(|c| matches!(c.1, Cause::Cactus | Cause::InFire | Cause::OnFire | Cause::Lava | Cause::Freeze));
        if let Ok(mut p) = q.get_mut(e)
            && panics
            && (!p.environmental || environment)
        {
            p.until = clock.of(e) + p.window;
            reqs.0.push(Req::Start(e, p.slot));
        }
    }
}
fn can_use(w: &mut World, e: Entity, inst: Inst) -> bool {
    let now = crate::cow::now_of(w, e);
    let Some(pos) = w.get::<Pos>(e).map(|p| p.0) else { return false };
    let Some(p) = w.get::<Panic>(e).copied() else { return false };
    let burning = w.get::<Burning>(e).is_some_and(|b| b.until > now);
    if now > p.until && !burning {
        return false;
    }
    let pick = |w: &mut World, v: (f64, i32, f64)| w.get_mut::<Panic>(e).unwrap().pick = v;
    if let Some((x, z)) = take_xz(w, e, inst.slot) {
        pick(w, (x, 1, z));
        return true;
    }
    // `PanicGoal.lookForWater`: a burning cow looks for water within 5 blocks first (nearest first).
    if let Some(range) = p.water.or(burning.then_some(5))
        && let Some(spot) = water_spot(w, pos, range)
    {
        pick(w, spot);
        return true;
    }
    // `DefaultRandomPos.getPos(mob, h, v)`: the best of ten by the species' walk value (`spots.rs`).
    match crate::spots::pick_for(w, e, crate::spots::Pick::Default { h: p.h, v: p.v }) {
        Some([x, y, z]) => {
            pick(w, (x, y.floor() as i32, z));
            true
        }
        None => {
            // `canUse` asked again at the next selector tick, while the window lasts.
            arm(w, e, inst.slot, 2);
            false
        }
    }
}
/// `water_pos`: the water block nearest to the cow among the blocks within `range` on each side and one up or down,
/// unless the cow stands in a block with a collision shape.
fn water_spot(w: &World, pos: Vec3, range: i32) -> Option<(f64, i32, f64)> {
    use horizon_client_lib::blocks::Fluid;
    let mw = w.resource::<Level>().mw.0;
    let at = [pos.x.floor() as i32, pos.y.floor() as i32, pos.z.floor() as i32];
    if mw.data.state(mw.world.block(at[0], at[1], at[2])).top != 0.0 {
        return None;
    }
    mw.nearest_block(at, range, 1, |p, _| mw.fluid(p[0], p[1], p[2]) == Fluid::Water).map(|p| (f64::from(p[0]) + 0.5, p[1], f64::from(p[2]) + 0.5))
}
fn start(w: &mut World, e: Entity, _: Inst) {
    let p = *w.get::<Panic>(e).unwrap();
    let (x, y, z) = p.pick;
    w.entity_mut(e).insert(Panicking);
    w.get_mut::<Nav>(e).unwrap().speed = p.speed;
    let mut s = w.get_mut::<Stroll>(e).unwrap();
    (s.tx, s.ty, s.tz) = (x, y, z);
    crate::pathsearch::probe!(ASKS[1].fetch_add(1, std::sync::atomic::Ordering::Relaxed));
    w.entity_mut(e).insert(NeedsPath);
}
fn stop(w: &mut World, e: Entity, inst: Inst) {
    let now = crate::cow::now_of(w, e);
    w.entity_mut(e).remove::<Panicking>();
    // `canUse` is asked again at the next selector tick while the damage is recent or the animal burns.
    let until = w.get::<Panic>(e).unwrap().until;
    if now <= until || w.get::<Burning>(e).is_some_and(|b| b.until > now) {
        arm(w, e, inst.slot, 2);
    }
}
/// Sets the animal on fire for `ticks`: its damage every 20 ticks, and the panic that a burning animal has.
pub fn ignite(world: &mut World, e: Entity, ticks: u64) {
    let now = crate::cow::now_of(world, e);
    let already = world.get::<Burning>(e).is_some();
    world.entity_mut(e).insert(Burning { until: now + ticks });
    if !already {
        world.resource_mut::<Events>().ignited.push(e);
    }
    // `Entity.baseTick`: the fire hurts on the ticks where the remaining fire is a multiple of 20, the first included.
    crate::cow::arm_at(world, e, now + if ticks % 20 == 0 { 1 } else { 20 }, Due::Burn);
    wake(world, e, &MODULE);
}
