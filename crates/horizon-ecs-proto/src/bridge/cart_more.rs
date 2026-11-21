//! The carts' contents and their aim through the bridge (`cart_more.rs`): the slots of a chest or hopper cart, its loot taken once, the cart a
//! ray enters first and the carts in a box, read from the spatial index. The contents are `cart_state::Contents`, the same components as the
//! cart's.
use crate::bridge::{host::EcsWorld, view};
use crate::cart_state::{Cart, CommandBlock, Contents, Flipped};
use crate::boat_state::BoatMarker;
use crate::cow::{Bounds, Health, Index, Pos};
use crate::rules::{boat::WIDTH as BOAT_WIDTH, seat_rules::CART_WIDTH};
use bevy_ecs::entity::Entity;
use horizon_client_lib::items::ItemStack;
use horizon_core::geometry::Vec3;
use horizon_core::nbt::Tag;
use std::collections::HashSet;

/// The ray is read every half block: a cart's box is about a block wide, so no entry is skipped between two reads.
const STEP: f64 = 0.5;
/// The widest vehicle's half width: a box is grown by it so every vehicle whose position is outside but whose box reaches in is found.
const MAX_HALF: f64 = (if BOAT_WIDTH > CART_WIDTH { BOAT_WIDTH } else { CART_WIDTH }) / 2.0;
/// The widest living half width (the ender dragon's 16 wide body): a box is grown by it so every creature whose box reaches in is found.
const LIVING_REACH: f64 = 8.0;

impl EcsWorld {
    /// The living creatures alive whose box meets `min..=max`: what a projectile can hit (`Entity.canBeHitByProjectile`: alive and pickable).
    pub fn living_touching(&self, min: Vec3, max: Vec3) -> Vec<Entity> {
        let w = self.world();
        let Some(index) = w.get_resource::<Index>() else { return Vec::new() };
        let (cx, cz) = ((min.x + max.x) / 2.0, (min.z + max.z) / 2.0);
        let reach = (max.x - min.x).hypot(max.z - min.z) / 2.0 + LIVING_REACH;
        let mut out = Vec::new();
        index.for_radius(cx, cz, reach, |e| {
            let alive = w.get::<Health>(e).is_some_and(|h| h.0 > 0.0);
            let meets = w.get::<Bounds>(e).is_some_and(|b| {
                let (lo, hi) = (b.0.min_position(), b.0.max_position());
                lo.x <= max.x && hi.x >= min.x && lo.y <= max.y && hi.y >= min.y && lo.z <= max.z && hi.z >= min.z
            });
            if alive && meets {
                out.push(e);
            }
        });
        out
    }
    /// The slots of the contents of cart `e`; None when it has none (no chest, no hopper).
    pub fn cart_items(&self, e: Entity) -> Option<&[Option<ItemStack>]> {
        self.world().get::<Contents>(e).map(|k| k.slots.as_slice())
    }
    /// Sets slot `slot` of the contents of cart `e`; false when there is no such slot.
    pub fn cart_set_slot(&mut self, e: Entity, slot: usize, stack: Option<ItemStack>) -> bool {
        let Some(mut k) = self.world_mut().get_mut::<Contents>(e) else { return false };
        let Some(s) = k.slots.get_mut(slot) else { return false };
        *s = stack;
        true
    }
    /// A copy of the command block state of cart `e`; None when it is not a command cart.
    pub fn cart_command(&self, e: Entity) -> Option<CommandBlock> {
        self.world().get::<CommandBlock>(e).cloned()
    }
    /// Replaces the command block state of cart `e` by `state`; false when it is not a command cart.
    pub fn set_cart_command(&mut self, e: Entity, state: CommandBlock) -> bool {
        let w = self.world_mut();
        let Some(mut b) = w.get_mut::<CommandBlock>(e) else { return false };
        *b = state;
        drop(b);
        view::mark(w, e);
        true
    }
    /// The flipped rotation of cart `e`; None when it has no such state.
    pub fn cart_flipped(&self, e: Entity) -> Option<bool> {
        self.world().get::<Flipped>(e).map(|f| f.0)
    }
    /// The loot table and seed of the contents of cart `e`, once: the host rolls them. None when nothing is pending or the table is no string.
    pub fn cart_take_loot(&mut self, e: Entity) -> Option<(String, i64)> {
        let l = self.world_mut().get_mut::<Contents>(e)?.loot.take()?;
        let Tag::String(table) = l.table else { return None };
        let seed = if let Some(Tag::Long(s)) = l.seed { s } else { 0 };
        Some((table.to_string_lossy(), seed))
    }
    /// The cart the ray from `eye` along `dir` (a unit vector) enters first within `reach`, with the distance to its entry face, `skip` (the
    /// cart the aimer rides) left out. The candidates come from the index along the ray; each is clipped exactly against its box (`Aabb::clip`).
    pub fn aimed_cart(&self, eye: Vec3, dir: Vec3, reach: f64, skip: Option<Entity>) -> Option<(Entity, f64)> {
        let w = self.world();
        let index = w.get_resource::<Index>()?;
        let end = Vec3::new(eye.x + dir.x * reach, eye.y + dir.y * reach, eye.z + dir.z * reach);
        let mut seen = HashSet::new();
        let mut best: Option<(Entity, f64)> = None;
        for k in 0..=(reach / STEP) as u32 {
            let t = f64::from(k) * STEP;
            index.for_near(eye.x + dir.x * t, eye.z + dir.z * t, |e| {
                if skip == Some(e) || !seen.insert(e) || w.get::<Cart>(e).is_none() {
                    return;
                }
                let Some(b) = w.get::<Bounds>(e) else { return };
                if let Some(hit) = b.0.clip(eye, end) {
                    let d = ((hit.x - eye.x).powi(2) + (hit.y - eye.y).powi(2) + (hit.z - eye.z).powi(2)).sqrt();
                    if best.map_or(true, |(_, bt)| d < bt) {
                        best = Some((e, d));
                    }
                }
            });
        }
        best
    }
    /// The boats and carts whose BOX meets the box `min..=max` (`getEntities(box)`, not the position test of `boats_in`/`carts_in`): the
    /// index answers for the box grown by the widest half width, then each candidate's own box is tested.
    pub fn vehicles_touching(&self, min: Vec3, max: Vec3) -> Vec<Entity> {
        let w = self.world();
        let Some(index) = w.get_resource::<Index>() else { return Vec::new() };
        let (cx, cz) = ((min.x + max.x) / 2.0, (min.z + max.z) / 2.0);
        let reach = (max.x - min.x).hypot(max.z - min.z) / 2.0 + MAX_HALF;
        let mut out = Vec::new();
        index.for_radius(cx, cz, reach, |e| {
            let vehicle = w.get::<BoatMarker>(e).is_some() || w.get::<Cart>(e).is_some();
            let meets = |b: &horizon_core::geometry::Aabb| {
                let (lo, hi) = (b.min_position(), b.max_position());
                lo.x <= max.x && hi.x >= min.x && lo.y <= max.y && hi.y >= min.y && lo.z <= max.z && hi.z >= min.z
            };
            if vehicle && w.get::<Bounds>(e).is_some_and(|b| meets(&b.0)) {
                out.push(e);
            }
        });
        out
    }
    /// The carts whose position lies in the box `min..=max`, read from the cells the box spans.
    pub fn carts_in(&self, min: Vec3, max: Vec3) -> Vec<Entity> {
        let w = self.world();
        let Some(index) = w.get_resource::<Index>() else { return Vec::new() };
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for x in grid(min.x, max.x) {
            for z in grid(min.z, max.z) {
                index.for_near_touching(x, z, (min.x, min.z), (max.x, max.z), |e| {
                    let inside = w.get::<Pos>(e).is_some_and(|p| p.0.x >= min.x && p.0.x <= max.x && p.0.y >= min.y && p.0.y <= max.y && p.0.z >= min.z && p.0.z <= max.z);
                    if seen.insert(e) && w.get::<Cart>(e).is_some() && inside {
                        out.push(e);
                    }
                });
            }
        }
        out
    }
}

/// Points from `lo` to `hi` one block apart, `hi` included: the index's cells of the span are each within one of them.
fn grid(lo: f64, hi: f64) -> impl Iterator<Item = f64> {
    let n = (hi - lo).max(0.0).ceil() as u32;
    (0..=n).map(move |i| (lo + f64::from(i)).min(hi))
}
