//! `EndCrystal` (`boss/enderdragon/EndCrystal`): a box that stands still and takes hits, with no tick of its own. The fire at its
//! feet is set once at its first tick (`after`) and again when a block changes there (`block_changed`): a crystal at rest costs nothing.
//! Its time (the spin and bob) is not kept: the client draws its own and nothing of it is saved or sent.
use crate::blocks::{set_state, world_of};
use crate::cow::{after, Bounds, OwnTime, Pos};
use crate::projectile::Blast;
use crate::targeting::Who;
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::prelude::*;
use bevy_ecs::world::DeferredWorld;
use horizon_core::geometry::{Aabb, Vec3};
use std::collections::HashMap;

/// `ShowBottom`: the base under the crystal is drawn (true by default in vanilla); saved.
#[derive(Component, Clone, Copy, Debug)]
#[component(on_add = index_add, on_remove = index_remove)]
pub struct EndCrystal {
    pub show_bottom: bool,
}

/// The beam of a respawn crystal, to a block (`setBeamTarget`, `DATA_BEAM_TARGET`); saved when set.
#[derive(Component, Clone, Copy, Debug)]
pub struct Beam(pub [i32; 3]);

/// `setPermanentlyInvulnerable` (the rebuilt spike's crystal): only a creative player's blow gets through.
#[derive(Component, Clone, Copy, Debug)]
pub struct Shielded;

/// The crystals by the block at their feet, where the fire goes. Kept by the component hooks.
#[derive(Resource, Default)]
pub struct CrystalCells(pub HashMap<[i32; 3], Entity>);

/// Whether the fire is set under the crystals: the dimension has a dragon fight (`getDragonFight() != null`), set by the host.
#[derive(Resource, Default)]
pub struct EndFire(pub bool);

/// A crystal is gone (`EndCrystal.onDestroyedBy`): where, and the one behind the blow. The fight reads it.
#[derive(Clone, Copy, Debug)]
pub struct Gone {
    pub at: Vec3,
    pub by: Option<Who>,
}

#[derive(Resource, Default)]
pub struct CrystalEvents(pub Vec<Gone>);

/// A blow on a crystal: fire is refused, a dragon's blow too; an explosion removes it without a blast of its own.
#[derive(Clone, Copy, Debug)]
pub struct Blow {
    pub by: Option<Who>,
    pub fire: bool,
    pub explosion: bool,
    pub creative: bool,
    pub dragon: bool,
}

pub fn insert_resources(w: &mut World) {
    w.init_resource::<CrystalCells>();
    w.init_resource::<EndFire>();
    w.init_resource::<CrystalEvents>();
}

/// The block at the feet: where the crystal stands and where its fire goes (`blockPosition`).
fn cell(p: Vec3) -> [i32; 3] {
    [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32]
}

/// The crystal's box: 2 by 2, from its feet up 2 (`EndCrystal` is `EntityType.END_CRYSTAL`).
pub fn bounds(p: Vec3) -> Aabb {
    Aabb::new(Vec3::new(p.x - 1.0, p.y, p.z - 1.0), Vec3::new(p.x + 1.0, p.y + 2.0, p.z + 1.0))
}

fn index_add(mut world: DeferredWorld, ctx: HookContext) {
    let Some(c) = world.get::<Pos>(ctx.entity).map(|p| cell(p.0)) else { return };
    world.resource_mut::<CrystalCells>().0.insert(c, ctx.entity);
}

fn index_remove(mut world: DeferredWorld, ctx: HookContext) {
    let Some(c) = world.get::<Pos>(ctx.entity).map(|p| cell(p.0)) else { return };
    let cells = &mut world.resource_mut::<CrystalCells>().0;
    if cells.get(&c) == Some(&ctx.entity) {
        cells.remove(&c);
    }
}

/// A crystal at `at` (feet). Its first tick is armed now: the fire under it is checked then, and nothing runs in between.
pub fn spawn(w: &mut World, at: Vec3, show_bottom: bool) -> Entity {
    insert_resources(w);
    let e = w.spawn((Pos(at), Bounds(bounds(at)), OwnTime, EndCrystal { show_bottom }, crate::nbt::is_saved(w, "minecraft:end_crystal").then_some(crate::nbt::Saved))).id();
    after(w, e, 1, first_tick);
    e
}

fn first_tick(w: &mut World, e: Entity) {
    if let Some(c) = w.get::<Pos>(e).map(|p| cell(p.0)) {
        fire_at(w, c);
    }
}

/// `EndCrystal.tick`'s fire: set where the block is air, in the End only.
fn fire_at(w: &mut World, c: [i32; 3]) {
    if !w.resource::<EndFire>().0 {
        return;
    }
    let mw = world_of(w);
    if mw.facts.is_air(mw.world.block(c[0], c[1], c[2])) {
        set_state(w, c, "minecraft:fire", &[]);
    }
}

/// A block changed at `c` (the host's block events, read at the start of a tick): a crystal standing there sets its fire again.
pub fn block_changed(w: &mut World, c: [i32; 3]) {
    if w.resource::<CrystalCells>().0.contains_key(&c) {
        fire_at(w, c);
    }
}

/// A blow (`EndCrystal.hurtServer`). Refused: fire, a dragon's blow, a shielded crystal's blow unless creative. Otherwise it
/// goes at once, a blast of 6 unless the blow is an explosion, and the fight hears of it.
pub fn hurt(w: &mut World, e: Entity, blow: Blow) -> bool {
    let Some(at) = w.get::<Pos>(e).map(|p| p.0) else { return false };
    if blow.fire || blow.dragon || (w.get::<Shielded>(e).is_some() && !blow.creative) {
        return false;
    }
    w.despawn(e);
    if !blow.explosion {
        explode_at(w, at, blow.by);
    }
    w.resource_mut::<CrystalEvents>().0.push(Gone { at, by: blow.by });
    tell_fight(w, e, at, blow.by);
    true
}

/// `EndCrystal.kill` (`/kill`): gone with no blast, and the fight is told with no one behind it.
pub fn kill(w: &mut World, e: Entity) {
    let Some(at) = w.get::<Pos>(e).map(|p| p.0) else { return };
    w.despawn(e);
    w.resource_mut::<CrystalEvents>().0.push(Gone { at, by: None });
    tell_fight(w, e, at, None);
}

/// A fight in this world hears of every crystal that goes (`EnderDragonFight.onCrystalDestroyed`), whoever removed it.
fn tell_fight(w: &mut World, e: Entity, at: Vec3, by: Option<Who>) {
    if w.contains_resource::<crate::dragon::fight::EndFight>() {
        crate::dragon::crystals::gone(w, e, at, by.and_then(|b| if let Who::Player(p) = b { Some(p) } else { None }));
    }
}

fn explode_at(w: &mut World, at: Vec3, by: Option<Who>) {
    crate::explosion::explode(w, Blast { at, radius: 6.0, fire: false, owner: by, source: None });
}

/// `setBeamTarget`: the beam to `to`, or none.
pub fn set_beam(w: &mut World, e: Entity, to: Option<[i32; 3]>) {
    match to {
        Some(b) => {
            w.entity_mut(e).insert(Beam(b));
        }
        None => {
            w.entity_mut(e).remove::<Beam>();
        }
    }
}

/// The saved fields: `ShowBottom`, and the beam when there is one.
pub fn saved(w: &World, e: Entity) -> Option<(bool, Option<[i32; 3]>)> {
    let show = w.get::<EndCrystal>(e)?.show_bottom;
    Some((show, w.get::<Beam>(e).map(|b| b.0)))
}
