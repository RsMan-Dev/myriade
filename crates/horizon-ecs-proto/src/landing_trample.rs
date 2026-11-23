//! `FarmlandBlock.fallOn`: a creature landing on farmland after a fall turns it to dirt (`turnToBaseBlock`) when a roll under
//! `fall - 0.5` passes, the mob may grief and its box is wider than 0.512 (width² · height). `cow::travel` marks the landing
//! sparse (`Landed`); this system judges it once and removes the mark.
use crate::cow::{BlockChanges, Body, Bounds, Index, Level, Pos, Rng};
use crate::goals::Events;
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;

/// A `Monster` (`Monster.getFallSounds`): its fall sounds are the hostile pair. Inserted at the spawn of a species of that category.
#[derive(Component, Clone, Copy, Debug)]
pub struct HostileFall;

/// A landing on farmland: the block under the body and the distance fallen. Only the landing body carries it.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Landed {
    pub at: [i32; 3],
    pub fall: f64,
}

pub fn register(s: &mut Schedule) {
    s.add_systems(landing_trample.after(crate::cow::travel));
}

/// `Block.pushEntitiesUp` for farmland turned to dirt: the shape dirt lacks is farmland's top slab (`box(0,0,0,16,15,16)`, 15/16 high).
/// Every box that overlaps that slab is set on the top of the block (`teleportRelative(0, 1 + d, 0)`, d the way up to the slab's top).
pub fn push_entities_up(w: &mut World, x: i32, y: i32, z: i32) {
    let (x0, z0, top, slab) = (f64::from(x), f64::from(z), f64::from(y) + 1.0, f64::from(y) + 0.9375);
    let mut found = Vec::new();
    w.resource::<Index>().for_near_touching(x0 + 0.5, z0 + 0.5, (x0, z0), (x0 + 1.0, z0 + 1.0), |e| found.push(e));
    for e in found {
        let (Some(b), Some(p)) = (w.get::<Bounds>(e).map(|b| b.0), w.get::<Pos>(e).map(|p| p.0)) else { continue };
        let (mn, mx) = (b.min_position(), b.max_position());
        if mn.x < x0 + 1.0 && mx.x > x0 && mn.z < z0 + 1.0 && mx.z > z0 && mn.y < top && mx.y > slab {
            crate::cow::teleport(w, e, Vec3::new(p.x, top, p.z));
        }
    }
}

/// `LivingEntity.checkFallDamage`: the dust a landing throws up, `150 x min(0.2 + power / 15, 2.5)` crumbs, the power being the fall
/// beyond the safe distance (none within it).
pub fn landing_dust_count(fall: f64, safe: f64) -> Option<i32> {
    let power = f64::from(((fall + 1.0e-6 - safe).floor() as i32).max(0));
    (power > 0.0).then(|| (150.0 * (f64::from(0.2_f32) + power / 15.0).min(2.5)) as i32)
}

/// Where the dust starts: at the mob, or, when it straddles into another column than the landed block `landing`, on that block's face
/// toward the mob.
pub fn landing_dust_at(position: Vec3, landing: [i32; 2]) -> Vec3 {
    if landing[0] == position.x.floor() as i32 && landing[1] == position.z.floor() as i32 {
        return position;
    }
    let dx = position.x - f64::from(landing[0]) - 0.5;
    let dz = position.z - f64::from(landing[1]) - 0.5;
    let scale = dx.abs().max(dz.abs());
    Vec3::new(f64::from(landing[0]) + 0.5 + dx / scale * 0.5, position.y, f64::from(landing[1]) + 0.5 + dz / scale * 0.5)
}

fn landing_trample(
    mut commands: Commands,
    level: Res<Level>,
    mut changes: ResMut<BlockChanges>,
    mut events: ResMut<Events>,
    mut q: Query<(Entity, &Landed, &Body, Option<&mut Rng>), With<Landed>>,
) {
    for (e, landed, body, rng) in &mut q {
        commands.entity(e).remove::<Landed>();
        let roll = match rng {
            Some(mut r) => r.next_f64(),
            None => Rng(e.to_bits() | 1).next_f64(),
        };
        if roll >= landed.fall - 0.5 || !level.mw.0.mob_griefing || body.width * body.width * body.height <= 0.512 {
            continue;
        }
        let [x, y, z] = landed.at;
        changes.0.push((x, y, z, "minecraft:dirt"));
        commands.queue(move |w: &mut World| push_entities_up(w, x, y, z));
        events.host.game_event_by(e, "minecraft:block_change", [f64::from(x), f64::from(y), f64::from(z)], false, false, false);
    }
}
