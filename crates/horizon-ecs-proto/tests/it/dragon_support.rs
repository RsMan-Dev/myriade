//! What the dragon tests against Horizon's `Dragon` share: the golden files (`tests/it/dragon_golden/`, written by the test-only
//! generator of `horizon-client`, `game/mobs/dragon/golden_tests.rs`), a dragon on flat ground at the golden's place, and the
//! comparison line by line (`HORIZON_DRAGON_DUMP=<dir>` writes our side to `<dir>/<name>.txt` to diff it).
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;
use horizon_core::random::LegacyRandom;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::dragon::flight::{DragonRng, FlightHistory};
use horizon_ecs_proto::dragon::hurt::{self, Blow, DragonHurt, Hurt};
use horizon_ecs_proto::dragon::nodes::{FlightNodes, LAYOUT};
use horizon_ecs_proto::dragon::perch::SittingDamage;
use horizon_ecs_proto::dragon::phases::{self, PhaseId};
use horizon_ecs_proto::goals::{Online, Player};
use horizon_ecs_proto::targeting::Who;

pub const GROUND: i32 = 64;
pub const PLAYER: (f64, f64, f64) = (10.5, 64.0, 0.5);
pub const PLAYER_ID: u32 = 77;

pub fn bits(v: Vec3) -> String {
    format!("{:016x} {:016x} {:016x}", v.x.to_bits(), v.y.to_bits(), v.z.to_bits())
}

pub fn compare(name: &str, ours: Vec<String>) {
    if let Some(dir) = std::env::var_os("HORIZON_DRAGON_DUMP") {
        std::fs::write(std::path::Path::new(&dir).join(format!("{name}.txt")), ours.join("\n")).unwrap();
    }
    let path = format!("{}/tests/it/dragon_golden/{name}.txt", env!("CARGO_MANIFEST_DIR"));
    let golden = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("golden {path} missing (the generator writes it): {e}"));
    let want: Vec<&str> = golden.trim_end().lines().collect();
    for (i, (a, b)) in ours.iter().zip(&want).enumerate() {
        assert_eq!(a, b, "{name} line {i}: the ECS side (left) differs from Horizon's Dragon (right)");
    }
    assert_eq!(ours.len(), want.len(), "{name}: line count");
}

/// A world of flat ground at 64 with the scenarios' player.
pub fn herd() -> Herd {
    herd_of(40)
}

/// The same world with a slab of `half` blocks each way (the gateway stands 96 blocks out).
pub fn herd_of(half: i32) -> Herd {
    let mut h = Herd::with_slab(half, false, false);
    h.world.insert_resource(FlightNodes::build(&LAYOUT, |_, _| GROUND));
    lay_ground(&mut h);
    h.world.resource_mut::<Online>().set(Player { id: PLAYER_ID, x: PLAYER.0, y: PLAYER.1, z: PLAYER.2, ..Default::default() });
    h
}

/// The generator's ground: a stone floor at GROUND - 1 (the slab's own surface is at y = 0), laid at the first step, with the
/// podium's block in bedrock as in the End (the dragon does not break it).
pub fn lay_ground(h: &mut Herd) {
    for x in -90..=90 {
        for z in -90..=90 {
            horizon_ecs_proto::blocks::set_state(&mut h.world, [x, GROUND - 1, z], "minecraft:stone", &[]);
        }
    }
    horizon_ecs_proto::blocks::set_state(&mut h.world, [0, GROUND - 1, 0], "minecraft:bedrock", &[]);
}

/// The dragon at (0.5, 80, 0.5), yaw 0, the golden's seed, in `phase` (its `begin` run) with `health`.
pub fn dragon(h: &mut Herd, phase: u8, health: f32) -> Entity {
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == "minecraft:ender_dragon").expect("the dragon's species");
    let e = spawn_armed(&mut h.world, &sp, 0.5, 0.5, 7);
    h.world.entity_mut(e).insert((Pos(Vec3::new(0.5, 80.0, 0.5)), Yaw(0.0), DragonRng(LegacyRandom::new(5), LegacyRandom::new(9)), FlightHistory::new(80.0, 0.0), Health(health), horizon_ecs_proto::dragon::parts::Parts::default()));
    phases::set_phase(&mut h.world, e, phase);
    e
}

/// The dragon's own state after a step, as the generator's `snap`.
pub fn snap(h: &Herd, e: Entity) -> String {
    let c = h.world.get::<DragonHurt>(e).copied().unwrap_or_default();
    format!(
        "h{:08x} p{} t{} w{} l{:08x} s{:08x} v{}",
        h.world.get::<Health>(e).unwrap().0.to_bits(),
        h.world.get::<PhaseId>(e).unwrap().0,
        c.time,
        c.window,
        c.last.to_bits(),
        h.world.get::<SittingDamage>(e).map_or(0.0, |s| s.0).to_bits(),
        bits(h.world.get::<Vel>(e).unwrap().0)
    )
}

pub fn show(r: Hurt) -> &'static str {
    match r {
        Hurt::Landed { full: true } => "full",
        Hurt::Landed { full: false } => "part",
        Hurt::Refused => "refused",
        Hurt::Shrugged => "shrugged",
    }
}

/// One blow, as the generator's `blow`: `K` player attack, `A` wind charge, `F` fire, `M` mob (not counted), `E` explosion.
pub fn blow(h: &mut Herd, e: Entity, part: usize, damage: f32, k: char) -> Hurt {
    let by = Some(Who::Player(PLAYER_ID));
    let base = Blow { part, damage, cause: Cause::Entity, by, arrow: false, from: Some((PLAYER.0, PLAYER.2)) };
    let b = match k {
        'K' => base,
        'A' => Blow { arrow: true, from: None, ..base },
        'F' => Blow { cause: Cause::InFire, by: None, from: None, ..base },
        'M' => Blow { by: None, from: None, ..base },
        _ => Blow { cause: Cause::Blast, by: None, from: None, ..base },
    };
    hurt::hurt(&mut h.world, e, b)
}

/// The blocks that the death and the respawn build, as the generator's `blocks`: every block that is not air in the boxes around the
/// podium and the gateway, by position and name.
pub fn blocks(h: &Herd, gateway: [i32; 3]) -> Vec<String> {
    use horizon_ecs_proto::blocks::{state_id, world_of};
    let mw = world_of(&h.world);
    let mut out = Vec::new();
    let boxes = [([-5, 55, -5], [5, 95, 5]), ([gateway[0] - 2, 72, gateway[2] - 2], [gateway[0] + 2, 78, gateway[2] + 2])];
    for (lo, hi) in boxes {
        for y in lo[1]..=hi[1] {
            for z in lo[2]..=hi[2] {
                for x in lo[0]..=hi[0] {
                    let name = mw.registry.state(state_id(mw, [x, y, z])).map_or("?", |s| s.name.as_str());
                    if name != "minecraft:air" && !(name == "minecraft:stone" && y <= GROUND) && !(name == "minecraft:bedrock" && [x, y, z] == [0, GROUND - 1, 0]) {
                        out.push(format!("blk {x} {y} {z} {name}"));
                    }
                }
            }
        }
    }
    out
}
