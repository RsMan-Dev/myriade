//! The End crystals (`D/crystal.rs`; `place_spike_crystals` D:1420, `destroy_crystal` 1493, `crystal_destroyed` 1528, `nearest_crystal_to`
//! 4650). A crystal is an entity with a 2x2 box from its feet up 2. The count is kept by the events (placed +1, destroyed -1), never by
//! a scan. The beam and the shield are rare: their own components, only on respawn crystals.
use crate::cow::*;
use crate::dragon::fight::{self, CrystalDestroyed, EndFight, FightEvents, NearestCrystal, StrafeRequest};
use crate::dragon::hurt::{self, Blow};
use crate::dragon::phases::PhaseId;
use crate::explosion::explode;
use crate::projectile::Blast;
use crate::targeting::Who;
use bevy_ecs::prelude::*;
use horizon_core::geometry::{Aabb, Vec3};
use horizon_world::worldgen::end_spike::EndSpike;

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct EndCrystal {
    pub show_bottom: bool,
}

/// The beam of a respawn crystal, to a block (`set beam`); removed when the respawn ends.
#[derive(Component, Clone, Copy, Debug)]
pub struct Beam(pub [i32; 3]);

/// The invulnerable crystal of a rebuilt spike: it takes only a creative player's hit (`hit_special`).
#[derive(Component, Clone, Copy, Debug)]
pub struct Shielded;

/// The crystal's box: 2 by 2, from its feet up 2 (`EndCrystal.getBoundingBox`).
pub fn bounds(p: Vec3) -> Aabb {
    Aabb::new(Vec3::new(p.x - 1.0, p.y, p.z - 1.0), Vec3::new(p.x + 1.0, p.y + 2.0, p.z + 1.0))
}

/// A crystal at `at` (feet), counted in the spike total. Its own time starts now, so a respawn can arm its timer on it.
pub fn spawn(w: &mut World, at: Vec3, show_bottom: bool) -> Entity {
    w.resource_mut::<EndFight>().crystals_alive += 1;
    w.spawn((Pos(at), Bounds(bounds(at)), EndCrystal { show_bottom }, OwnTime)).id()
}

/// The crystals of a spike (`place_spike_crystals`): one at the centre, a block above the top, unless one already stands there.
pub fn place_spike(w: &mut World, spike: &EndSpike) {
    let at = Vec3::new(f64::from(spike.center_x) + 0.5, f64::from(spike.height + 1), f64::from(spike.center_z) + 0.5);
    let mut q = w.query::<&Pos>();
    if q.iter(w).any(|p| p.0.distance_squared(at) < 1.0) {
        return;
    }
    spawn(w, at, true);
}

/// Destroys crystal `e` (`destroy_crystal`): it goes first, a direct hit blows it up (radius 6), then the fight is told.
pub fn destroy(w: &mut World, e: Entity, by: Option<u32>, direct: bool) {
    let Some(at) = w.get::<Pos>(e).map(|p| p.0) else { return };
    w.despawn(e);
    if direct {
        explode(w, Blast { at, radius: 6.0, fire: false, owner: by.map(Who::Player), source: None });
    }
    gone(w, e, at, by);
}

/// The crystal is gone (`crystal_destroyed`): a respawn crystal aborts the respawn, a spike crystal leaves the count. The dragon
/// loses its nearest crystal once it was that one, and takes the blast's 10 points; the strafe follows.
pub fn gone(w: &mut World, e: Entity, at: Vec3, by: Option<u32>) {
    let respawn_crystal = w.resource::<EndFight>().respawn_crystals.contains(&e);
    if respawn_crystal {
        crate::dragon::respawn::abort(w);
    } else {
        let f = w.resource_mut::<EndFight>().into_inner();
        f.crystals_alive = f.crystals_alive.saturating_sub(1);
    }
    let by = by.or_else(|| fight::nearest_enemy(w, Vec3::new(at.x.floor(), at.y.floor(), at.z.floor()), None, None).map(|(id, _)| id));
    w.resource_mut::<FightEvents>().crystal_destroyed.push(CrystalDestroyed { crystal: e, by });
    let Some(d) = w.resource::<EndFight>().dragon else { return };
    if w.get::<NearestCrystal>(d).and_then(|n| n.0) == Some(e) {
        w.entity_mut(d).insert(NearestCrystal(None));
        let blow = Blow { part: 0, damage: 10.0, cause: Cause::Blast, by: by.map(Who::Player), arrow: false, from: None };
        hurt::hurt(w, d, blow);
    }
    strafe_after_crystal(w, d, by);
}

/// `strafe_after_crystal` (D:1037): an enemy player behind the blast asks the holding dragon to strafe him, once.
fn strafe_after_crystal(w: &mut World, d: Entity, by: Option<u32>) {
    let Some(p) = by else { return };
    let holding = w.get::<PhaseId>(d).is_some_and(|i| i.0 == 0);
    if holding && w.get::<StrafeRequest>(d).is_none() && fight::is_enemy(w, p) {
        w.entity_mut(d).insert(StrafeRequest(p));
    }
}

/// A hit on crystal `e` (`hit_special`): fire is refused; a shielded crystal takes only a creative player's hit, which unshields it.
pub fn hit(w: &mut World, e: Entity, by: Option<u32>, fire: bool, creative: bool) -> bool {
    if fire {
        return false;
    }
    if w.get::<Shielded>(e).is_some() {
        if !creative {
            return false;
        }
        w.entity_mut(e).remove::<Shielded>();
    }
    destroy(w, e, by, true);
    true
}

/// The crystal nearest to `p` whose box meets `reach` (`nearest_crystal_to`). Read when a fight needs it, never per tick.
pub fn nearest_to(w: &mut World, p: Vec3, reach: Aabb) -> Option<Entity> {
    let mut q = w.query::<(Entity, &Pos, &EndCrystal)>();
    q.iter(w)
        .filter(|(_, c, _)| bounds(c.0).intersects(reach))
        .min_by(|a, b| a.1 .0.distance_squared(p).total_cmp(&b.1 .0.distance_squared(p)))
        .map(|(e, _, _)| e)
}
