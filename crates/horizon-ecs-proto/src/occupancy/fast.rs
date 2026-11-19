//! The lock-free path of `Occupancy::resolve` for what most calls meet: air and plain cubes. The same resolution as the kernel's
//! `try_resolve_step_movement_in` over unit cubes (the same arithmetic on the same numbers, in the same order, written on plain arrays
//! of numbers instead of `Aabb` values, whose constructor orders the corners again at every step), reading the sections through a cache of
//! the calling thread. It declines (`None`: the caller takes the general path under the lock) as soon as anything else is near: a
//! shape that is not a plain cube in the padded box's sections, a section the thread has not met (it is fetched, which takes the lock
//! once), a box that is too big or not finite.
use super::{Binding, Occupancy, Section, World};
use horizon_client_render::collisions::SceneCollisions;
use horizon_core::geometry::{Aabb, Vec3};
use std::sync::atomic::Ordering::Relaxed;

const EPS: f64 = 1.0e-7;
const MAX_CUBES: usize = 64;
const SLOTS: usize = 256;

/// Which neighbours (bit of `rel`) a padded box touches, by the sets of offsets it reaches on each axis (3-bit masks of dx + 1, dy + 1, dz + 1):
/// the cells of the ring beyond a section's face lie in the neighbours of that side.
const TOUCH: [u32; 512] = {
    let mut t = [0u32; 512];
    let (mut a, mut b, mut c) = (0, 0, 0);
    while a < 8 {
        b = 0;
        while b < 8 {
            c = 0;
            while c < 8 {
                let mut m = 0u32;
                let mut dx = 0;
                while dx < 3 {
                    let mut dy = 0;
                    while dy < 3 {
                        let mut dz = 0;
                        while dz < 3 {
                            if (a >> dx) & 1 == 1 && (b >> dy) & 1 == 1 && (c >> dz) & 1 == 1 && !(dx == 1 && dy == 1 && dz == 1) {
                                m |= 1 << (dx * 9 + dy * 3 + dz);
                            }
                            dz += 1;
                        }
                        dy += 1;
                    }
                    dx += 1;
                }
                t[a * 64 + b * 8 + c] = m;
                c += 1;
            }
            b += 1;
        }
        a += 1;
    }
    t
};

struct Tl {
    owner: u64,
    generation: u64,
    recent: [(u64, *const Section); SLOTS],
    cubes: [[f64; 3]; MAX_CUBES],
}
thread_local! {
    static TL: std::cell::UnsafeCell<Tl> = const { std::cell::UnsafeCell::new(Tl { owner: 0, generation: 0, recent: [(0, std::ptr::null()); SLOTS], cubes: [[0.0; 3]; MAX_CUBES] }) };
}

#[inline(always)]
fn jmin(a: f64, b: f64) -> f64 {
    if a.is_nan() {
        a
    } else if b.is_nan() {
        b
    } else if a == 0.0 && b == 0.0 {
        f64::from_bits(a.to_bits() | b.to_bits())
    } else if a <= b {
        a
    } else {
        b
    }
}
#[inline(always)]
fn jmax(a: f64, b: f64) -> f64 {
    if a.is_nan() {
        a
    } else if b.is_nan() {
        b
    } else if a == 0.0 && b == 0.0 {
        f64::from_bits(a.to_bits() & b.to_bits())
    } else if a >= b {
        a
    } else {
        b
    }
}

/// One axis of `resolve_borrowed_shape_movement` over unit cubes: the amount along `AXIS` after each cube, in order. A cube the box does not
/// overlap on the two other axes leaves the amount as it is (`collide_unit_cube` answers the amount itself), so it is passed with two
/// comparisons; the heights and depths of a row are decided once per row.
#[inline(always)]
fn along<const AXIS: usize>(cubes: &[[f64; 3]], lo: &[f64; 3], hi: &[f64; 3], a: &[f64; 3], b: &[f64; 3], mut amount: f64) -> f64 {
    // The overlap on y and on z is the same for the cubes of a row (they share both): decided when it changes.
    let (mut last_y, mut last_z) = (f64::NAN, f64::NAN);
    let (mut ok_y, mut ok_z) = (false, false);
    let mut check = true;
    for c in cubes {
        if check && amount.abs() < EPS {
            return 0.0;
        }
        check = false;
        if AXIS != 1 && c[1] != last_y {
            last_y = c[1];
            ok_y = a[1] < last_y + 1.0 && last_y <= b[1];
        }
        if AXIS != 2 && c[2] != last_z {
            last_z = c[2];
            ok_z = a[2] < last_z + 1.0 && last_z <= b[2];
        }
        let ok = match AXIS {
            0 => ok_y && ok_z,
            1 => ok_z && a[0] < c[0] + 1.0 && c[0] <= b[0],
            _ => ok_y && a[0] < c[0] + 1.0 && c[0] <= b[0],
        };
        if !ok {
            continue;
        }
        let off = c[AXIS];
        let before = amount;
        if amount > 0.0 {
            if off > b[AXIS] {
                let distance = off - hi[AXIS];
                amount = if distance >= -EPS { jmin(amount, distance) } else { amount };
            }
        } else if amount < 0.0 && off + 1.0 <= a[AXIS] {
            let distance = 1.0 + off - lo[AXIS];
            amount = if distance <= EPS { jmax(amount, distance) } else { amount };
        }
        check = amount.to_bits() != before.to_bits();
    }
    amount
}

/// `resolve_borrowed_shape_movement` over unit cubes.
#[inline(always)]
fn resolve_cubes(mv: [f64; 3], lo: [f64; 3], hi: [f64; 3], cubes: &[[f64; 3]]) -> [f64; 3] {
    if cubes.is_empty() {
        return mv;
    }
    let mut result = [0.0; 3];
    let order = if mv[0].abs() < mv[2].abs() { [1, 2, 0] } else { [1, 0, 2] };
    for axis in order {
        let amount = mv[axis];
        if amount == 0.0 {
            continue;
        }
        let mlo = [lo[0] + result[0], lo[1] + result[1], lo[2] + result[2]];
        let mhi = [hi[0] + result[0], hi[1] + result[1], hi[2] + result[2]];
        let a = [mlo[0] + EPS, mlo[1] + EPS, mlo[2] + EPS];
        let b = [mhi[0] - EPS, mhi[1] - EPS, mhi[2] - EPS];
        result[axis] = match axis {
            0 => along::<0>(cubes, &mlo, &mhi, &a, &b, amount),
            1 => along::<1>(cubes, &mlo, &mhi, &a, &b, amount),
            _ => along::<2>(cubes, &mlo, &mhi, &a, &b, amount),
        };
    }
    result
}

/// `Aabb::expand_towards` on corners.
#[inline(always)]
fn expand(lo: [f64; 3], hi: [f64; 3], v: [f64; 3]) -> ([f64; 3], [f64; 3]) {
    let (mut lo, mut hi) = (lo, hi);
    for i in 0..3 {
        if v[i] < 0.0 {
            lo[i] += v[i];
        } else if v[i] > 0.0 {
            hi[i] += v[i];
        }
    }
    (lo, hi)
}

/// `floor`, as an integer, with no library call and no range fix-up: `i64::MIN` for what does not fit (a NaN, an infinity, 2^63 and more).
#[inline(always)]
fn floor_i(v: f64) -> i64 {
    #[cfg(target_arch = "x86_64")]
    // SAFETY: SSE2 is part of the x86-64 baseline; the instruction answers `i64::MIN` for what it cannot convert.
    let t = unsafe { core::arch::x86_64::_mm_cvttsd_si64(core::arch::x86_64::_mm_set_sd(v)) };
    #[cfg(not(target_arch = "x86_64"))]
    let t = if v.is_finite() && v.abs() < 9.0e18 { v as i64 } else { i64::MIN };
    t - i64::from((t as f64) > v)
}

pub(super) fn resolve(occ: &Occupancy, scene: &SceneCollisions<'_, '_>, bounds: Aabb, movement: Vec3, max_step: f32, on_ground: bool) -> Option<Vec3> {
    TL.with(|tl| {
        // SAFETY: the thread's own cache, not shared; nothing below calls back into this function.
        let tl = unsafe { &mut *tl.get() };
        let generation = occ.generation.load(Relaxed);
        if tl.owner != occ.id || tl.generation != generation {
            tl.owner = occ.id;
            tl.generation = generation;
            tl.recent = [(0, std::ptr::null()); SLOTS];
        }
        resolve_in(tl, occ, scene, bounds, movement, max_step, on_ground)
    })
}

#[inline(always)]
fn resolve_in(tl: &mut Tl, occ: &Occupancy, scene: &SceneCollisions<'_, '_>, bounds: Aabb, movement: Vec3, max_step: f32, on_ground: bool) -> Option<Vec3> {
    let (min, max) = (bounds.min_position(), bounds.max_position());
    let (lo, hi) = ([min.x, min.y, min.z], [max.x, max.y, max.z]);
    let mv = [movement.x, movement.y, movement.z];
    let subject = scene.subject.is_some();
    let Tl { recent, cubes, .. } = tl;
    let mut n = 0;
    let resolved = if movement.length_squared() == 0.0 {
        mv
    } else {
        let (slo, shi) = expand(lo, hi, mv);
        n = collect(recent, cubes, occ, scene.world, scene.binding, subject, slo, shi)?;
        resolve_cubes(mv, lo, hi, &cubes[..n])
    };
    let landed = mv[1] != resolved[1] && mv[1] < 0.0;
    if max_step > 0.0 && (landed || on_ground) && (mv[0] != resolved[0] || mv[2] != resolved[2]) {
        let (mut blo, mut bhi) = (lo, hi);
        if landed {
            blo[1] += resolved[1];
            bhi[1] += resolved[1];
        }
        let (mut qlo, mut qhi) = expand(blo, bhi, [mv[0], f64::from(max_step), mv[2]]);
        if !landed {
            (qlo, qhi) = expand(qlo, qhi, [0.0, -(1.0e-5_f32 as f64), 0.0]);
        }
        n = collect(recent, cubes, occ, scene.world, scene.binding, subject, qlo, qhi)?;
        let cubes = &cubes[..n];
        // `borrowed_step_heights`: each cube's two Y boundaries, from the feet, as floats, without repeats, ascending.
        let mut heights = [0.0f32; 24];
        let mut count = 0;
        let current = resolved[1] as f32;
        for c in cubes {
            for k in 0..2 {
                let height = ((k as f64 + c[1]) - blo[1]) as f32;
                if height < 0.0 || height == current {
                    continue;
                }
                if height > max_step {
                    break;
                }
                let height = if height.is_nan() { f32::NAN } else { height };
                if !heights[..count].iter().any(|old| old.to_bits() == height.to_bits()) {
                    if count == heights.len() {
                        return None;
                    }
                    heights[count] = height;
                    count += 1;
                }
            }
        }
        heights[..count].sort_unstable_by(f32::total_cmp);
        for &height in &heights[..count] {
            let candidate = resolve_cubes([mv[0], f64::from(height), mv[2]], blo, bhi, cubes);
            if candidate[0] * candidate[0] + candidate[2] * candidate[2] > resolved[0] * resolved[0] + resolved[2] * resolved[2] {
                let drop = lo[1] - blo[1];
                return Some(Vec3::new(candidate[0] + -0.0, candidate[1] + -drop, candidate[2] + -0.0));
            }
        }
    }
    Some(Vec3::new(resolved[0], resolved[1], resolved[2]))
}

/// The unit cubes the kernel's scan of the box finds, in its order, written to `out`; their number, or `None` when anything else is near.
#[allow(clippy::too_many_arguments)]
#[inline(always)]
fn collect(recent: &mut [(u64, *const Section); SLOTS], out: &mut [[f64; 3]; MAX_CUBES], occ: &Occupancy, world: &World, binding: &Binding<'_>, subject: bool, lo: [f64; 3], hi: [f64; 3]) -> Option<usize> {
    let (x0, y0, z0, x1, y1, z1) = (floor_i(lo[0] - EPS), floor_i(lo[1] - EPS), floor_i(lo[2] - EPS), floor_i(hi[0] + EPS), floor_i(hi[1] + EPS), floor_i(hi[2] + EPS));
    // Beyond a few cells the kernel's own errors and budget rules apply; a value that is not a number is `i64::MIN` and fails here.
    let fits = |t: i64| (t.wrapping_add(2_000_000_000) as u64) <= 4_000_000_000;
    if !(fits(x0) & fits(y0) & fits(z0)) || x1 < x0 || y1 < y0 || z1 < z0 || x1 - x0 > 14 || y1 - y0 > 14 || z1 - z0 > 14 {
        return None;
    }
    let (x0, y0, z0, x1, y1, z1) = (x0 as i32, y0 as i32, z0 as i32, x1 as i32, y1 as i32, z1 as i32);
    // The box inside one section, and no section around holds a shape that reaches out of its cell: that section's words are all there is to read.
    if x0 >> 4 == x1 >> 4 && y0 >> 4 == y1 >> 4 && z0 >> 4 == z1 >> 4 {
        let key = super::key([x0 >> 4, y0 >> 4, z0 >> 4]);
        let slot = (key.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 56) as usize;
        let p = if recent[slot].0 == key && !recent[slot].1.is_null() { recent[slot].1 } else { fetch(recent, occ, world, binding, key, [x0 >> 4, y0 >> 4, z0 >> 4], slot) };
        // SAFETY: as below.
        let s = unsafe { &*p };
        let f = s.flags.load(Relaxed);
        // The neighbours the box's ring reaches into (those whose face it touches).
        let sides = |a: i32, b: i32| 2 | usize::from(a & 15 == 0) | usize::from(b & 15 == 15) << 2;
        let touched = TOUCH[sides(x0, x1) << 6 | sides(y0, y1) << 3 | sides(z0, z1)];
        if f & 16 == 0 || s.halo.load(Relaxed) & touched == 0 {
            if f & 1 == 0 {
                return Some(0);
            }
            let dirty = f & 2 != 0 || (subject && f & 8 != 0);
            let whole = (((1u32 << (x1 - x0 + 1)) - 1) << (x0 & 15)) as u16;
            let mut m = whole;
            if lo[0] >= f64::from(x0) + 1.0 {
                m &= !(1 << (x0 & 15));
            }
            if hi[0] <= f64::from(x1) {
                m &= !(1 << (x1 & 15));
            }
            // The rows the box keeps: those of its range, less its two faces' when the box only grazes them.
            let (ya, yb) = (if lo[1] < f64::from(y0) + 1.0 { y0 } else { y0 + 1 }, if hi[1] > f64::from(y1) { y1 } else { y1 - 1 });
            let (za, zb) = (if lo[2] < f64::from(z0) + 1.0 { z0 } else { z0 + 1 }, if hi[2] > f64::from(z1) { z1 } else { z1 - 1 });
            let (bx, mut n) = (f64::from((x0 >> 4) << 4), 0);
            for z in za..=zb {
                let (zr, zf) = (z & 15, f64::from(z));
                for y in ya..=yb {
                    let row = (((y & 15) << 4) | zr) as usize;
                    if dirty && ((s.near[row] ^ s.full[row]) | if subject { s.liquid[row] } else { 0 }) & whole != 0 {
                        return None;
                    }
                    let mut bits = s.full[row] & m;
                    if bits != 0 {
                        // A row holds at most 16 cubes: room for them is checked once.
                        if n > MAX_CUBES - 16 {
                            return None;
                        }
                        let yf = f64::from(y);
                        while bits != 0 {
                            let bit = bits.trailing_zeros();
                            bits &= bits - 1;
                            // SAFETY: n <= MAX_CUBES - 16 on entering the row and a row adds at most 16.
                            *unsafe { out.get_unchecked_mut(n) } = [bx + f64::from(bit), yf, zf];
                            n += 1;
                        }
                    }
                }
            }
            // A shape that reaches out of its cell in this section: the ring's cells in it are looked at (those in neighbours hold none).
            if f & 4 != 0 {
                let (sx, sy, sz) = ((x0 >> 4) << 4, (y0 >> 4) << 4, (z0 >> 4) << 4);
                let (ax, bx) = ((x0 - 1).max(sx) - sx, (x1 + 1).min(sx + 15) - sx);
                let pm = (((1u32 << (bx - ax + 1)) - 1) << ax) as u16;
                for z in (z0 - 1).max(sz)..=(z1 + 1).min(sz + 15) {
                    for y in (y0 - 1).max(sy)..=(y1 + 1).min(sy + 15) {
                        let row = (((y & 15) << 4) | (z & 15)) as usize;
                        if (s.large[row] | s.piston[row]) & pm != 0 {
                            return None;
                        }
                    }
                }
            }
            return Some(n);
        }
    }
    let (kx0, kx1, ky0, ky1, kz0, kz1) = ((x0 - 1) >> 4, (x1 + 1) >> 4, (y0 - 1) >> 4, (y1 + 1) >> 4, (z0 - 1) >> 4, (z1 + 1) >> 4);
    let (ny, nz) = ((ky1 - ky0 + 1) as usize, (kz1 - kz0 + 1) as usize);
    let single = kx0 == kx1 && ky0 == ky1 && kz0 == kz1;
    let mut secs: [*const Section; 27] = [std::ptr::null(); 27];
    let mut flags = 0u8;
    let mut first: *const Section = std::ptr::null();
    {
        let mut m = 0;
        for kx in kx0..=kx1 {
            for ky in ky0..=ky1 {
                for kz in kz0..=kz1 {
                    let key = super::key([kx, ky, kz]);
                    let slot = (key.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 56) as usize;
                    let p = if recent[slot].0 == key && !recent[slot].1.is_null() { recent[slot].1 } else { fetch(recent, occ, world, binding, key, [kx, ky, kz], slot) };
                    // SAFETY: a box of the map's, kept until `clear` (which changes the generation the cache is checked against).
                    flags |= unsafe { (*p).flags.load(Relaxed) };
                    if single {
                        first = p;
                    } else {
                        secs[m] = p;
                        m += 1;
                    }
                }
            }
        }
    }
    if flags & 1 == 0 {
        return Some(0);
    }
    // SAFETY: as above.
    let at = |kx: i32, ky: i32, kz: i32| -> &Section {
        if single {
            unsafe { &*first }
        } else {
            unsafe { &*secs[(((kx - kx0) as usize * ny) + (ky - ky0) as usize) * nz + (kz - kz0) as usize] }
        }
    };
    let span = |kx: i32, lo: i32, hi: i32| -> u16 {
        let (a, c) = (lo.max(kx << 4) - (kx << 4), hi.min((kx << 4) + 15) - (kx << 4));
        if a > c { 0 } else { (((1u32 << (c - a + 1)) - 1) << a) as u16 }
    };
    // A section that has held a shape other than a cube (or a liquid, for a body that stands on fluids): the words of the box's cells are
    // looked at, and the call is declined if one is set. One that has held a shape reaching out of its cell: the same for the box and its ring.
    let dirty_inner = flags & 2 != 0 || (subject && flags & 8 != 0);
    let mut inner = [0u16; 3];
    for (i, kx) in (kx0..=kx1).enumerate() {
        inner[i] = span(kx, x0, x1);
    }
    if dirty_inner {
        for z in z0..=z1 {
            for y in y0..=y1 {
                let row = (((y & 15) << 4) | (z & 15)) as usize;
                for (i, kx) in (kx0..=kx1).enumerate() {
                    let s = at(kx, y >> 4, z >> 4);
                    if (s.near[row] ^ s.full[row] | if subject { s.liquid[row] } else { 0 }) & inner[i] != 0 {
                        return None;
                    }
                }
            }
        }
    }
    if flags & 4 != 0 {
        let mut padded = [0u16; 3];
        for (i, kx) in (kx0..=kx1).enumerate() {
            padded[i] = span(kx, x0 - 1, x1 + 1);
        }
        for z in z0 - 1..=z1 + 1 {
            for y in y0 - 1..=y1 + 1 {
                let row = (((y & 15) << 4) | (z & 15)) as usize;
                for (i, kx) in (kx0..=kx1).enumerate() {
                    let s = at(kx, y >> 4, z >> 4);
                    if (s.large[row] | s.piston[row]) & padded[i] != 0 {
                        return None;
                    }
                }
            }
        }
    }
    // The scan keeps a cell of the box only if its block meets the box: the cells in the middle always do; the cells on the box's faces
    // do unless the box only grazes them (`intersects_corners`), which each axis decides once.
    let (lx_ok, hx_ok) = (lo[0] < f64::from(x0) + 1.0, hi[0] > f64::from(x1));
    let (ly_ok, hy_ok) = (lo[1] < f64::from(y0) + 1.0, hi[1] > f64::from(y1));
    let (lz_ok, hz_ok) = (lo[2] < f64::from(z0) + 1.0, hi[2] > f64::from(z1));
    for (i, kx) in (kx0..=kx1).enumerate() {
        // The cells of x the box keeps: those of the column range, less its two faces' when they are only grazed.
        let mut keep = inner[i];
        if !lx_ok {
            keep &= !span(kx, x0, x0);
        }
        if !hx_ok {
            keep &= !span(kx, x1, x1);
        }
        inner[i] = keep;
    }
    let mut n = 0;
    for z in z0..=z1 {
        if !((z > z0 || lz_ok) && (z < z1 || hz_ok)) {
            continue;
        }
        for y in y0..=y1 {
            if !((y > y0 || ly_ok) && (y < y1 || hy_ok)) {
                continue;
            }
            let row = (((y & 15) << 4) | (z & 15)) as usize;
            for (i, kx) in (kx0..=kx1).enumerate() {
                let s = at(kx, y >> 4, z >> 4);
                let mut bits = s.full[row] & inner[i];
                while bits != 0 {
                    let bit = bits.trailing_zeros() as i32;
                    bits &= bits - 1;
                    if n == MAX_CUBES {
                        return None;
                    }
                    out[n] = [f64::from((kx << 4) + bit), f64::from(y), f64::from(z)];
                    n += 1;
                }
            }
        }
    }
    Some(n)
}

/// A section the thread's cache does not hold: from the map under the lock (built on its first read), then cached.
#[cold]
#[inline(never)]
fn fetch(recent: &mut [(u64, *const Section); SLOTS], occ: &Occupancy, world: &World, binding: &Binding<'_>, key: u64, k: [i32; 3], slot: usize) -> *const Section {
    let p = occ.section(world, binding, key, k);
    recent[slot] = (key, p);
    p
}
