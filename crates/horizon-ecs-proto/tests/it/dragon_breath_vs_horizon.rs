//! The dragon's breath against Horizon's `Dragon` (`game/mobs/dragon.rs`, `breath_puffs` and the flame cloud), with imposed draws
//! (`LegacyRandom` at a fixed seed). The golden lines come from the reconciler, which runs Horizon's side in a test of
//! `horizon-client`; this file reads them from `tests/it/dragon_golden/breath.txt` and compares them to the ECS side, to the bit.
//! Lines: `landing T K <at x y z> <vel x y z>`, `flame T K <at> <vel>` (the values as f64 bits in hex), `cloud AGE RADIUS_BITS HITS`
//! (HITS: the player ids, `-` for none), `gone AGE`.
use horizon_core::geometry::{Aabb, Vec3};
use horizon_core::random::LegacyRandom;
use horizon_ecs_proto::dragon::breath::{self, Step};
use horizon_ecs_proto::targeting::Who;

fn bits(v: [f64; 3]) -> String {
    format!("{:016x} {:016x} {:016x}", v[0].to_bits(), v[1].to_bits(), v[2].to_bits())
}

fn puff_lines(tag: &str, t: u32, puffs: Vec<breath::Puff>, out: &mut Vec<String>) {
    for (k, (at, v)) in puffs.into_iter().enumerate() {
        out.push(format!("{tag} {t} {k} {} {}", bits(at), bits(v)));
    }
}

/// The ECS side, line by line, in the golden's order.
fn ours() -> Vec<String> {
    let mut out = Vec::new();

    let (look, head, motion) = ([0.0, -0.6, 0.8], [10.0, 70.5, -3.0], [0.25, -0.05, 0.5]);
    let mut r = LegacyRandom::new(7);
    for t in 0..20 {
        puff_lines("landing", t, breath::landing_puffs(look, head, motion, &mut r), &mut out);
    }

    let (look, head) = ([0.6, -0.0, 0.8], [0.0, 65.5, 4.0]);
    let mut r = LegacyRandom::new(11);
    for t in 1..=12u32 {
        puff_lines("flame", t, breath::flame_puffs(t, look, head, &mut r), &mut out);
    }

    // A flame at 0,65,0 for its 200 ticks: player 1 stands in it, player 2 at 7 blocks, out of the radius 5 all along.
    let player = |id: u32, x: f64| {
        let who = Who::Player(id);
        let b = Aabb::new(Vec3::new(x - 0.3, 65.0, -0.3), Vec3::new(x + 0.3, 66.8, 0.3));
        (who, b, Vec3::new(x, 65.0, 0.0))
    };
    let cands = [player(1, 0.0), player(2, 7.0)];
    let mut c = breath::FLAME.cloud([0.0, 65.0, 0.0]);
    for _ in 1..=205 {
        match breath::step(&mut c, &cands) {
            Step::Gone => {
                out.push(format!("gone {}", c.age));
                break;
            }
            Step::Live(hit) => {
                let ids: Vec<String> = hit
                    .iter()
                    .map(|w| match w {
                        Who::Player(id) => id.to_string(),
                        Who::Entity(_) => "e".to_string(),
                    })
                    .collect();
                let ids = if ids.is_empty() { "-".to_string() } else { ids.join(",") };
                out.push(format!("cloud {} {:08x} {ids}", c.age, c.radius.to_bits()));
            }
        }
    }
    out
}

#[test]
fn breath_matches_horizon_to_the_bit() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/it/dragon_golden/breath.txt");
    let golden = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("golden {path} missing (the reconciler writes it): {e}"));
    let ours = ours().join("\n");
    assert_eq!(ours, golden.trim_end(), "breath lines differ from Horizon's Dragon");
}
