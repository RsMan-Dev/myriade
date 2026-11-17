//! Prototype ECS: a cow's answers to pure world queries, kept as an ECS component (see
//! `game/mobs/cow_ecs.rs`). The entry point lends the cow's cache to this thread for the length of
//! its tick; a query that was asked before with the same arguments, in a region whose blocks have
//! not changed since (the cache is emptied when the region's stamp changes), gets its old answer.
use bevy_ecs::component::Component;
use horizon_core::geometry::{Aabb, Vec3};
use std::cell::RefCell;

type Key = (u8, [u64; 6], [u64; 3]);

#[derive(Clone, Copy)]
enum Value {
    Support(Option<[i32; 3]>),
    Fire(f32),
    Powder(bool),
}

/// What the cow's tick asked, with the answers.
#[derive(Component, Default, Clone)]
pub struct QueryCache {
    answers: Vec<(Key, Value)>,
    pub hits: u64,
    pub asks: u64,
}

impl QueryCache {
    pub fn clear(&mut self) {
        self.answers.clear();
    }
}

struct Active {
    cache: QueryCache,
    /// The cells whose blocks the stamp covers.
    lo: [i32; 3],
    hi: [i32; 3],
    /// Whether answers are used (mode 2) or only asked and kept (mode 1).
    reuse: bool,
}

thread_local! {
    static ACTIVE: RefCell<Option<Active>> = const { RefCell::new(None) };
}

/// Lend `cache` to this thread: its answers hold for the blocks of `lo..=hi`.
pub fn enter(cache: QueryCache, lo: [i32; 3], hi: [i32; 3], reuse: bool) {
    ACTIVE.with(|a| *a.borrow_mut() = Some(Active { cache, lo, hi, reuse }));
}

/// Take the cache back.
pub fn leave() -> Option<QueryCache> {
    ACTIVE.with(|a| a.borrow_mut().take().map(|a| a.cache))
}

fn bits(a: Aabb) -> [u64; 6] {
    let (lo, hi) = (a.min_position(), a.max_position());
    [
        lo.x.to_bits(),
        lo.y.to_bits(),
        lo.z.to_bits(),
        hi.x.to_bits(),
        hi.y.to_bits(),
        hi.z.to_bits(),
    ]
}

fn ask(
    kind: u8,
    area: Aabb,
    from: Vec3,
    margin: i32,
    compute: impl FnOnce() -> Value,
) -> Value {
    ACTIVE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(active) = slot.as_mut() else {
            return compute();
        };
        // The cells the query reads: `area` grown by `margin` cells each way.
        let (lo, hi) = (area.min_position(), area.max_position());
        let inside = (lo.x - 1.0e-7).floor() as i32 - margin >= active.lo[0]
            && (lo.y - 1.0e-7).floor() as i32 - margin >= active.lo[1]
            && (lo.z - 1.0e-7).floor() as i32 - margin >= active.lo[2]
            && (hi.x + 1.0e-7).floor() as i32 + margin <= active.hi[0]
            && (hi.y + 1.0e-7).floor() as i32 + margin <= active.hi[1]
            && (hi.z + 1.0e-7).floor() as i32 + margin <= active.hi[2];
        if !inside {
            drop(slot);
            return compute();
        }
        let key = (kind, bits(area), [from.x.to_bits(), from.y.to_bits(), from.z.to_bits()]);
        active.cache.asks += 1;
        if active.reuse
            && let Some((_, answer)) = active.cache.answers.iter().find(|(k, _)| *k == key)
        {
            active.cache.hits += 1;
            return *answer;
        }
        drop(slot);
        let answer = compute();
        ACTIVE.with(|slot| {
            if let Some(active) = slot.borrow_mut().as_mut() {
                if active.cache.answers.len() >= 12 {
                    active.cache.answers.remove(0);
                }
                active.cache.answers.push((key, answer));
            }
        });
        answer
    })
}

/// `MobWorld::supporting_block(area, from)` through the lent cache, if any.
pub fn support(
    area: Aabb,
    from: Vec3,
    compute: impl FnOnce() -> Option<[i32; 3]>,
) -> Option<[i32; 3]> {
    match ask(0, area, from, 1, || Value::Support(compute())) {
        Value::Support(v) => v,
        _ => unreachable!("kind 0 holds a support"),
    }
}

/// `environment::fire_contact(world, registry, bounds)`.
pub fn fire(bounds: Aabb, compute: impl FnOnce() -> f32) -> f32 {
    match ask(1, bounds, Vec3::ZERO, 1, || Value::Fire(compute())) {
        Value::Fire(v) => v,
        _ => unreachable!("kind 1 holds a fire"),
    }
}

/// `freezing::in_powder_snow` of a mob whose box is `bounds`.
pub fn powder_snow(bounds: Aabb, compute: impl FnOnce() -> bool) -> bool {
    match ask(2, bounds, Vec3::ZERO, 1, || Value::Powder(compute())) {
        Value::Powder(v) => v,
        _ => unreachable!("kind 2 holds a powder"),
    }
}

/// The cache and the stamp of its region, when they live in the mob (mode 3, no ECS): a clone of the
/// mob starts without them, and they are not part of its `Debug` text.
#[derive(Default)]
pub struct MobMemo(pub Option<Box<(Option<crate::world::RegionStamp>, QueryCache)>>);
impl Clone for MobMemo {
    fn clone(&self) -> Self {
        Self(None)
    }
}
impl std::fmt::Debug for MobMemo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MobMemo")
    }
}
