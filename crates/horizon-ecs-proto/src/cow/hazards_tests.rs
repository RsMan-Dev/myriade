
use super::*;
use std::collections::BTreeMap;

const KINDS: [u8; 6] = [0, HAZ_LAVA, HAZ_FIRE, HAZ_WATER, HAZ_SNOW, HAZ_CACTUS];

/// The hazard blocks as a plain map: a block's bits, absent when it has none.
type Model = BTreeMap<[i32; 3], u8>;

/// The chunks that hold the blocks of `origin..origin + width`, and the cells one step around them (`set` drops the others).
fn loaded(origin: i32, width: usize) -> Chunks {
    let mut chunks = Chunks::default();
    let (lo, hi) = ((origin - 2) >> 4, (origin + width as i32 + 1) >> 4);
    for cz in lo..=hi {
        for cx in lo..=hi {
            chunks.create(cx, cz);
        }
    }
    chunks
}

/// `set` as a map: the block at (x, y, z) now has these bits (0: none).
fn set_ref(m: &mut Model, x: i32, y: i32, z: i32, bit: u8) {
    if bit == 0 {
        m.remove(&[x, y, z]);
    } else {
        m.insert([x, y, z], bit);
    }
}

/// `of_box` as a scan of the map over the columns and heights of the box's key.
fn bits_ref(m: &Model, h: &Hazards, b: &Aabb) -> u8 {
    let [xa, xb, za, zb, y0, y1, x0, x1] = h.key_of(b);
    let mut bits = 0;
    for (&[x, y, z], &bit) in m {
        let fluid = bit & (HAZ_WATER | HAZ_LAVA) != 0;
        let inside = (xa..=xb).contains(&x) && (za..=zb).contains(&z);
        if inside && ((fluid && (y0..=y1).contains(&y)) || (!fluid && (x0..=x1).contains(&y))) {
            bits |= bit;
        }
    }
    bits
}

#[inline(never)]
fn bench_hazards_new(h: &Hazards, chunks: &Chunks, boxes: &[Aabb]) -> u32 {
    boxes.iter().map(|b| u32::from(h.of_box(chunks, b))).sum()
}
#[inline(never)]
fn bench_hazards_ref(m: &Model, h: &Hazards, boxes: &[Aabb]) -> u32 {
    boxes.iter().map(|b| u32::from(bits_ref(m, h, b))).sum()
}
/// The sparse world of a ghast arena: callgrind with `--toggle-collect=*bench_hazards_new*` then `*bench_hazards_ref*`.
#[test]
#[ignore = "bench"]
fn hazards_bench() {
    let (origin, width) = (-100, 201);
    let mut rng = Rng(0x1234_5678_9ABC_DEF1);
    let (mut h, mut chunks, mut m) = (Hazards::default(), loaded(origin, width), Model::new());
    for _ in 0..60 {
        let (x, z) = (origin + rng.next_int(width as i32), origin + rng.next_int(width as i32));
        let (y, bit) = (rng.next_int(4), KINDS[1 + rng.next_int(5) as usize]);
        h.set(&mut chunks, x, y, z, bit);
        set_ref(&mut m, x, y, z, bit);
    }
    let boxes: Vec<Aabb> = (0..20_000)
        .map(|_| box_at(Vec3::new(f64::from(origin) + rng.next_f64() * 200.0, rng.next_f64() * 8.0, f64::from(origin) + rng.next_f64() * 200.0), 4.0, 4.0))
        .collect();
    assert_eq!(bench_hazards_new(&h, &chunks, &boxes), bench_hazards_ref(&m, &h, &boxes));
}

#[test]
fn hazards_index_equals_reference() {
    for width in [16usize, 17, 33, 50, 70] {
        let origin = -(width as i32) / 2;
        for seq in 0..600u64 {
            let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ ((width as u64) << 32) ^ seq);
            let (mut h, mut chunks, mut r) = (Hazards::default(), loaded(origin, width), Model::new());
            for _ in 0..rng.next_int(301) {
                let x = origin - 2 + rng.next_int(width as i32 + 4);
                let z = origin - 2 + rng.next_int(width as i32 + 4);
                let y = -3 + rng.next_int(14);
                let bit = KINDS[rng.next_int(KINDS.len() as i32) as usize];
                h.set(&mut chunks, x, y, z, bit);
                set_ref(&mut r, x, y, z, bit);
            }
            for _ in 0..100 {
                let p = Vec3::new(
                    (origin - 3) as f64 + rng.next_f64() * (width as f64 + 6.0),
                    -4.0 + rng.next_f64() * 16.0,
                    (origin - 3) as f64 + rng.next_f64() * (width as f64 + 6.0),
                );
                let (bw, bh) = (0.1 + rng.next_int(59) as f32 / 10.0, 0.1 + rng.next_int(59) as f32 / 10.0);
                let b = box_at(p, bw, bh);
                assert_eq!(h.of_box(&chunks, &b), bits_ref(&r, &h, &b));
            }
        }
    }
}

#[test]
fn of_box_kept_matches_reference() {
    for seq in 0..200u64 {
        let width = 17 + seq as usize % 40;
        let origin = -(width as i32) / 2;
        let mut rng = Rng(0xD1B5_4A32_D192_ED03 ^ seq);
        let (mut h, mut chunks) = (Hazards::default(), loaded(origin, width));
        let b = box_at(Vec3::new(0.5, 1.0, 0.5), 1.2, 2.0);
        let mut cell = Cell(u32::MAX, [i32::MIN; 8], 0);
        let sent = 0xE0u8;
        h.of_box_kept(&chunks, &b, &mut cell, 0);
        for _ in 0..60 {
            let v0 = h.version;
            for _ in 0..rng.next_int(4) {
                let x = rng.next_int(5) - 2;
                let z = rng.next_int(5) - 2;
                let y = -1 + rng.next_int(5);
                h.set(&mut chunks, x, y, z, KINDS[rng.next_int(KINDS.len() as i32) as usize]);
            }
            let changed = h.version != v0;
            let got = h.of_box_kept(&chunks, &b, &mut cell, sent);
            if changed {
                assert_eq!(got, h.of_box(&chunks, &b));
            } else {
                assert_eq!(got, sent);
            }
        }
    }
}
