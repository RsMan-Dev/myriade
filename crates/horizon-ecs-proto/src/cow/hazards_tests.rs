
use super::*;

const KINDS: [u8; 6] = [0, HAZ_LAVA, HAZ_FIRE, HAZ_WATER, HAZ_SNOW, HAZ_CACTUS];

fn empty(origin: i32, width: usize) -> Hazards {
    Hazards {
        origin,
        width,
        cols: vec![0; width * width],
        pool: Vec::new(),
        eggs: Default::default(),
        version: 0,
        block_count: vec![0; width.div_ceil(16).pow(2)],
    }
}

/// `set` as before the counts and the change test: it rewrites the column and reads nothing else.
fn set_ref(h: &mut Hazards, x: i32, y: i32, z: i32, bit: u8) {
    let (cx, cz) = (x - h.origin, z - h.origin);
    if cx < 0 || cz < 0 || cx as usize >= h.width || cz as usize >= h.width {
        return;
    }
    let i = cz as usize * h.width + cx as usize;
    if h.cols[i] == 0 {
        if bit == 0 {
            return;
        }
        h.pool.push(Vec::new());
        h.cols[i] = h.pool.len() as u32;
    }
    let list = &mut h.pool[h.cols[i] as usize - 1];
    list.retain(|e| e.0 != y);
    if bit != 0 {
        list.push((y, bit));
    }
}

/// `of_box` as before the block skip: every column of the box is read.
fn bits_ref(h: &Hazards, b: &Aabb) -> u8 {
    let [xa, xb, za, zb, y0, y1, x0, x1] = h.key_of(b);
    let mut bits = 0;
    for cx in xa..=xb {
        for cz in za..=zb {
            let c = h.cols[cz as usize * h.width + cx as usize];
            if c != 0 {
                for &(y, bit) in &h.pool[c as usize - 1] {
                    let fluid = bit & (HAZ_WATER | HAZ_LAVA) != 0;
                    if (fluid && (y0..=y1).contains(&y)) || (!fluid && (x0..=x1).contains(&y)) {
                        bits |= bit;
                    }
                }
            }
        }
    }
    bits
}

/// Each block's count is the number of entries its columns hold.
fn counts_ok(h: &Hazards) -> bool {
    let bw = h.width.div_ceil(16);
    let mut want = vec![0u32; h.block_count.len()];
    for cz in 0..h.width {
        for cx in 0..h.width {
            let c = h.cols[cz * h.width + cx];
            if c != 0 {
                want[(cz >> 4) * bw + (cx >> 4)] += h.pool[c as usize - 1].len() as u32;
            }
        }
    }
    want == h.block_count
}

#[inline(never)]
fn bench_hazards_new(h: &Hazards, boxes: &[Aabb]) -> u32 {
    boxes.iter().map(|b| u32::from(h.of_box(b))).sum()
}
#[inline(never)]
fn bench_hazards_ref(h: &Hazards, boxes: &[Aabb]) -> u32 {
    boxes.iter().map(|b| u32::from(bits_ref(h, b))).sum()
}
/// The sparse world of a ghast arena: callgrind with `--toggle-collect=*bench_hazards_new*` then `*bench_hazards_ref*`.
#[test]
#[ignore = "bench"]
fn hazards_bench() {
    let (origin, width) = (-100, 201);
    let mut rng = Rng(0x1234_5678_9ABC_DEF1);
    let mut h = empty(origin, width);
    for _ in 0..60 {
        let (x, z) = (origin + rng.next_int(width as i32), origin + rng.next_int(width as i32));
        h.set(x, rng.next_int(4), z, KINDS[1 + rng.next_int(5) as usize]);
    }
    let boxes: Vec<Aabb> = (0..20_000)
        .map(|_| box_at(Vec3::new(f64::from(origin) + rng.next_f64() * 200.0, rng.next_f64() * 8.0, f64::from(origin) + rng.next_f64() * 200.0), 4.0, 4.0))
        .collect();
    assert_eq!(bench_hazards_new(&h, &boxes), bench_hazards_ref(&h, &boxes));
}

#[test]
fn hazards_index_equals_reference() {
    for width in [16usize, 17, 33, 50, 70] {
        let origin = -(width as i32) / 2;
        for seq in 0..600u64 {
            let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ ((width as u64) << 32) ^ seq);
            let (mut h, mut r) = (empty(origin, width), empty(origin, width));
            for _ in 0..rng.next_int(301) {
                let x = origin - 2 + rng.next_int(width as i32 + 4);
                let z = origin - 2 + rng.next_int(width as i32 + 4);
                let y = -3 + rng.next_int(14);
                let bit = KINDS[rng.next_int(KINDS.len() as i32) as usize];
                h.set(x, y, z, bit);
                set_ref(&mut r, x, y, z, bit);
            }
            assert!(counts_ok(&h));
            for _ in 0..100 {
                let p = Vec3::new(
                    (origin - 3) as f64 + rng.next_f64() * (width as f64 + 6.0),
                    -4.0 + rng.next_f64() * 16.0,
                    (origin - 3) as f64 + rng.next_f64() * (width as f64 + 6.0),
                );
                let (bw, bh) = (0.1 + rng.next_int(59) as f32 / 10.0, 0.1 + rng.next_int(59) as f32 / 10.0);
                let b = box_at(p, bw, bh);
                assert_eq!(h.of_box(&b), bits_ref(&r, &b));
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
        let mut h = empty(origin, width);
        let b = box_at(Vec3::new(0.5, 1.0, 0.5), 1.2, 2.0);
        let mut cell = Cell(0, [i32::MIN; 8], 0);
        let sent = 0xE0u8;
        h.of_box_kept(&b, &mut cell, 0);
        for _ in 0..60 {
            let v0 = h.version;
            for _ in 0..rng.next_int(4) {
                let x = rng.next_int(5) - 2;
                let z = rng.next_int(5) - 2;
                let y = -1 + rng.next_int(5);
                h.set(x, y, z, KINDS[rng.next_int(KINDS.len() as i32) as usize]);
            }
            let changed = h.version != v0;
            let got = h.of_box_kept(&b, &mut cell, sent);
            if changed {
                assert_eq!(got, h.of_box(&b));
            } else {
                assert_eq!(got, sent);
            }
        }
    }
}
