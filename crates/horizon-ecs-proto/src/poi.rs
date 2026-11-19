//! The points of interest (`PoiManager`): the blocks villagers and the creatures of a village go to or live by (beds, job sites, bells), their
//! types (Horizon's own `horizon_world::poi`), their tickets (who has taken one), the search of the ones near a place, and the distance
//! of a place to the nearest village, in sections (`PoiManager.sectionsToVillage`). The registry is written by events: a block that changed
//! (`ChangedBlocks`) is read again, a point appears or goes (`poi_added` / `poi_removed`, readable the tick after like the other events);
//! nothing scans the world each tick. The bench's world is indexed once at the first use (a server would index each chunk as it loads).
//! A creature asks "is it in a village" by the named predicate `horizon:in_village`.
use crate::cow::*;
use crate::hostile::ChangedBlocks;
use bevy_ecs::prelude::*;
use horizon_world::poi::{saved_poi, PoiType};
use std::collections::HashMap;

/// A point of interest: its type and the tickets taken.
#[derive(Clone, Copy, Debug)]
pub struct Poi {
    pub kind: PoiType,
    pub taken: usize,
}
#[derive(Resource, Default)]
pub struct PoiRegistry {
    points: HashMap<[i32; 3], Poi>,
    /// The village points (homes, job sites, meeting points) in each section: what `sections_to_village` measures from.
    sections: HashMap<[i32; 3], u32>,
    scanned: bool,
    /// The points that appeared / went this tick, and the ones of the tick before (what the modules read).
    now: (Vec<[i32; 3]>, Vec<[i32; 3]>),
    last: (Vec<[i32; 3]>, Vec<[i32; 3]>),
}
fn section(p: [i32; 3]) -> [i32; 3] {
    [p[0] >> 4, p[1] >> 4, p[2] >> 4]
}
/// `PoiTypes.IS_VILLAGE`: the points a village is made of.
fn is_village(k: PoiType) -> bool {
    k.is_job() || matches!(k, PoiType::Home | PoiType::Meeting)
}
impl PoiRegistry {
    /// The points that appeared / went in the last tick.
    pub fn added(&self) -> &[[i32; 3]] {
        &self.last.0
    }
    pub fn removed(&self) -> &[[i32; 3]] {
        &self.last.1
    }
    pub fn get(&self, p: [i32; 3]) -> Option<Poi> {
        self.points.get(&p).copied()
    }
    pub fn len(&self) -> usize {
        self.points.len()
    }
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }
    fn add(&mut self, p: [i32; 3], kind: PoiType) {
        if self.points.insert(p, Poi { kind, taken: 0 }).is_none() {
            if is_village(kind) {
                *self.sections.entry(section(p)).or_default() += 1;
            }
            self.now.0.push(p);
        }
    }
    fn remove(&mut self, p: [i32; 3]) {
        if let Some(old) = self.points.remove(&p) {
            if is_village(old.kind)
                && let Some(n) = self.sections.get_mut(&section(p))
            {
                *n -= 1;
                if *n == 0 {
                    self.sections.remove(&section(p));
                }
            }
            self.now.1.push(p);
        }
    }
    /// `PoiManager.getInRange` (with `HAS_SPACE` when `free`): the points of the accepted types within `radius` blocks of `center` on each axis,
    /// nearest first, ties by chunk (z, then x) then y, z, x.
    pub fn in_range(&self, center: [i32; 3], radius: i32, free: bool, accept: impl Fn(PoiType) -> bool) -> Vec<([i32; 3], PoiType)> {
        let mut v: Vec<(i64, i32, i32, [i32; 3], PoiType)> = self
            .points
            .iter()
            .filter(|(p, poi)| (0..3).all(|i| (p[i] - center[i]).abs() <= radius) && accept(poi.kind) && (!free || poi.taken < poi.kind.max_tickets()))
            .map(|(p, poi)| ((0..3).map(|i| i64::from(p[i] - center[i]).pow(2)).sum::<i64>(), p[2] >> 4, p[0] >> 4, *p, poi.kind))
            .collect();
        v.sort_by_key(|x| (x.0, x.1, x.2, x.3[1], x.3[2], x.3[0]));
        v.into_iter().map(|x| (x.3, x.4)).collect()
    }
    /// `PoiManager.take`: a ticket of the point, if one is free.
    pub fn acquire(&mut self, p: [i32; 3]) -> bool {
        match self.points.get_mut(&p) {
            Some(poi) if poi.taken < poi.kind.max_tickets() => {
                poi.taken += 1;
                true
            }
            _ => false,
        }
    }
    /// `PoiManager.release`.
    pub fn release(&mut self, p: [i32; 3]) -> bool {
        match self.points.get_mut(&p) {
            Some(poi) if poi.taken > 0 => {
                poi.taken -= 1;
                true
            }
            _ => false,
        }
    }
    /// `PoiManager.sectionsToVillage`: the distance in sections (the largest of the three axes) to the nearest section with a village
    /// point, 7 when there is none within that.
    pub fn sections_to_village(&self, s: [i32; 3]) -> i32 {
        self.sections.keys().map(|v| (0..3).map(|i| (v[i] - s[i]).abs()).max().unwrap_or(0)).min().unwrap_or(7).min(7)
    }
    /// `ServerLevel.isVillage`: at most one section from a village point.
    pub fn is_village(&self, p: [i32; 3]) -> bool {
        self.sections_to_village(section(p)) <= 1
    }
    /// `PoiManager.getClosestSection`-style walk of the golem goals: of the sections within `radius` of `center`, the one nearest a village
    /// that is nearer than `center` itself (`center` when none is).
    pub fn closest_section(&self, center: [i32; 3], radius: i32) -> [i32; 3] {
        let here = self.sections_to_village(center);
        let mut best: Option<([i32; 3], i32)> = None;
        for z in center[2] - radius..=center[2] + radius {
            for y in center[1] - radius..=center[1] + radius {
                for x in center[0] - radius..=center[0] + radius {
                    let d = self.sections_to_village([x, y, z]);
                    if d < here && best.is_none_or(|(_, b)| d < b) {
                        best = Some(([x, y, z], d));
                    }
                }
            }
        }
        best.map_or(center, |(s, _)| s)
    }
}
pub fn insert_resources(w: &mut World) {
    w.insert_resource(PoiRegistry::default());
}
pub fn register(s: &mut bevy_ecs::schedule::Schedule) {
    s.add_systems(poi_changes.after(block_changes).before(crate::targeting::sight_eval));
}
/// The block at `p` as a point of interest, if it is one.
fn classify(mw: &horizon_client_lib::mobs::MobWorld<'static>, p: [i32; 3]) -> Option<PoiType> {
    saved_poi(mw.registry.state(mw.world.block(p[0], p[1], p[2]))?)
}
/// The world of the bench indexed once: every block of the slab and the first blocks above it.
fn scan(reg: &mut PoiRegistry, level: &Level) {
    let mw = level.mw.0;
    reg.scanned = true;
    for x in -level.half..=level.half {
        for z in -level.half..=level.half {
            for y in -2..8 {
                if let Some(k) = classify(mw, [x, y, z]) {
                    reg.add([x, y, z], k);
                }
            }
        }
    }
    // What the first scan found is the world as it was: not an event.
    reg.now = (Vec::new(), Vec::new());
}
/// The blocks that changed: each is read again, a point appears or goes, the events of the last tick are kept for the modules.
fn poi_changes(level: Res<Level>, changed: Res<ChangedBlocks>, mut reg: ResMut<PoiRegistry>) {
    if !reg.scanned {
        scan(&mut reg, &level);
    }
    reg.last = std::mem::take(&mut reg.now);
    if changed.0.is_empty() {
        return;
    }
    let mw = level.mw.0;
    for &(x, y, z) in &changed.0 {
        let p = [x, y, z];
        match (classify(mw, p), reg.get(p)) {
            (Some(k), None) => reg.add(p, k),
            (None, Some(_)) => reg.remove(p),
            (Some(k), Some(old)) if k != old.kind => {
                reg.remove(p);
                reg.add(p, k);
            }
            _ => {}
        }
    }
}
