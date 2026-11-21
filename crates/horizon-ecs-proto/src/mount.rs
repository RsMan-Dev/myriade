//! The rideable capability (`Cap::Rideable`): an entity that can carry a rider. The state is one component, `Mount` (who rides it, the
//! rider's input as the host writes it each tick, the jump strength, the temper, and HOW it is driven: `Steer`); the rider of a creature is
//! `Riding(mount)`; a player of the bench rides by its id. Being ridden is the frame's occupation `OCC_MOUNTED`. Mounting is an
//! interaction (the rule `RIDE`); dismounting a service. A ridden mount takes its rider's steering each tick (`ridden_drive`, only the
//! ridden ones), keeps its riders at their seats, and raises the events `mounted` and `dismounted`.
//! The drive is a PARAMETER of the mount (`Steer`): the jump bar of the horse (`Charged`), the stick of the pig and the strider
//! (`Stick`: a fixed forward at a fraction of the speed, a boost drawn at each use of the stick, only for a rider holding it), the dash
//! of the camel (`Dash`: a push along the look, a cooldown, a sprint bonus, no input while it refuses to move). A mount has one seat
//! or more (`seats`: the forward offset of each; the first rider steers, the others are passengers). A CREATURE rider steers the
//! mount through the navigation: its goals' `go_to_at` / `stop_nav` act on the mount (`nav_owner`), the mount's own movement goals
//! are disabled while it is steered by a creature, and the mount walks to the rider's target. Nothing here is about one species.
use crate::cow::*;
use crate::goals::*;
use crate::rules::{no_attrs, params, Rule};
use crate::targeting::Who;
use bevy_ecs::prelude::*;
use horizon_core::geometry::trig::Mth;
use horizon_core::geometry::Vec3;

/// What the rider asks of its mount this tick (`Rider`: the steering, the look, the jump bar, the sprint, the stick).
#[derive(Clone, Copy, Debug, Default)]
pub struct RiderInput {
    pub forward: f32,
    pub strafe: f32,
    pub yaw: f32,
    pub pitch: f32,
    /// `playerJumpPendingScale`: 0 to 1, taken by the mount when it can use it.
    pub jump: f32,
    /// The rider sprints (the camel runs faster).
    pub sprint: bool,
    /// The rider holds the mount's steering stick (carrot or fungus on a stick): a pig and a strider obey only then.
    pub steer: bool,
}
/// `Player.jumpRidingTicks` to the scale of the jump bar (`Player.getJumpRidingScale`): the host that counts the ticks of a held jump key
/// writes `RiderInput.jump` with it.
pub fn jump_bar(ticks: u32) -> f32 {
    if ticks < 10 { ticks as f32 * 0.1 } else { 0.8 + 2.0 / (ticks - 9) as f32 * 0.1 }
}
/// How a mount takes its rider's input.
#[derive(Clone, Copy, Debug)]
pub enum Steer {
    /// `AbstractHorse.tickRidden`: forward, a quarter of it backwards, half of the strafe; the jump bar lifts it by `jump_strength * bar`
    /// (and pushes it forward by 0,4 * bar when walking forward).
    Charged,
    /// `Pig` / `Strider`: always forward along the look at `factor` of the movement speed, times the boost of the stick: during
    /// `boost_min + nextInt(boost_range)` ticks the speed is multiplied by `1 + boost_gain * sin(pi * elapsed / total)`.
    Stick { factor: f64, boost_min: u32, boost_range: i32, boost_gain: f32 },
    /// `Camel.tickRidden`: the jump is a dash (`push` of the speed along the look, `lift` * `lift_scale` up) then `cooldown` ticks in which
    /// the pressed jump is thrown away; the sprint adds `sprint_bonus` to the speed outside the cooldown; no input while it refuses
    /// to move (`posture`), and a forward input stands a sitting one up.
    Dash { push: f64, lift: f64, lift_scale: f64, cooldown: u64, sprint_bonus: f32 },
}
impl Steer {
    pub const PIG: Self = Self::Stick { factor: 0.225, boost_min: 140, boost_range: 841, boost_gain: 1.15 };
    pub const STRIDER: Self = Self::Stick { factor: 0.55, boost_min: 140, boost_range: 841, boost_gain: 1.15 };
    pub const CAMEL: Self = Self::Dash { push: 22.2222, lift: 1.4285, lift_scale: 0.42, cooldown: 55, sprint_bonus: 0.1 };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Mount {
    pub rider: Option<Who>,
    /// The second seat and the following ones (`seats.len()` in all): never steer.
    pub passenger: Option<Who>,
    pub input: RiderInput,
    /// `AbstractHorse`'s jump strength (the attribute), and the highest temper (`getMaxTemper`): 0 for a mount that is not an `AbstractHorse`.
    pub jump_strength: f32,
    pub max_temper: i32,
    /// `AbstractHorse.getTemper`, between 0 and `max_temper`.
    pub temper: i32,
    /// `AbstractHorse.isSaddled`: only a saddled mount takes its rider's steering (`getControllingPassenger`).
    pub saddled: bool,
    pub steer: Steer,
    /// The forward offset of each seat (the first rider's, then the passenger's), and the height of a seat as a fraction of the body.
    pub seats: &'static [f32],
    pub seat_height: f32,
    /// The own time from which a `Steer::Dash` may dash again.
    pub ready_at: u64,
    /// The creature that rides keeps its distance from its target (a shooter: `ranged_*` goals).
    pub rider_ranged: bool,
    /// A player may take a seat by interaction (a spider or a chicken carries only the creatures put on it).
    pub by_player: bool,
}
impl Default for Mount {
    fn default() -> Self {
        Self { rider: None, passenger: None, input: RiderInput::default(), jump_strength: 0.7, max_temper: 0, temper: 0, saddled: false, steer: Steer::Charged, seats: &[0.0], seat_height: 0.75, ready_at: 0, rider_ranged: false, by_player: true }
    }
}
/// The stick of a pig or strider was used: the boost runs (`elapsed` of `total` own ticks).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Boosted {
    pub elapsed: u32,
    pub total: u32,
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
    pub steer: Steer,
    pub seats: &'static [f32],
    pub seat_height: f32,
    pub by_player: bool,
}
impl RideParams {
    pub const HORSE: Self = Self { jump_strength: 0.7, max_temper: 100, steer: Steer::Charged, seats: &[0.0], seat_height: 0.75, by_player: true };
    /// A creature that only carries creatures (the spider, the chicken).
    pub const CARRIER: Self = Self { by_player: false, max_temper: 0, ..Self::HORSE };
    pub const PIG: Self = Self { steer: Steer::PIG, max_temper: 0, ..Self::HORSE };
    pub const STRIDER: Self = Self { steer: Steer::STRIDER, max_temper: 0, ..Self::HORSE };
    /// `Camel`: two seats (`camel.rs:258-281`: the first 0,5 ahead, the second 0,7 behind).
    pub const CAMEL: Self = Self { steer: Steer::CAMEL, seats: &[0.5, -0.7], ..Self::HORSE };
}
pub static RIDE: Rule = Rule { name: "ride", equip, attrs: no_attrs, interact };

pub fn register(s: &mut bevy_ecs::schedule::Schedule) {
    s.add_systems(ridden_drive.after(move_control).before(travel));
}
fn equip(w: &mut World, e: Entity, p: &'static (dyn std::any::Any + Sync)) {
    let p = *params::<RideParams>(p);
    if let Some(mut m) = w.get_mut::<Mount>(e) {
        (m.jump_strength, m.max_temper, m.steer, m.seats, m.seat_height, m.by_player) = (p.jump_strength, p.max_temper, p.steer, p.seats, p.seat_height, p.by_player);
    }
}
/// `AbstractHorse.mobInteract` with an empty hand or an item it does not eat: the player rides an adult with a free seat (a stick
/// mount needs its saddle: without it nobody takes the seat).
fn interact(w: &mut World, e: Entity, _: &'static (dyn std::any::Any + Sync), i: &crate::behaviours::Interaction) -> bool {
    let Some(m) = w.get::<Mount>(e) else { return false };
    // A saddle or a body armor in hand is for `equipment::EQUIP`, a lead for `leash::LEASH`.
    if matches!(crate::items::worn_in(i.item), Some(Slot::Saddle | Slot::Body)) || i.item == lead() || crate::leash::holder_of(w, e) == Some(crate::leash::Holder::Player(i.player)) || !m.by_player || w.get::<GrowAt>(e).is_some() || (matches!(m.steer, Steer::Stick { .. }) && !m.saddled) {
        return false;
    }
    let _ = i.item;
    ride(w, e, Who::Player(i.player))
}
/// `Entity.startRiding`: `who` takes a seat of `mount` (the first, which steers, or the next free one; false when all are taken).
pub fn ride(w: &mut World, mount: Entity, who: Who) -> bool {
    let Some(mut m) = w.get_mut::<Mount>(mount) else { return false };
    let first = m.rider.is_none();
    if first {
        m.rider = Some(who);
        m.input = RiderInput::default();
    } else if m.seats.len() > 1 && m.passenger.is_none() {
        m.passenger = Some(who);
    } else {
        return false;
    }
    if let Who::Entity(r) = who {
        let ranged = is_ranged(w, r);
        w.entity_mut(r).insert(Riding(mount));
        if first {
            w.get_mut::<Mount>(mount).unwrap().rider_ranged = ranged;
        }
    }
    if first {
        occupy(w, mount, OCC_MOUNTED);
        w.entity_mut(mount).insert((Ridden, Moving));
        stop_nav(w, mount);
        crate::targeting::clear_target(w, mount);
        if matches!(who, Who::Entity(_)) {
            control_flags(w, mount, true);
        }
    }
    w.resource_mut::<Events>().mounted.push(mount);
    true
}
/// The rider is a shooter: it keeps its distance (it has a ranged module).
fn is_ranged(w: &World, r: Entity) -> bool {
    use crate::behaviours::{burst_shot, charged_shot, ranged_attack, ranged_bow};
    w.get::<GoalSet>(r).is_some_and(|g| g.has(&ranged_bow::MODULE) || g.has(&ranged_attack::MODULE) || g.has(&burst_shot::MODULE) || g.has(&charged_shot::MODULE))
}
fn get_off(w: &mut World, mount: Entity, who: Who) {
    let at = w.get::<Pos>(mount).map(|p| p.0);
    match (who, at) {
        (Who::Entity(r), at) => {
            w.entity_mut(r).remove::<Riding>();
            if let (Some(at), Some(mut p)) = (at, w.get_mut::<Pos>(r)) {
                p.0 = Vec3::new(at.x + 1.0, at.y, at.z);
                w.entity_mut(r).insert(Moving);
            }
        }
        (Who::Player(id), Some(at)) => {
            if let Some(p) = w.resource::<Online>().get(id) {
                w.resource_mut::<Online>().set(Player { x: at.x + 1.0, y: at.y, z: at.z, ..p });
            }
        }
        _ => {}
    }
}
/// `Entity.stopRiding` of the first rider: it gets off (a player is set down where the mount stands); the passenger, if any, takes
/// the steering.
pub fn dismount(w: &mut World, mount: Entity) -> Option<Who> {
    let mut m = w.get_mut::<Mount>(mount)?;
    let who = m.rider.take()?;
    m.rider = m.passenger.take();
    m.input = RiderInput::default();
    let next = m.rider;
    get_off(w, mount, who);
    if let Some(mut d) = w.get_mut::<Drive>(mount) {
        d.air_speed = None;
    }
    if next.is_none() {
        release(w, mount, OCC_MOUNTED);
        w.entity_mut(mount).remove::<Ridden>();
    }
    control_flags(w, mount, matches!(next, Some(Who::Entity(_))));
    w.resource_mut::<Events>().dismounted.push(mount);
    Some(who)
}
/// The passenger of the second seat gets off.
pub fn dismount_passenger(w: &mut World, mount: Entity) -> Option<Who> {
    let who = w.get_mut::<Mount>(mount)?.passenger.take()?;
    get_off(w, mount, who);
    w.resource_mut::<Events>().dismounted.push(mount);
    Some(who)
}
pub fn rider_of(w: &World, mount: Entity) -> Option<Who> {
    w.get::<Mount>(mount)?.rider
}
/// A mount steered by a creature has no movement goal of its own (`Mob.updateControlFlags`: the rider's goals move it).
fn control_flags(w: &mut World, mount: Entity, steered: bool) {
    crate::goals::set_disabled_flags(w, mount, if steered { MOVE | LOOK | JUMP } else { 0 });
}
/// The entity whose navigation a creature's goals drive: its mount when it steers it (`Mob.getNavigation` of a rider), else itself.
pub fn nav_owner(w: &World, e: Entity) -> Entity {
    match w.get::<Riding>(e) {
        Some(Riding(m)) if w.get::<Mount>(*m).is_some_and(|t| t.rider == Some(Who::Entity(e))) => *m,
        _ => e,
    }
}
/// The creature that steers this mount, if any.
pub fn steering_creature(w: &World, mount: Entity) -> Option<Entity> {
    match w.get::<Mount>(mount)?.rider {
        Some(Who::Entity(r)) => Some(r),
        _ => None,
    }
}
/// `AbstractHorse.modifyTemper`: added, then held between 0 and the highest.
pub fn modify_temper(w: &mut World, e: Entity, amount: i32) {
    if let Some(mut m) = w.get_mut::<Mount>(e) {
        m.temper = (m.temper + amount).clamp(0, m.max_temper);
    }
}
/// `ItemBasedSteering.boost`: the stick was used; false while a boost runs or when the mount has no stick drive. The duration is drawn
/// from the mount's stream (`boost_min + nextInt(boost_range)` ticks). The host calls it on the use of the stick by the rider.
pub fn boost(w: &mut World, e: Entity) -> bool {
    let Some(Steer::Stick { boost_min, boost_range, .. }) = w.get::<Mount>(e).map(|m| m.steer) else { return false };
    if w.get::<Boosted>(e).is_some() {
        return false;
    }
    let total = boost_min + with_rng(w, e, |r| r.next_int(boost_range)) as u32;
    w.entity_mut(e).insert(Boosted { elapsed: 0, total });
    true
}
/// The seat of a rider: where the mount carries it (forward offset along the mount's yaw, a fraction of its height).
fn seat_at(pos: Vec3, yaw: f32, body: &Body, offset: f32, fraction: f32) -> Vec3 {
    let a = yaw.to_radians();
    let (dx, dz) = (f64::from(-a.msin() * offset), f64::from(a.mcos() * offset));
    Vec3::new(pos.x + dx, pos.y + f64::from(body.height * fraction), pos.z + dz)
}
/// A creature rider steers by the navigation: the mount goes to the rider's target (`riders::drive`): every 10 ticks or when its walk
/// is over; a shooter within 8 blocks stops instead.
fn creature_steer(w: &mut World, mount: Entity) {
    let Some(m) = w.get::<Mount>(mount).copied() else { return };
    let Some(Who::Entity(r)) = m.rider else { return };
    let Some(t) = w.get::<crate::targeting::Target>(r).map(|t| t.who) else { return };
    let Some(info) = crate::targeting::info(w, t) else { return };
    if info.exempt {
        return;
    }
    let Some(pos) = w.get::<Pos>(mount).map(|p| p.0) else { return };
    let d2 = (info.pos.x - pos.x).powi(2) + (info.pos.y - pos.y).powi(2) + (info.pos.z - pos.z).powi(2);
    if m.rider_ranged && d2 < 64.0 {
        stop_nav(w, mount);
        return;
    }
    let idle = w.get::<Following>(mount).is_none() && w.get::<NeedsPath>(mount).is_none();
    let beat = now_of(w, mount) + w.get::<Uid>(mount).map_or(0, |u| u.0);
    if idle || beat.is_multiple_of(10) {
        go_to_at(w, mount, [info.pos.x, info.pos.y, info.pos.z], 1.0, 0);
    }
}
/// Each tick, the ridden mounts: the rider's steering is the mount's input (`AbstractHorse.tickRidden` and `getRiddenInput`, `Pig` and
/// `Camel`: see `Steer`), its jump, its yaw; the riders stay at their seats; a creature rider steers by the navigation.
#[allow(clippy::type_complexity)]
fn ridden_drive(
    clock: Res<Clock>,
    mut q: Query<(Entity, &mut Mount, &mut Drive, &mut Yaw, &Contact, &Body, &Pos, &mut Vel, Option<&mut Boosted>, Option<&crate::behaviours::posture::Posture>, Option<&crate::behaviours::fly_steer::Params>), With<Ridden>>,
    targets: Query<(), With<crate::targeting::Target>>,
    level: Res<Level>,
    on: Res<Online>,
    mut held: Query<crate::leash::Held, crate::leash::HeldFilter>,
    holders: Query<(&Pos, &Bounds, Option<&Vel>, Option<&Yaw>), (Without<crate::leash::Leash>, Without<Ridden>)>,
    mut commands: Commands,
) {
    // The lead (no system of its own: this one runs every tick anyway).
    crate::leash::tick(&level, &on, &mut held, &holders, &mut commands);
    for (e, mut m, mut d, mut yaw, c, body, pos, mut vel, boosted, posture, flier) in &mut q {
        // A flying mount (`FlyingSteer`: its `Params` are the steer's constants) takes `fly_steer`'s drive instead of this one.
        if let Some(p) = flier {
            crate::behaviours::fly_steer::drive_one(&m, &yaw, &mut vel, p);
            continue;
        }
        let Some(first) = m.rider else {
            commands.entity(e).remove::<Ridden>();
            continue;
        };
        let r = m.input;
        let now = clock.of(e);
        d.air_speed = None;
        let mut controlled = false;
        match first {
            Who::Entity(rid) => {
                if targets.contains(rid) {
                    commands.queue(move |w: &mut World| creature_steer(w, e));
                }
            }
            Who::Player(_) if m.saddled => match m.steer {
                Steer::Charged => {
                    controlled = true;
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
                            v.x += f64::from(-0.4 * s.msin() * r.jump);
                            v.z += f64::from(0.4 * s.mcos() * r.jump);
                        }
                        vel.0 = v;
                        m.input.jump = 0.0;
                    }
                }
                Steer::Stick { factor, boost_gain, .. } if r.steer => {
                    controlled = true;
                    yaw.0 = r.yaw;
                    let mut boost = 1.0_f32;
                    if let Some(mut b) = boosted {
                        b.elapsed += 1;
                        if b.elapsed > b.total {
                            commands.entity(e).remove::<Boosted>();
                        } else {
                            boost = 1.0 + boost_gain * (b.elapsed as f32 / b.total as f32 * std::f32::consts::PI).msin();
                        }
                    }
                    d.speed = (body.speed * factor) as f32 * boost;
                    d.xxa = 0.0;
                    d.zza = 1.0;
                }
                Steer::Dash { push, lift, lift_scale, cooldown, sprint_bonus } => {
                    controlled = true;
                    let refused = posture.is_some_and(|p| p.refuses(now as i64));
                    if !refused {
                        yaw.0 = r.yaw;
                    }
                    let rising = r.forward > 0.0 && posture.is_some_and(|p| p.sat(now as i64));
                    if rising {
                        commands.queue(move |w: &mut World| crate::behaviours::posture::stand_up(w, e));
                    }
                    if refused || rising {
                        d.xxa = 0.0;
                        d.zza = 0.0;
                    } else {
                        let ready = now >= m.ready_at;
                        d.speed = body.speed as f32 + if r.sprint && ready { sprint_bonus } else { 0.0 };
                        d.xxa = r.strafe * 0.5;
                        d.zza = if r.forward <= 0.0 { r.forward * 0.25 } else { r.forward };
                        if r.jump > 0.0 {
                            let used = if !ready {
                                true
                            } else if !c.on_ground {
                                false
                            } else {
                                let a = yaw.0.to_radians();
                                let (dx, dz, k) = (f64::from(-a.msin()), f64::from(a.mcos()), f64::from(r.jump));
                                vel.0.x += dx * push * k * body.speed;
                                vel.0.z += dz * push * k * body.speed;
                                vel.0.y += lift * k * lift_scale;
                                m.ready_at = now + cooldown;
                                true
                            };
                            if used {
                                m.input.jump = 0.0;
                            }
                        }
                    }
                }
                Steer::Stick { .. } => {}
            },
            Who::Player(_) => {}
        }
        // `getFlyingSpeed` of a steered mount: a tenth of the speed it was given.
        if controlled {
            d.air_speed = Some(d.speed * 0.1);
        }
        commands.entity(e).insert(Moving);
    }
}
/// After the move: the riders sit where the mount is (a player of the bench, a creature put at the seat and turned with the mount); a creature
/// rider that is gone gets off. Not a system of its own (see `leash::tick`): `cow::apply_pushes` calls it each tick with its parameters; the
/// riders' cells are read again by the next tick's `reindex`.
#[allow(clippy::type_complexity)]
pub(crate) fn seats(
    q: &Query<(Entity, &Mount, &Yaw, &Body, &Pos), With<Ridden>>,
    riders: &mut Query<(&mut Pos, Option<&mut Yaw>), (With<Riding>, Without<Ridden>)>,
    on: &mut Online,
    commands: &mut Commands,
) {
    for (e, m, yaw, body, pos) in q.iter() {
        for (k, who) in [m.rider, m.passenger].into_iter().enumerate() {
            let at = seat_at(pos.0, yaw.0, body, m.seats.get(k).copied().unwrap_or(0.0), m.seat_height);
            match who {
                Some(Who::Player(id)) => {
                    if let Some(p) = on.get(id)
                        && (p.x, p.y, p.z) != (at.x, at.y, at.z)
                    {
                        on.set(Player { x: at.x, y: at.y, z: at.z, ..p });
                    }
                }
                Some(Who::Entity(rid)) => match riders.get_mut(rid) {
                    Ok((mut p, ry)) => {
                        if p.0 != at {
                            p.0 = at;
                            commands.entity(rid).insert(Moving);
                        }
                        if let Some(mut ry) = ry {
                            ry.0 = yaw.0;
                        }
                    }
                    Err(_) => {
                        commands.queue(move |w: &mut World| {
                            if k == 0 {
                                dismount(w, e);
                            } else {
                                dismount_passenger(w, e);
                            }
                        });
                    }
                },
                None => {}
            }
        }
    }
}

/// `Entity.ejectPassengers`: every rider gets off, the first seat then the second.
pub fn eject_passengers(w: &mut World, mount: Entity) {
    while dismount(w, mount).is_some() {}
}
