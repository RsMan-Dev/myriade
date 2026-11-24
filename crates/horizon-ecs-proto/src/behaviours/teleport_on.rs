//! `teleport_on`: an attached body moves to another free face (`teleportSomewhere`): `tries` tries, each an offset of up to `radius` blocks on
//! every axis; a try needs air, a free box, no block of the world's bottom or border, and an attach face among the six whose support is a full
//! face and which is not behind a `tag` block. The lid shuts and the target is dropped; `eject` also removes the riders. Triggers: a hurt below
//! `hurt_below` of the max health, one chance in `hurt_one_in` (`damaged`); and, when `copy_on_hit`, a projectile hit on an open body: the body
//! teleports, then a copy of its colour stays behind unless the bodies around it (`shells`, within `crowd_radius`) make the chance fail, one in
//! `crowd_divisor` per other. Not done here: the game event and the level event of the teleport (`HostOut` has no output for them yet).
use super::*;
use crate::behaviours::attached_face::{Attached, Sounds, set_peek};
use crate::cow::*;
use crate::goals::{Events, with_rng};
use crate::projectile::STEPS;
use crate::rules::teleport_on::{attachment_box, cell_of, copy_refused, eps, foot_of, is_hurt_below, spot_box};
use crate::targeting;
use horizon_core::geometry::{Aabb, Vec3};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub shells: &'static [&'static str],
    pub tag: &'static str,
    pub sounds: Sounds,
    pub covered: bool,
    /// The sound of the move, at the old place.
    pub teleport: &'static str,
    pub hurt_below: f32,
    pub hurt_one_in: i32,
    pub tries: i32,
    pub radius: i32,
    pub eject: bool,
    pub copy_on_hit: bool,
    pub crowd_radius: f64,
    pub crowd_divisor: f32,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct Spot {
    pub cfg: &'static Params,
}

pub static MODULE: Module = Module {
    name: "teleport_on",
    flags: 0,
    equip,
    damaged,
    requires: &[Cap::Body, Cap::Living],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert((Spot { cfg: params::<Params>(p) }, Unchecked));
}
/// The body has not been asked yet whether its support holds (its first tick).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
struct Unchecked;

pub fn register(s: &mut bevy_ecs::schedule::Schedule) {
    s.add_systems(check_supports.after(crate::cow::block_changes));
}
/// `Shulker.tick`: a body that is not a passenger and whose support does not hold looks for another (`findNewAttachment`). By event: on its
/// first tick, and when a block changes in the world.
fn check_supports(w: &mut World) {
    let fresh: Vec<Entity> = w.query_filtered::<Entity, With<Unchecked>>().iter(w).collect();
    let changed = w.get_resource::<crate::block_events::BlockEvents>().is_some_and(|b| !b.changed.is_empty());
    let all: Vec<Entity> = if changed { w.query_filtered::<Entity, With<Spot>>().iter(w).collect() } else { fresh.clone() };
    for e in fresh {
        w.entity_mut(e).remove::<Unchecked>();
    }
    for e in all {
        if !crate::bridge::host_vehicle::is_passenger(w, e) && !can_stay(w, e) {
            find_new_attachment(w, e);
        }
    }
}
/// `Shulker.findNewAttachment`: the first face of its own cell that holds it, else `teleportSomewhere`.
pub fn find_new_attachment(w: &mut World, e: Entity) {
    let Some(own) = w.get::<Pos>(e).map(|p| cell_of(p.0)) else { return };
    if let Some(face) = attachable(w, e, own) {
        if let Some(mut a) = w.get_mut::<Attached>(e) {
            a.face = face;
            crate::bridge::view::mark(w, e);
        }
    } else {
        teleport_somewhere(w, e);
    }
}
/// Below `hurt_below` of the max health, one chance in `hurt_one_in` to teleport (the body's own random).
fn damaged(w: &mut World, e: Entity, _: Inst, _: &Damaged) {
    let Some(cfg) = w.get::<Spot>(e).map(|s| s.cfg) else { return };
    let hp = w.get::<Health>(e).map_or(0.0, |h| h.0);
    if is_hurt_below(hp, crate::cow::max_health(w, e), cfg.hurt_below) && with_rng(w, e, |r| r.next_int(cfg.hurt_one_in)) == 0 {
        teleport_somewhere(w, e);
    }
}

/// The first of the tries that finds a spot. False: none, or no body to move.
pub fn teleport_somewhere(w: &mut World, e: Entity) -> bool {
    let (Some(old), Some(cfg)) = (w.get::<Pos>(e).map(|p| cell_of(p.0)), w.get::<Spot>(e).map(|s| s.cfg)) else { return false };
    for _ in 0..cfg.tries {
        let t = [old[0] + offset(w, e, cfg.radius), old[1] + offset(w, e, cfg.radius), old[2] + offset(w, e, cfg.radius)];
        let unit = spot_box(t);
        if t[1] <= bottom_y(w) || !air(w, t) || !in_border(w, t) || !box_free(w, e, unit.deflate(eps())) {
            continue;
        }
        let Some(face) = attachable(w, e, t) else { continue };
        let s = STEPS[face as usize];
        if has_tag(w, cfg.tag, [t[0] + s[0], t[1] + s[1], t[2] + s[2]]) {
            continue;
        }
        let at = w.get::<Pos>(e).map(|p| [p.0.x, p.0.y, p.0.z]).unwrap_or([0.0; 3]);
        w.resource_mut::<Events>().host.sound(cfg.teleport, at, 1.0, 1.0);
        if let Some(mut a) = w.get_mut::<Attached>(e) {
            a.face = face;
            crate::bridge::view::mark(w, e);
        }
        set_peek(w, e, 0, &cfg.sounds, cfg.covered);
        if cfg.eject {
            crate::mount::eject_passengers(w, e);
        }
        crate::cow::teleport(w, e, foot_of(t));
        targeting::clear_target(w, e);
        return true;
    }
    false
}

/// A projectile hit an open body: it teleports, then (`copy_on_hit`) the chance of a copy of its colour where it was.
pub fn on_projectile_hit(w: &mut World, e: Entity) {
    let Some(cfg) = w.get::<Spot>(e).map(|s| s.cfg) else { return };
    let (Some(old), Some(old_box)) = (w.get::<Pos>(e).map(|p| p.0), w.get::<Bounds>(e).map(|b| b.0)) else { return };
    if !teleport_somewhere(w, e) || !cfg.copy_on_hit {
        return;
    }
    let area = old_box.inflate(Vec3::new(cfg.crowd_radius, cfg.crowd_radius, cfg.crowd_radius));
    let others = crate::cow::count_others_of(w, e, cfg.shells, area);
    if copy_refused(with_level_rng(w, |r| r.next_float()), others as f32, cfg.crowd_divisor) {
        return;
    }
    let colour = w.get::<Attached>(e).map_or(0, |a| a.color);
    crate::cow::spawn_copy(w, e, old, colour);
}

/// `canStayAt` for the spot the body is on, with its own face.
pub fn can_stay(w: &World, e: Entity) -> bool {
    let (Some(p), Some(face)) = (w.get::<Pos>(e).map(|p| cell_of(p.0)), w.get::<Attached>(e).map(|a| a.face)) else { return false };
    stays(w, e, p, face)
}
/// `canStayAt` for the spot `p` with face `face`, for the body `e` wherever it stands.
pub fn can_stay_at(w: &World, e: Entity, p: [i32; 3], face: u8) -> bool {
    stays(w, e, p, face)
}
fn attachable(w: &World, e: Entity, p: [i32; 3]) -> Option<u8> {
    (0..6).find(|&f| stays(w, e, p, f))
}
/// The spot holds the body: its cell is air (or a moving piston driven through it), the support is a full face, the open box is free.
fn stays(w: &World, e: Entity, p: [i32; 3], face: u8) -> bool {
    let own = w.get::<Pos>(e).map(|q| cell_of(q.0));
    if !air(w, p) && (Some(p) != own || !moving_piston(w, p)) {
        return false;
    }
    let s = STEPS[face as usize];
    let support = [p[0] + s[0], p[1] + s[1], p[2] + s[2]];
    full_face(w, support, face ^ 1) && box_free(w, e, attachment_box(p, STEPS[(face ^ 1) as usize], size_of(w, e)))
}
fn offset(w: &mut World, e: Entity, radius: i32) -> i32 {
    with_rng(w, e, |r| r.next_int(2 * radius + 1) - radius)
}
fn size_of(w: &World, e: Entity) -> f32 {
    crate::cow::model_scale(w, e)
}
fn bottom_y(w: &World) -> i32 {
    crate::cow::bottom_y(w)
}
fn air(w: &World, p: [i32; 3]) -> bool {
    crate::cow::block_is_air(w, p)
}
fn moving_piston(w: &World, p: [i32; 3]) -> bool {
    crate::cow::block_is_moving_piston(w, p)
}
fn in_border(w: &World, p: [i32; 3]) -> bool {
    crate::cow::within_border(w, p)
}
fn box_free(w: &World, e: Entity, b: Aabb) -> bool {
    crate::cow::box_free_of_blocks(w, e, b) && crate::cow::box_free_of_crowd(w, e, b)
}
fn full_face(w: &World, p: [i32; 3], face: u8) -> bool {
    crate::cow::full_face(w, p, face)
}
fn has_tag(w: &World, tag: &str, p: [i32; 3]) -> bool {
    crate::cow::has_block_tag(w, tag, p)
}
fn with_level_rng<T>(w: &mut World, f: impl FnOnce(&mut horizon_core::random::LegacyRandom) -> T) -> T {
    crate::cow::with_level_random(w, f)
}
