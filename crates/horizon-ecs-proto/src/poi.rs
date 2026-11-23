//! The points of interest (`PoiManager`): the blocks villagers and the creatures of a village go to or live by (beds, job sites, bells), their
//! types (Horizon's own `horizon_world::poi`), their tickets (who has taken one), the search of the ones near a place, and the distance
//! of a place to the nearest village, in sections (`PoiManager.sectionsToVillage`). The registry is written by events: a block that changed
//! (`ChangedBlocks`) is read again, a point appears or goes (`poi_added` / `poi_removed`, readable the tick after like the other events);
//! nothing scans the world each tick. The bench's world is indexed once at the first use (a server would index each chunk as it loads).
//! A creature asks "is it in a village" by the named predicate `horizon:in_village`.
use crate::cow::*;
use crate::hostile::ChangedBlocks;
use crate::rules::poi::{is_village, nearby_order, section, within_cube, NearbyKey};
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
    /// Every point of each section (village or not) with a copy of its `Poi`, kept equal to `points`: what `in_range` searches without a lookup per point.
    by_section: horizon_core::fast_hash::FxHashMap<[i32; 3], Vec<([i32; 3], Poi)>>,
    scanned: bool,
    /// The points that appeared / went this tick, and the ones of the tick before (what the modules read).
    now: (Vec<[i32; 3]>, Vec<[i32; 3]>),
    last: (Vec<[i32; 3]>, Vec<[i32; 3]>),
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
        let poi = Poi { kind, taken: 0 };
        if self.points.insert(p, poi).is_none() {
            if is_village(kind) {
                *self.sections.entry(section(p)).or_default() += 1;
            }
            self.by_section.entry(section(p)).or_default().push((p, poi));
            self.now.0.push(p);
        } else {
            self.sync(p);
        }
    }
    /// Copies the `Poi` of `p` into its section list.
    fn sync(&mut self, p: [i32; 3]) {
        if let Some(poi) = self.points.get(&p).copied()
            && let Some(e) = self.by_section.get_mut(&section(p)).and_then(|v| v.iter_mut().find(|e| e.0 == p))
        {
            e.1 = poi;
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
            if let Some(v) = self.by_section.get_mut(&section(p))
                && let Some(i) = v.iter().position(|q| q.0 == p)
            {
                v.swap_remove(i);
            }
            if self.by_section.get(&section(p)).is_some_and(Vec::is_empty) {
                self.by_section.remove(&section(p));
            }
            self.now.1.push(p);
        }
    }
    /// The sections the cube `±radius` around `center` touches, among those holding points (all of them when there are fewer).
    fn sections_near(&self, center: [i32; 3], radius: i32) -> Vec<[i32; 3]> {
        let lo = center.map(|c| c.saturating_sub(radius) >> 4);
        let hi = center.map(|c| c.saturating_add(radius) >> 4);
        let cells: i64 = (0..3).map(|i| i64::from(hi[i] - lo[i] + 1).max(0)).product();
        if cells > self.by_section.len() as i64 {
            return self.by_section.keys().copied().filter(|s| (0..3).all(|i| (lo[i]..=hi[i]).contains(&s[i]))).collect();
        }
        let mut v = Vec::new();
        for z in lo[2]..=hi[2] {
            for y in lo[1]..=hi[1] {
                for x in lo[0]..=hi[0] {
                    if self.by_section.contains_key(&[x, y, z]) {
                        v.push([x, y, z]);
                    }
                }
            }
        }
        v
    }
    /// `PoiManager.getInRange` (with `HAS_SPACE` when `free`): the points of the accepted types within `radius` blocks of `center` on each axis,
    /// nearest first, ties by chunk (z, then x) then y, z, x.
    pub fn in_range(&self, center: [i32; 3], radius: i32, free: bool, accept: impl Fn(PoiType) -> bool) -> Vec<([i32; 3], PoiType)> {
        let mut v: Vec<(NearbyKey, [i32; 3], PoiType)> = Vec::new();
        for s in self.sections_near(center, radius) {
            for &(p, poi) in &self.by_section[&s] {
                if within_cube(p, center, radius) && accept(poi.kind) && (!free || poi.taken < poi.kind.max_tickets()) {
                    v.push((nearby_order(p, center), p, poi.kind));
                }
            }
        }
        v.sort_by_key(|x| x.0);
        v.into_iter().map(|x| (x.1, x.2)).collect()
    }
    /// The reference scan of `in_range` (every point), kept to check the index against it.
    #[cfg(test)]
    fn in_range_scan(&self, center: [i32; 3], radius: i32, free: bool, accept: impl Fn(PoiType) -> bool) -> Vec<([i32; 3], PoiType)> {
        let mut v: Vec<(NearbyKey, [i32; 3], PoiType)> = self
            .points
            .iter()
            .filter(|(p, poi)| within_cube(**p, center, radius) && accept(poi.kind) && (!free || poi.taken < poi.kind.max_tickets()))
            .map(|(p, poi)| (nearby_order(*p, center), *p, poi.kind))
            .collect();
        v.sort_by_key(|x| x.0);
        v.into_iter().map(|x| (x.1, x.2)).collect()
    }
    /// `PoiManager.take`: a ticket of the point, if one is free.
    pub fn acquire(&mut self, p: [i32; 3]) -> bool {
        match self.points.get_mut(&p) {
            Some(poi) if poi.taken < poi.kind.max_tickets() => {
                poi.taken += 1;
                self.sync(p);
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
                self.sync(p);
                true
            }
            _ => false,
        }
    }
    /// `PoiManager.sectionsToVillage`: the distance in sections (the largest of the three axes) to the nearest section with a village
    /// point, 7 when there is none within that.
    /// `ServerLevel.isVillage` for a section: at most one section from a village.
    pub fn is_village_section(&self, s: [i32; 3]) -> bool {
        self.sections_to_village(s) <= 1
    }
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
    w.init_resource::<crate::behaviours::poi_claim::ClaimIndex>();
}
pub fn register(s: &mut bevy_ecs::schedule::Schedule) {
    s.add_systems(poi_changes.after(block_changes).before(crate::targeting::sight_eval));
}
/// The block at `p` as a point of interest, if it is one.
fn classify(mw: &myriade_engine_lib::mobs::MobWorld<'static>, p: [i32; 3]) -> Option<PoiType> {
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

impl PoiRegistry {
    /// `occupied_pois_in_range`: the points within `radius` of `center` (a cube, as `getInRange`) that someone holds a ticket of.
    pub fn occupied_in_range(&self, center: [i32; 3], radius: i32) -> Vec<[i32; 3]> {
        let mut v: Vec<[i32; 3]> = self.points.iter().filter(|(p, poi)| poi.taken > 0 && within_cube(**p, center, radius)).map(|(p, _)| *p).collect();
        v.sort_by_key(|p| nearby_order(*p, center));
        v
    }
    /// The sections that hold a village point.
    pub fn village_sections(&self) -> impl Iterator<Item = [i32; 3]> + '_ {
        self.sections.keys().copied()
    }
    /// The village points someone holds a ticket of (job sites, beds, bells), in no set order.
    pub fn held_points(&self) -> Vec<[i32; 3]> {
        self.points.iter().filter(|(_, poi)| poi.taken > 0 && is_village(poi.kind)).map(|(p, _)| *p).collect()
    }
    /// The sections of the village only the points with a ticket taken make (what the `Mob` reads from the villagers' memories).
    pub fn held_sections(&self) -> Vec<[i32; 3]> {
        let mut v: Vec<[i32; 3]> = Vec::new();
        for (p, poi) in &self.points {
            if poi.taken > 0 && is_village(poi.kind) && !v.contains(&section(*p)) {
                v.push(section(*p));
            }
        }
        v
    }
}
#[cfg(test)]
mod index_tests {
    use super::*;
    const KINDS: [PoiType; 21] = [
        PoiType::Armorer,
        PoiType::Butcher,
        PoiType::Cartographer,
        PoiType::Cleric,
        PoiType::Farmer,
        PoiType::Fisherman,
        PoiType::Fletcher,
        PoiType::Leatherworker,
        PoiType::Librarian,
        PoiType::Mason,
        PoiType::Shepherd,
        PoiType::Toolsmith,
        PoiType::Weaponsmith,
        PoiType::Meeting,
        PoiType::Home,
        PoiType::Beehive,
        PoiType::BeeNest,
        PoiType::NetherPortal,
        PoiType::Lodestone,
        PoiType::TestInstance,
        PoiType::LightningRod,
    ];
    const RADII: [i32; 7] = [-1, 0, 1, 15, 16, 48, 200];
    fn next(s: &mut u64) -> u64 {
        *s ^= *s << 13;
        *s ^= *s >> 7;
        *s ^= *s << 17;
        *s
    }
    fn pick(s: &mut u64, n: usize) -> usize {
        (next(s) % n as u64) as usize
    }
    fn coord(s: &mut u64, lo: i32, hi: i32) -> i32 {
        lo + pick(s, (hi - lo + 1) as usize) as i32
    }
    fn point(s: &mut u64, lo: i32, hi: i32) -> [i32; 3] {
        [coord(s, lo, hi), coord(s, lo, hi), coord(s, lo, hi)]
    }
    #[inline(never)]
    fn bench_poi_new(reg: &PoiRegistry, centers: &[[i32; 3]], accept: fn(PoiType) -> bool) -> usize {
        centers.iter().map(|&c| reg.in_range(c, 48, true, accept).len()).sum()
    }
    #[inline(never)]
    fn bench_poi_scan(reg: &PoiRegistry, centers: &[[i32; 3]], accept: fn(PoiType) -> bool) -> usize {
        centers.iter().map(|&c| reg.in_range_scan(c, 48, true, accept).len()).sum()
    }
    /// A village-sized registry (700 points over 200 blocks) searched by 100 villagers: `--toggle-collect=*bench_poi_*`.
    fn poi_bench(accept: fn(PoiType) -> bool) {
        let mut s = 0x2545_F491_4F6C_DD1Du64;
        let mut reg = PoiRegistry::default();
        for _ in 0..700 {
            reg.add([coord(&mut s, -100, 100), coord(&mut s, 60, 75), coord(&mut s, -100, 100)], KINDS[pick(&mut s, KINDS.len())]);
        }
        let centers: Vec<[i32; 3]> = (0..100).map(|_| [coord(&mut s, -100, 100), coord(&mut s, 60, 75), coord(&mut s, -100, 100)]).collect();
        assert_eq!(bench_poi_new(&reg, &centers, accept), bench_poi_scan(&reg, &centers, accept));
    }
    #[test]
    #[ignore = "bench"]
    fn poi_bench_jobs() {
        poi_bench(PoiType::is_job);
    }
    #[test]
    #[ignore = "bench"]
    fn poi_bench_homes() {
        poi_bench(|k| k == PoiType::Home);
    }

    #[test]
    fn in_range_equals_scan() {
        let mut s = 0x2545_F491_4F6C_DD1Du64;
        for _ in 0..3000 {
            let mut reg = PoiRegistry::default();
            let mut pts: Vec<[i32; 3]> = Vec::new();
            for _ in 0..pick(&mut s, 401) {
                let p = point(&mut s, -80, 80);
                reg.add(p, KINDS[pick(&mut s, KINDS.len())]);
                pts.push(p);
            }
            for _ in 0..pick(&mut s, 200) {
                let p = if pts.is_empty() || pick(&mut s, 2) == 0 { point(&mut s, -80, 80) } else { pts[pick(&mut s, pts.len())] };
                match pick(&mut s, 4) {
                    0 => {
                        reg.add(p, KINDS[pick(&mut s, KINDS.len())]);
                        pts.push(p);
                    }
                    1 => reg.remove(p),
                    2 => {
                        reg.acquire(p);
                    }
                    _ => {
                        reg.release(p);
                    }
                }
            }
            for _ in 0..20 {
                let center = point(&mut s, -100, 100);
                let radius = RADII[pick(&mut s, RADII.len())];
                let free = pick(&mut s, 2) == 1;
                let accept: fn(PoiType) -> bool = match pick(&mut s, 4) {
                    0 => |_| true,
                    1 => PoiType::is_job,
                    2 => |k| k == PoiType::Home,
                    _ => |_| false,
                };
                assert_eq!(reg.in_range(center, radius, free, accept), reg.in_range_scan(center, radius, free, accept));
            }
        }
    }
}
