//! The rideable capability (`Cap::Rideable`): an entity that can carry a rider. The state is one component, `Mount` (who rides it, the
//! rider's input as the host writes it each tick, the jump strength and the temper); the rider of a creature is `Riding(mount)`; a
//! player of the bench rides by its id. Being ridden is the frame's occupation `OCC_MOUNTED`. Mounting is an interaction (the rule
//! `RIDE`); dismounting a service. A ridden mount takes its rider's steering each tick (`ridden_drive`, only the ridden ones), keeps its
//! rider's player at the seat, and raises the events `mounted` and `dismounted`. Nothing here is about one species.
use crate::cow::*;
use crate::goals::*;
use crate::rules::{no_attrs, params, Rule};
use crate::targeting::Who;
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;

/// What the rider asks of its mount this tick (`Rider`: the steering, the look, the jump bar).
#[derive(Clone, Copy, Debug, Default)]
pub struct RiderInput {
    pub forward: f32,
    pub strafe: f32,
    pub yaw: f32,
    pub pitch: f32,
    /// `playerJumpPendingScale`: 0 to 1, taken by the mount when it is on the ground.
    pub jump: f32,
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Mount {
    pub rider: Option<Who>,
    pub input: RiderInput,
    /// `AbstractHorse`'s jump strength (the attribute), and the highest temper (`getMaxTemper`).
    pub jump_strength: f32,
    pub max_temper: i32,
    /// `AbstractHorse.getTemper`, between 0 and `max_temper`.
    pub temper: i32,
    /// `AbstractHorse.isSaddled`: only a saddled mount takes its rider's steering (`getControllingPassenger`).
    pub saddled: bool,
}
impl Default for Mount {
    fn default() -> Self {
        Self { rider: None, input: RiderInput::default(), jump_strength: 0.7, max_temper: 100, temper: 0, saddled: false }
    }
}
/// A creature that rides: its mount.
#[derive(Component, Clone, Copy, Debug)]
pub struct Riding(pub Entity);
/// A mount that has a rider: the drive system visits it.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Ridden;

/// The parameters of the riding rule.
#[derive(Clone, Copy, Debug)]
pub struct RideParams {
    pub jump_strength: f32,
    pub max_temper: i32,
}
impl RideParams {
    pub const HORSE: Self = Self { jump_strength: 0.7, max_temper: 100 };
}
pub static RIDE: Rule = Rule { name: "ride", equip, attrs: no_attrs, interact };

pub fn register(s: &mut bevy_ecs::schedule::Schedule) {
    s.add_systems(ridden_drive.after(move_control).before(travel));
}
fn equip(w: &mut World, e: Entity, p: &(dyn std::any::Any + Sync)) {
    let p = *params::<RideParams>(p);
    if let Some(mut m) = w.get_mut::<Mount>(e) {
        (m.jump_strength, m.max_temper) = (p.jump_strength, p.max_temper);
    }
}
/// `AbstractHorse.mobInteract` with an empty hand or an item it does not eat: the player rides an adult that carries nobody.
fn interact(w: &mut World, e: Entity, _: &(dyn std::any::Any + Sync), i: &crate::behaviours::Interaction) -> bool {
    if w.get::<GrowAt>(e).is_some() || w.get::<Mount>(e).is_none_or(|m| m.rider.is_some()) {
        return false;
    }
    let _ = i.item;
    ride(w, e, Who::Player(i.player))
}
/// `Entity.startRiding`: `who` takes the seat of `mount` (false when it carries somebody already).
pub fn ride(w: &mut World, mount: Entity, who: Who) -> bool {
    let Some(mut m) = w.get_mut::<Mount>(mount) else { return false };
    if m.rider.is_some() {
        return false;
    }
    m.rider = Some(who);
    m.input = RiderInput::default();
    if let Who::Entity(r) = who {
        w.entity_mut(r).insert(Riding(mount));
    }
    occupy(w, mount, OCC_MOUNTED);
    w.entity_mut(mount).insert((Ridden, Moving));
    stop_nav(w, mount);
    crate::targeting::clear_target(w, mount);
    w.resource_mut::<Events>().mounted.push(mount);
    true
}
/// `Entity.stopRiding`: the rider gets off (a player is set down where the mount stands).
pub fn dismount(w: &mut World, mount: Entity) -> Option<Who> {
    let who = w.get_mut::<Mount>(mount)?.rider.take()?;
    match who {
        Who::Entity(r) => {
            w.entity_mut(r).remove::<Riding>();
        }
        Who::Player(id) => {
            if let (Some(p), Some(at)) = (w.resource::<Online>().get(id), w.get::<Pos>(mount).map(|p| p.0)) {
                w.resource_mut::<Online>().set(Player { x: at.x + 1.0, y: at.y, z: at.z, ..p });
            }
        }
    }
    release(w, mount, OCC_MOUNTED);
    w.entity_mut(mount).remove::<Ridden>();
    w.resource_mut::<Events>().dismounted.push(mount);
    Some(who)
}
pub fn rider_of(w: &World, mount: Entity) -> Option<Who> {
    w.get::<Mount>(mount)?.rider
}
/// `AbstractHorse.modifyTemper`: added, then held between 0 and the highest.
pub fn modify_temper(w: &mut World, e: Entity, amount: i32) {
    if let Some(mut m) = w.get_mut::<Mount>(e) {
        m.temper = (m.temper + amount).clamp(0, m.max_temper);
    }
}
/// Each tick, the ridden mounts: the rider's steering is the mount's input (`AbstractHorse.tickRidden` and `getRiddenInput`), its jump
/// (`executeRidersJump`), its yaw; the player rider stays at the seat.
#[allow(clippy::type_complexity)]
fn ridden_drive(
    mut q: Query<(Entity, &mut Mount, &mut Drive, &mut Yaw, &Contact, &Body, &Pos, &mut Vel), With<Ridden>>,
    mut on: ResMut<Online>,
    mut commands: Commands,
) {
    for (e, mut m, mut d, mut yaw, c, body, pos, mut vel) in &mut q {
        if !m.saddled {
            continue;
        }
        let r = m.input;
        yaw.0 = r.yaw;
        let forward = if r.forward <= 0.0 { r.forward * 0.25 } else { r.forward };
        d.speed = body.speed as f32;
        d.xxa = r.strafe * 0.5;
        d.zza = forward;
        if c.on_ground && r.jump > 0.0 {
            let s = r.yaw.to_radians();
            let mut v = vel.0;
            v.y = f64::from(m.jump_strength * r.jump);
            if forward > 0.0 {
                v.x += f64::from(-0.4 * s.sin() * r.jump);
                v.z += f64::from(0.4 * s.cos() * r.jump);
            }
            vel.0 = v;
            m.input.jump = 0.0;
        }
        if let Some(Who::Player(id)) = m.rider
            && let Some(p) = on.get(id)
        {
            let seat = Vec3::new(pos.0.x, pos.0.y + f64::from(body.height) * 0.75, pos.0.z);
            if (p.x, p.y, p.z) != (seat.x, seat.y, seat.z) {
                on.set(Player { x: seat.x, y: seat.y, z: seat.z, ..p });
            }
        }
        commands.entity(e).insert(Moving);
    }
}
