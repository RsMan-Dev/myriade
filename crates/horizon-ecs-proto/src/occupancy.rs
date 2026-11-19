//! Occupancy masks beside the world (suivi/ecs-noyau-collisions.md): per 16 x 16 x 16 section, per row of 16 cells in x, two
//! u16 words: the cells that are a plain unit cube (`BlockCollisionRegistry::is_plain_cube`: the same cube under every
//! context) and the cells that are something else that can collide or reach out of its cell. A cell that is neither has an
//! empty shape in every context. Built when a section is first read, dropped when one of its blocks changes
//! (`invalidate`, called where the blocks are written).
//!
//! `resolve` is the block kernel's `Entity.collide` with the collection of colliders replaced: it walks the full-cube
//! bits of the swept box in the scan's order (z, then y, then x) and feeds the same `try_resolve_step_movement_in` with the
//! same `BlockCollider::Cube` values, so the arithmetic is the kernel's. As soon as one cell of the box or of its ring is
//! "something else" (or the box is too big, or not finite), nothing is decided here: the kernel runs unchanged.
use horizon_client_render::collisions::SceneCollisions;
use horizon_core::geometry::{try_resolve_step_movement_in, Aabb, BlockCollider, Vec3};
use horizon_world::registry::BlockStateId;
use std::{
    collections::HashMap,
    hash::{BuildHasherDefault, Hasher},
    sync::{
        atomic::{AtomicU64, Ordering::Relaxed},
        Mutex,
    },
};

pub struct Section {
    full: [u16; 256],
    other: [u16; 256],
    any_full: bool,
    any_other: bool,
}
static VOID: Section = Section { full: [0; 256], other: [0; 256], any_full: false, any_other: false };

#[derive(Default)]
struct KeyHash(u64);
impl Hasher for KeyHash {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0.rotate_left(5) ^ u64::from(b)).wrapping_mul(0x517c_c1b7_2722_0a95);
        }
    }
    fn write_u64(&mut self, i: u64) {
        self.0 = i.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    }
}
type Map = HashMap<u64, Box<Section>, BuildHasherDefault<KeyHash>>;
/// A section's key: 21 bits per axis.
fn key(k: [i32; 3]) -> u64 {
    (k[0] as u64 & 0x1F_FFFF) | (k[1] as u64 & 0x1F_FFFF) << 21 | (k[2] as u64 & 0x1F_FFFF) << 42
}

struct Inner {
    map: Map,
    /// The last sections looked up (direct-mapped by key): no hashing for the sections a walking body keeps asking for.
    recent: [(u64, usize); 64],
    colliders: Vec<BlockCollider>,
}
impl Default for Inner {
    fn default() -> Self {
        Inner { map: Map::default(), recent: [(0, 0); 64], colliders: Vec::new() }
    }
}
/// The masks of one world, and how often the fast path answered.
#[derive(Default)]
pub struct Occupancy {
    inner: Mutex<Inner>,
    pub fast: AtomicU64,
    pub fallback: AtomicU64,
    pub built: AtomicU64,
}

impl Occupancy {
    /// A block of this section changed (or the chunk was replaced): the mask is rebuilt at the next read.
    pub fn invalidate(&self, section: [i32; 3]) {
        let mut g = self.inner.lock().unwrap();
        g.map.remove(&key(section));
        g.recent = [(0, 0); 64];
    }
    pub fn clear(&self) {
        let mut g = self.inner.lock().unwrap();
        g.map.clear();
        g.recent = [(0, 0); 64];
    }

    /// `SceneCollisions::resolve_in_border_with(bounds, movement, step, on_ground, None, &[])`, the same result bit for bit.
    pub fn resolve(&self, scene: SceneCollisions<'_, '_>, bounds: Aabb, movement: Vec3, max_step: f32, on_ground: bool) -> Result<Vec3, Box<dyn std::error::Error>> {
        {
            let mut g = self.inner.lock().unwrap();
            let Inner { map, recent, colliders } = &mut *g;
            colliders.clear();
            let (world, binding) = (scene.world, scene.binding);
            let r = try_resolve_step_movement_in(movement, bounds, max_step, on_ground, colliders, |b, out| {
                collect(map, recent, &self.built, world, binding, b, out)
            });
            colliders.clear();
            if let Ok(v) = r {
                self.fast.fetch_add(1, Relaxed);
                return Ok(v);
            }
        }
        self.fallback.fetch_add(1, Relaxed);
        scene.resolve_in_border_with(bounds, movement, max_step, on_ground, None, &[])
    }
}

fn classify(binding: &horizon_world::block_collision::BlockCollisionRegistry<'_>, s: BlockStateId) -> u8 {
    if !binding.may_collide(s) && !binding.reaches_out(s) {
        0
    } else if binding.is_plain_cube(s) {
        1
    } else {
        2
    }
}

fn build(world: &horizon_client_render::world::World, binding: &horizon_world::block_collision::BlockCollisionRegistry<'_>, k: [i32; 3]) -> Box<Section> {
    let Some(chunk) = world.chunks.get(&(k[0], k[2])) else { return Box::new(Section { full: [0; 256], other: [0; 256], any_full: false, any_other: false }) };
    let (c0, c1) = ([k[0] << 4, k[1] << 4, k[2] << 4], [(k[0] << 4) + 15, (k[1] << 4) + 15, (k[2] << 4) + 15]);
    let mut s = Box::new(Section { full: [0; 256], other: [0; 256], any_full: false, any_other: false });
    if !world.may_hold(c0, c1, |s| binding.may_collide(s) || binding.reaches_out(s)) {
        return s;
    }
    for y in 0..16 {
        for z in 0..16 {
            let row = (y << 4) | z;
            for x in 0..16 {
                match classify(binding, chunk.block(c0[0] + x, c0[1] + y, c0[2] + z)) {
                    1 => s.full[row as usize] |= 1 << x,
                    2 => s.other[row as usize] |= 1 << x,
                    _ => {}
                }
            }
        }
    }
    s.any_full = s.full.iter().any(|&w| w != 0);
    s.any_other = s.other.iter().any(|&w| w != 0);
    s
}

/// The colliders of the scan of `b` (what `BlockCollisionScan` finds for plain cubes), or `Err` when a cell that is not
/// empty and not a plain cube lies in the box or its ring.
fn collect(
    map: &mut Map,
    recent: &mut [(u64, usize); 64],
    built: &AtomicU64,
    world: &horizon_client_render::world::World,
    binding: &horizon_world::block_collision::BlockCollisionRegistry<'_>,
    b: Aabb,
    out: &mut Vec<BlockCollider>,
) -> Result<(), ()> {
    let (min, max) = (b.min_position(), b.max_position());
    if !min.is_finite() || !max.is_finite() {
        return Err(());
    }
    let e = 1.0e-7;
    // `floor` without the library call: the truncation, one less for a negative fraction.
    let fl = |v: f64| {
        let t = v as i64 as f64;
        if t > v { t - 1.0 } else { t }
    };
    let (fx0, fy0, fz0, fx1, fy1, fz1) = (fl(min.x - e), fl(min.y - e), fl(min.z - e), fl(max.x + e), fl(max.y + e), fl(max.z + e));
    // Beyond a few cells the kernel's own errors and budget rules apply: not decided here.
    if fx1 - fx0 > 14.0 || fy1 - fy0 > 14.0 || fz1 - fz0 > 14.0 || fx0.abs() > 2.0e9 || fz0.abs() > 2.0e9 || fy0.abs() > 2.0e9 {
        return Err(());
    }
    let (x0, y0, z0, x1, y1, z1) = (fx0 as i32, fy0 as i32, fz0 as i32, fx1 as i32, fy1 as i32, fz1 as i32);
    // The sections of the ring box.
    let (kx0, kx1, ky0, ky1, kz0, kz1) = ((x0 - 1) >> 4, (x1 + 1) >> 4, (y0 - 1) >> 4, (y1 + 1) >> 4, (z0 - 1) >> 4, (z1 + 1) >> 4);
    let (nx, ny, nz) = ((kx1 - kx0 + 1) as usize, (ky1 - ky0 + 1) as usize, (kz1 - kz0 + 1) as usize);
    // The boxes of the map's sections keep their address while the lock is held (nothing is removed during a call).
    let mut secs: [*const Section; 27] = [&VOID; 27];
    let (mut any_full, mut any_other) = (false, false);
    for (i, kx) in (kx0..=kx1).enumerate() {
        for (j, ky) in (ky0..=ky1).enumerate() {
            for (l, kz) in (kz0..=kz1).enumerate() {
                let k = key([kx, ky, kz]);
                let slot = (k.wrapping_mul(0x9E37_79B9_7F4A_7C15)   >> 58) as usize;
                let s: &Section = if recent[slot].1 != 0 && recent[slot].0 == k {
                    // SAFETY: the address of a box of the map, which keeps it until `invalidate` (that clears `recent`).
                    unsafe { &*(recent[slot].1 as *const Section) }
                } else {
                    let s: &Section = match map.get(&k) {
                        Some(s) => s,
                        None => {
                            built.fetch_add(1, Relaxed);
                            map.entry(k).or_insert_with(|| build(world, binding, [kx, ky, kz]))
                        }
                    };
                    recent[slot] = (k, s as *const Section as usize);
                    s
                };
                any_full |= s.any_full;
                any_other |= s.any_other;
                secs[(i * ny + j) * nz + l] = s;
            }
        }
    }
    let _ = nx;
    // SAFETY: see above; the pointers come from the map just read.
    let at = |kx: i32, ky: i32, kz: i32| unsafe { &*secs[(((kx - kx0) as usize * ny) + (ky - ky0) as usize) * nz + (kz - kz0) as usize] };
    // A word of the bits of x in [lo, hi] (block coordinates) within the section `kx`.
    let span = |kx: i32, lo: i32, hi: i32| -> u16 {
        let (a, c) = ((lo.max(kx << 4) - (kx << 4)) as u32, (hi.min((kx << 4) + 15) - (kx << 4)) as u32);
        if a > c { 0 } else { (((1u32 << (c - a + 1)) - 1) << a) as u16 }
    };
    if any_other {
        for z in z0 - 1..=z1 + 1 {
            for y in y0 - 1..=y1 + 1 {
                let row = (((y & 15) << 4) | (z & 15)) as usize;
                for kx in kx0..=kx1 {
                    if at(kx, y >> 4, z >> 4).other[row] & span(kx, x0 - 1, x1 + 1) != 0 {
                        return Err(());
                    }
                }
            }
        }
    }
    if !any_full {
        return Ok(());
    }
    for z in z0..=z1 {
        for y in y0..=y1 {
            let row = (((y & 15) << 4) | (z & 15)) as usize;
            for kx in kx0..=kx1 {
                let mut bits = at(kx, y >> 4, z >> 4).full[row] & span(kx, x0, x1);
                while bits != 0 {
                    let x = (kx << 4) + bits.trailing_zeros() as i32;
                    bits &= bits - 1;
                    let offset = Vec3::new(f64::from(x), f64::from(y), f64::from(z));
                    let top = Vec3::new(offset.x + 1.0, offset.y + 1.0, offset.z + 1.0);
                    if b.intersects_corners(offset, top) {
                        out.push(BlockCollider::Cube(offset));
                    }
                }
            }
        }
    }
    Ok(())
}
