
use super::*;
/// The two-level `for_radius` visits the same entities in the same order as a loop over every column of the box.
#[test]
fn for_radius_matches_column_loop() {
    let mut world = World::new();
    let width = 70;
    let mut idx = Index::square(96);
    let mut s = 12345u64;
    let mut rnd = move |n: u64| {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        s % n
    };
    let mut placed = Vec::new();
    for _ in 0..400 {
        let e = world.spawn_empty().id();
        let c = rnd((width * width) as u64) as i32;
        let cell = idx.cell_of(f64::from(c % width), f64::from(c / width));
        idx.insert(e, cell);
        placed.push((e, cell));
    }
    for k in 0..60 {
        let (e, cell) = placed.swap_remove(rnd(placed.len() as u64) as usize);
        idx.remove(e, cell);
        if k % 2 == 0 {
            let c2 = rnd((width * width) as u64) as i32;
            let cell2 = idx.cell_of(f64::from(c2 % width), f64::from(c2 / width));
            idx.insert(e, cell2);
            placed.push((e, cell2));
        }
    }
    for _ in 0..300 {
        let (x, z, r) = (rnd(90) as f64 - 10.0, rnd(90) as f64 - 10.0, rnd(40) as f64);
        let mut got = Vec::new();
        idx.for_radius(x, z, r, |e| got.push(e));
        let mut want = Vec::new();
        for zz in (z - r - 1.0).floor() as i32..=(z + r + 1.0).floor() as i32 {
            for xx in (x - r - 1.0).floor() as i32..=(x + r + 1.0).floor() as i32 {
                let cell = idx.cell_of(f64::from(xx), f64::from(zz));
                if cell != u32::MAX {
                    want.extend_from_slice(idx.list(cell));
                }
            }
        }
        assert_eq!(got, want, "x {x} z {z} r {r}");
    }
}
