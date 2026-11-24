//! `LeashFenceKnotEntity` (`minecraft:leash_knot`): the knot a lead is tied to on a fence. It holds the leashes that point at its
//! cell and lives while the fence stands. `BlockAttachedEntity.tick` checks `survives` (`BlockTags.FENCES`) on the 101st tick after
//! the birth; a block change in its cell wakes it. Nothing ticks: an intact knot costs nothing per tick, leashees or not.
use super::Def;
use crate::behaviours::Cap;
use crate::block_events::BlockEvents;
use crate::cow::*;
use crate::goals::Events;
use crate::leash::{self, Holder};
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;
use horizon_core::nbt::{Compound, JavaString, Tag};
use std::collections::HashMap;

pub static DEF: Def = Def { name: "minecraft:leash_knot", caps: Some(&[Cap::Transform, Cap::Hanging]), saved: false, ..Def::EMPTY };

/// `BlockAttachedEntity.tick`: the check falls on the 101st tick after the birth.
pub const CHECK_PERIOD: u64 = 101;
/// `LeashFenceKnotEntity.OFFSET_Y`: the knot sits this high above its fence cell.
const OFFSET_Y: f64 = 0.375;

/// The knot: the fence cell it is tied to, and its birth in its own time.
#[derive(Component, Clone, Copy, Debug)]
#[component(on_remove = leave_index)]
pub struct Knot {
    pub fence: [i32; 3],
    pub born: u64,
}

/// The knot of each fence cell (one knot per cell): found when a lead is tied, and woken when that cell changes.
#[derive(Resource, Default)]
pub struct KnotIndex(HashMap<[i32; 3], Entity>);

/// The host's answer for the fence: `true` when the block at the cell is a fence (`BlockTags.FENCES`).
#[derive(Resource)]
pub struct Fences(pub fn(&World, [i32; 3]) -> bool);

fn leave_index(mut world: bevy_ecs::world::DeferredWorld, ctx: bevy_ecs::lifecycle::HookContext) {
    let Some(k) = world.get::<Knot>(ctx.entity).copied() else { return };
    let mut index = world.resource_mut::<KnotIndex>();
    if index.0.get(&k.fence) == Some(&ctx.entity) {
        index.0.remove(&k.fence);
    }
}

/// `LeashFenceKnotEntity.getOrCreate`: the knot of the fence at `fence`, made at its centre when there is none. A new knot needs no
/// check: the fence was seen when the lead was tied, and only a change in its cell can end it.
pub fn tie(w: &mut World, fence: [i32; 3]) -> Entity {
    if let Some(&e) = w.resource::<KnotIndex>().0.get(&fence) {
        return e;
    }
    let at = Vec3::new(f64::from(fence[0]) + 0.5, f64::from(fence[1]) + OFFSET_Y, f64::from(fence[2]) + 0.5);
    let e = w.spawn((Pos(at), OwnTime, Knot { fence, born: 0 })).id();
    let born = now_of(w, e);
    w.get_mut::<Knot>(e).expect("just spawned").born = born;
    w.resource_mut::<KnotIndex>().0.insert(fence, e);
    e
}

/// A leashee left its knot (`notifyLeasheeRemoved`): a knot that holds no leash any more is discarded, without a sound.
pub fn released(w: &mut World, fence: [i32; 3]) {
    let Some(&e) = w.resource::<KnotIndex>().0.get(&fence) else { return };
    if leash::held_by(w, Holder::Knot(fence)).is_empty() {
        w.despawn(e);
    }
}

/// Arms the knot's next check: the first multiple of 101 after its birth that is strictly after now.
fn arm(w: &mut World, e: Entity) {
    let Some(born) = w.get::<Knot>(e).map(|k| k.born) else { return };
    let now = now_of(w, e);
    let next = born + CHECK_PERIOD * ((now - born) / CHECK_PERIOD + 1);
    after(w, e, (next - now) as i32, check);
}

/// `BlockAttachedEntity.tick`'s `survives`: a knot whose fence is gone is discarded (no kill, no item). The leashes it held are the
/// leash owner's to release.
fn check(w: &mut World, e: Entity) {
    let Some(k) = w.get::<Knot>(e).copied() else { return };
    if w.get_resource::<Fences>().is_none_or(|f| (f.0)(w, k.fence)) {
        return;
    }
    let at = w.get::<Pos>(e).map_or(Vec3::ZERO, |p| p.0);
    w.despawn(e);
    w.resource_mut::<Events>().host.sound("item.lead.untied", [at.x, at.y, at.z], 1.0, 1.0);
}

/// Block changes wake the knot of the changed cell, if there is one: it arms its next check.
pub fn knot_wake(w: &mut World) {
    let changed = match w.get_resource::<BlockEvents>() {
        Some(b) if !b.changed.is_empty() => b.changed.clone(),
        _ => return,
    };
    for c in changed {
        if let Some(&e) = w.resource::<KnotIndex>().0.get(&c) {
            arm(w, e);
        }
    }
}

pub fn insert_resources(w: &mut World) {
    w.insert_resource(KnotIndex::default());
    crate::nbt::declare(w, DEF.name, crate::nbt::Recipe { saved: false, build: crate::nbt::Build::With(build) });
}

/// A knot made by `summon`: the knot of the fence cell holding the NBT's position, under the NBT's UUID.
fn build(w: &mut World, at: Vec3, c: &Compound) -> Option<Entity> {
    let fence = [at.x.floor() as i32, at.y.floor() as i32, at.z.floor() as i32];
    let e = tie(w, fence);
    if let Some(Tag::IntArray(u)) = c.get(&JavaString::from("UUID")) && let Ok(u) = <[i32; 4]>::try_from(u.as_slice()) {
        crate::bridge::uuids::set_uuid(w, e, u);
    }
    if let Some(Tag::List(tags)) = c.get(&JavaString::from("Tags")) {
        for t in tags {
            if let Tag::String(n) = t {
                if let Ok(n) = n.to_string() { crate::nbt_tag::add_unsaved(w, e, &n); }
            }
        }
    }
    Some(e)
}
pub fn register(s: &mut bevy_ecs::schedule::Schedule) {
    s.add_systems(knot_wake.after(crate::cow::block_changes));
}

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_core::nbt::Compound;

    #[test]
    fn a_summoned_knot_sits_on_its_fence_under_the_nbt_uuid() {
        let mut w = crate::cow::Herd::new(0, 1.0, true, false, 1).world;
        let mut c = Compound::new();
        c.insert("UUID".into(), Tag::IntArray(vec![7193, 0, 0, 1]));
        let e = crate::nbt::spawn_built(&mut w, DEF.name, Vec3::new(4.5, 64.0, 6.5), Some(&c)).flatten();
        let e = e.expect("the knot is made");
        assert_eq!(w.get::<Knot>(e).map(|k| k.fence), Some([4, 64, 6]));
        assert_eq!(crate::bridge::uuids::entity_by_uuid(&w, [7193, 0, 0, 1]), Some(e));
    }
}
