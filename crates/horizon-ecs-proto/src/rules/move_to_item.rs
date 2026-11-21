//! Reach of a fetcher toward a dropped item: the item's spot is within the fetcher's scan box grown by one block on each axis
//! (`mobs/dolphin.rs:442-477`).
use horizon_core::geometry::Vec3;

/// True when `spot` lies within `scan + 1` of `pos` on every axis (per-axis box test, inclusive).
pub fn reaches_item(spot: Vec3, pos: Vec3, scan: f64) -> bool {
    (spot.x - pos.x).abs() <= scan + 1.0 && (spot.y - pos.y).abs() <= scan + 1.0 && (spot.z - pos.z).abs() <= scan + 1.0
}
