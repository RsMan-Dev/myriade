//! The dragon's part boxes against Horizon's (`tests/it/dragon_golden/L1.txt`, one case per line):
//! `index px py pz  x0 y0 z0 x1 y1 z1`, the part's position as Horizon placed it and its box as Horizon boxed it.
use horizon_ecs_proto::dragon::parts::{part_box, PartState, Parts};

const GOLDEN: &str = include_str!("dragon_golden/L1.txt");
const EPS: f64 = 1e-6;

fn num(s: &str) -> f64 {
    s.parse().expect("golden number")
}

#[test]
fn part_boxes_match_horizon() {
    let mut cases = 0;
    for line in GOLDEN.lines().filter(|l| !l.trim().is_empty() && !l.starts_with('#')) {
        let f: Vec<&str> = line.split_whitespace().collect();
        assert_eq!(f.len(), 10, "golden line has 10 fields: {line}");
        let i: usize = f[0].parse().expect("part index");
        let mut parts = Parts::default();
        parts.0[i] = PartState { pos: [num(f[1]), num(f[2]), num(f[3])], prev: [0.0; 3] };
        let got = part_box(&parts, i);
        let want = [num(f[4]), num(f[5]), num(f[6]), num(f[7]), num(f[8]), num(f[9])];
        let (lo, hi) = (got.min_position(), got.max_position());
        let have = [lo.x, lo.y, lo.z, hi.x, hi.y, hi.z];
        for (h, w) in have.iter().zip(want) {
            assert!((h - w).abs() < EPS, "part {i}: box {have:?} vs Horizon {want:?} in {line}");
        }
        cases += 1;
    }
    assert!(cases > 0, "the golden file holds no case");
}
