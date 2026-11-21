//! `ClosestHomeAsWalkTarget` (`villager_brain.rs:2225`): the nearest `Home` POI within `range`; the retry wait is a memory with a ttl.
use crate::behaviours::block_reach::block_at;
use crate::brain::*;
use crate::goals::with_rng;
use crate::poi::PoiRegistry;
use horizon_world::poi::PoiType;

/// The wait after an attempt (`lastUpdate`): a memory that lasts this long.
pub static RETRY: MemKey = MemKey::new("horizon:village_home_retry", Shape::Unit);

pub struct Params {
    pub range: i32,
    pub retry: u64,
    /// The villager's `Home`: a villager that has one looks for no walk target (`mm.home.present()`).
    pub home: &'static MemKey,
}

pub static BEHAVIOUR: Behaviour = Behaviour { name: "minecraft:closest_home_as_walk_target", mems: &[&WALK_TARGET, &RETRY], one_shot: true, can_start, ..Behaviour::DEFAULT };

fn can_start(w: &mut World, e: Entity, c: &Ctx) -> bool {
    let p = c.get::<Params>();
    if present(w, e, &WALK_TARGET) || present(w, e, &RETRY) || present(w, e, p.home) {
        return false;
    }
    let Some(here) = block_at(w, e) else { return false };
    let r2 = f64::from(p.range * p.range);
    let homes = w.resource::<PoiRegistry>().in_range(here, p.range, false, |k| k == PoiType::Home);
    let home = homes.into_iter().map(|(q, _)| q).filter(|q| dist2(*q, here) <= r2).min_by(|a, b| dist2(*a, here).total_cmp(&dist2(*b, here)));
    let Some(home) = home else { return false };
    if dist2(home, here) <= 4.0 {
        return false;
    }
    let wait = p.retry + with_rng(w, e, |r| r.next_int(20)) as u64;
    set(w, e, &RETRY, MemValue::Unit, Some(wait));
    set(w, e, &WALK_TARGET, MemValue::Walk(WalkTarget { target: Tracker::Block(home), speed: 0.5, close: 1 }), None);
    true
}

/// `dist_sqr` of the Mob (`manhattan_sq`: the sum of the squared differences of the three axes).
fn dist2(a: [i32; 3], b: [i32; 3]) -> f64 {
    (0..3).map(|i| f64::from(a[i] - b[i]).powi(2)).sum()
}

/// The bed heads within `radius` of the entity, nearest first (`homes_in_range`, the `hide` behaviour's places).
pub fn homes(w: &World, e: Entity, radius: i32) -> Vec<[i32; 3]> {
    let Some(here) = block_at(w, e) else { return Vec::new() };
    w.resource::<PoiRegistry>().in_range(here, radius, false, |k| k == PoiType::Home).into_iter().map(|(q, _)| q).collect()
}
