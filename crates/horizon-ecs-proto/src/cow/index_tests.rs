
use super::*;
/// The two-level `for_radius` visits the same entities in the same order as a loop over every column of the box.
#[test]
fn for_radius_matches_column_loop() {
    let mut world = World::new();
    let width = 70;
    let mut idx = Index::new(0.0, width);
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
        let c = (rnd((width * width) as u64)) as u32;
        idx.insert(e, c);
        placed.push((e, c));
    }
    for k in 0..60 {
        let (e, c) = placed.swap_remove(rnd(placed.len() as u64) as usize);
        idx.remove(e, c);
        if k % 2 == 0 {
            let c2 = rnd((width * width) as u64) as u32;
            idx.insert(e, c2);
            placed.push((e, c2));
        }
    }
    for _ in 0..300 {
        let (x, z, r) = (rnd(90) as f64 - 10.0, rnd(90) as f64 - 10.0, rnd(40) as f64);
        let mut got = Vec::new();
        idx.for_radius(x, z, r, |e| got.push(e));
        let w = width as i64;
        let mut want = Vec::new();
        for zz in idx.column(z - r - 1.0).max(0)..=idx.column(z + r + 1.0).min(w - 1) {
            for xx in idx.column(x - r - 1.0).max(0)..=idx.column(x + r + 1.0).min(w - 1) {
                want.extend_from_slice(&idx.cells[(zz * w + xx) as usize]);
            }
        }
        assert_eq!(got, want, "x {x} z {z} r {r}");
    }
}
