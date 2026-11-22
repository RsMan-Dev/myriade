//! A player walking into a sulfur cube that holds a block shoves it (`SulfurCube.player_push`, `playerTouch`): within `reach` blocks
//! horizontally and overlapping its height, the cube gets the push `speed = clamp(|v| * 2 * speed_factor, 0, speed_max)` scaled by the
//! archetype's knockback resistance (`push_factor`), the vertical part only on the ground; the push sound plays once per cool-down
//! when the push is strong. Event-driven: a `PlayerWatch` wakes the goal when a player enters; while that player stays inside, the goal
//! re-arms each tick for this one player (no scan of the entities), and ends when the player leaves.
//! DOUTE : `Player` n'a ni vitesse, ni état vivant, ni mode spectateur dans le cadre : `PlayerTouch` (socle) les fournira.
//! DOUTE : la brûlure du joueur (`sulfur_cube_hot`) est faite par `burn_on_touch` ; ici seulement la poussée.
use super::swallow_hold::Held;
use super::*;
use crate::cow::*;
use crate::goals::*;
use horizon_core::geometry::Vec3;
use crate::rules::push_on_touch::{knockback_factor, push_speed, push_vector};

/// The shove's constants (species): the reach, the speed factor and cap, the lift on the ground.
#[derive(Component, Clone, Copy, Debug)]
pub struct Params {
    pub reach: f64,
    pub speed_factor: f64,
    pub speed_max: f64,
    pub lift: f64,
}
impl Params {
    pub const DEFAULT: Self = Self { reach: 1.3, speed_factor: 0.3, speed_max: 0.5, lift: 0.3 };
}
/// The player who pushes the cube now, and the own time before which the push sound waits (`push_cooldown`).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Pusher {
    pub player: u32,
    pub sound_after: u64,
}

pub static MODULE: Module = Module {
    name: "push_on_touch",
    flags: 0,
    equip,
    arm,
    can_use,
    start,
    stop,
    tick: Some(tick),
    requires: &[Cap::Body, Cap::Goals],
    ..Module::DEFAULT
};

fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = *params::<Params>(p);
    w.entity_mut(e).insert((PlayerWatch::new(p.reach as f32), p));
}
/// The module's tick, called only while a creature that carries it lives (no registered system).
fn tick(w: &mut World) {
    crate::goals::tick_state(w, entered);
}
/// A player enters the watch of a cube that holds a block: the goal starts for that player.
fn entered(ev: Res<Events>, mut reqs: ResMut<Requests>, q: Query<&Held>, mut commands: Commands) {
    for (e, player) in ev.player_entered.iter() {
        if q.get(e).is_ok() {
            commands.entity(e).insert(Pusher { player, sound_after: 0 });
            reqs.0.push(Req::Start(e, 0));
        }
    }
}
fn arm(w: &mut World, e: Entity, inst: Inst) {
    crate::goals::arm(w, e, inst.slot, 1);
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    w.get::<Pusher>(e).is_some() && w.get::<Held>(e).is_some()
}
/// One push of this tick from the watched player; the goal ends when that player is no longer inside.
fn start(w: &mut World, e: Entity, inst: Inst) {
    let Some(pusher) = w.get::<Pusher>(e).copied() else { return };
    let Some(held) = w.get::<Held>(e).map(|h| h.archetype) else { return };
    let p = w.resource::<Online>().get(pusher.player);
    let inside = w.get::<PlayerWatch>(e).is_some_and(|pw| pw.is_inside(pusher.player));
    let (Some(p), true) = (p, inside) else {
        return end_now(w, e, inst);
    };
    let Some(pos) = w.get::<Pos>(e).map(|x| x.0) else { return };
    let params = w.get::<Params>(e).copied().unwrap_or(Params::DEFAULT);
    let kb = knockback_factor(held.knockback_resistance);
    let speed = push_speed(player_speed(w, pusher.player), params.speed_factor, params.speed_max);
    let on_ground = w.get::<Contact>(e).is_some_and(|c| c.on_ground);
    let [px, py, pz] = push_vector(pos.x - p.x, pos.z - p.z, kb, speed, params.lift, on_ground);
    let push = Vec3::new(px, py, pz);
    let strong = f64::from(push.length()) > f64::from(held.push_threshold);
    let now = now_of(w, e);
    if strong && now >= pusher.sound_after {
        let at = [pos.x, pos.y, pos.z];
        w.resource_mut::<crate::goals::Events>().host.sound(held.push_sound, at, 1.0, 1.0);
        if let Some(mut pu) = w.get_mut::<Pusher>(e) {
            pu.sound_after = now + (held.push_cooldown * 20.0) as u64;
        }
    }
    if let Some(mut v) = w.get_mut::<Vel>(e) {
        v.0 = v.0.add_vector(push);
    }
}
/// The player's speed (`Entity.getDeltaMovement().length()`): the frame's `Player` has no velocity yet.
/// DOUTE : vitesse du joueur absente du cadre ; à fournir par le socle (`PlayerTouch`). Nulle en attendant : pas de poussée.
fn player_speed(_: &World, _: u32) -> f64 {
    0.0
}
fn end_now(w: &mut World, e: Entity, inst: Inst) {
    w.entity_mut(e).remove::<Pusher>();
    w.resource_mut::<Requests>().0.push(Req::End(e, inst.slot));
}
fn stop(w: &mut World, e: Entity, inst: Inst) {
    if w.get::<Pusher>(e).is_some() {
        arm(w, e, inst);
    }
}
