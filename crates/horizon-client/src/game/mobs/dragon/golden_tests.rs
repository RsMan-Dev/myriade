//! Writes the golden files of the ECS port (`horizon-ecs-proto/tests/it/dragon_golden/`) from the real `Dragon`.
//! Test-only: it changes no behaviour. Run with `HORIZON_DRAGON_GOLDEN=<dir>` (otherwise it only checks it can run).
use super::*;
use crate::game::*;
use horizon_client::chunk::Chunk;
use horizon_ecs_proto::bridge::select::Select;
use std::fmt::Write as _;

const GROUND: i32 = 64;
const SEED: i64 = 5;
const TICKS: usize = 300;
const PAIRS: [(usize, usize, i32); 6] = [(0, 5, 1), (12, 20, 1), (3, 15, 1), (18, 22, 1), (11, 0, 0), (13, 23, 0)];
const PROBES: [(f64, f64, f64, i32); 4] = [(0.5, 80.0, 0.5, 1), (58.0, 70.0, 6.0, 1), (-12.0, 66.0, 25.0, 0), (30.0, 90.0, -30.0, 1)];

fn flat_game(test: impl FnOnce(&mut Game<'_>)) {
    let root = PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").unwrap());
    let settings = Settings::default();
    let loaded = load_resources(
        horizon_assets::resource_source::ResourceRoot::directory(root.clone()),
        Vec::new(),
        None,
        &root,
        &settings,
        Instant::now(),
        execution_role::ExecutionRole::IntegratedServer,
        &Progress::new(),
    )
    .unwrap();
    let mut options = Options::parse(std::iter::empty()).unwrap().unwrap();
    options.resources = root.clone();
    options.execution_role = execution_role::ExecutionRole::IntegratedServer;
    let mut g = Game::new(
        &loaded.resources,
        loaded.pack,
        options,
        settings,
        Audio::default(),
    )
    .unwrap();
    g.options.dedicated = Some(Vec::new());
    // Normal, not the default Peaceful: on Peaceful the dragon sees no enemy and never strafes, as no fight is played.
    g.session.difficulty = 2;
    g.world = World::new(g.res.facts().air, 1);
    for x in -6..=6 {
        for z in -6..=6 {
            g.world.chunks.insert((x, z), Arc::new(Chunk::empty(x, z)));
        }
    }
    let stone = g.pack.assets.registry.default_state("minecraft:stone").unwrap();
    for x in -90..=90 {
        for z in -90..=90 {
            g.world.set(x, GROUND - 1, z, stone).unwrap();
        }
    }
    // The podium's own block is bedrock, as in the End: the dragon does not break it, so the heightmap at (0, 0) stays at GROUND.
    let bedrock = g.pack.assets.registry.default_state("minecraft:bedrock").unwrap();
    g.world.set(0, GROUND - 1, 0, bedrock).unwrap();
    test(&mut g);
}

fn bits(v: Vec3) -> String {
    format!("{:016x} {:016x} {:016x}", v.x.to_bits(), v.y.to_bits(), v.z.to_bits())
}

fn lines_l1() -> String {
    let mut out = String::from("# index px py pz x0 y0 z0 x1 y1 z1 (Dragon::part_box)\n");
    let mut d = Dragon::new(Vec3::ZERO, 0.0, 1);
    for (k, p) in [(0.0, 0.0, 0.0), (3.25, 70.5, -8.125), (-40.0, 100.0, 17.75)].into_iter().enumerate() {
        for i in 0..8 {
            d.parts[i].0 = Vec3::new(p.0 + i as f64 * 0.5, p.1 + k as f64, p.2 - i as f64 * 0.25);
            let (pos, b) = d.part_geometry(i);
            let (lo, hi) = (b.min_position(), b.max_position());
            let _ = writeln!(out, "{i} {:?} {:?} {:?} {:?} {:?} {:?} {:?} {:?} {:?}", pos.x, pos.y, pos.z, lo.x, lo.y, lo.z, hi.x, hi.y, hi.z);
        }
    }
    out
}

fn lines_flight(g: &mut Game<'_>) -> String {
    let mut out = String::new();
    g.dragon.replace(Dragon::new(Vec3::new(0.5, 80.0, 0.5), 0.0, SEED));
    for (i, n) in g.dragon_nodes().iter().enumerate() {
        let _ = writeln!(out, "node {i} {} {} {}", n[0], n[1], n[2]);
    }
    for (a, b, c) in PAIRS {
        g.fight.alive_crystals = c;
        let line = match g.dragon_path(a, b, None) {
            Some(p) => p.nodes.iter().map(|n| format!("{}/{}/{}", n[0], n[1], n[2])).collect::<Vec<_>>().join(";"),
            None => "none".into(),
        };
        let _ = writeln!(out, "path {a} {b} {c} {line}");
    }
    for (x, y, z, c) in PROBES {
        g.fight.alive_crystals = c;
        let _ = writeln!(out, "closest {c} {x} {y} {z} {}", g.closest_node(x, y, z));
    }
    g.fight.alive_crystals = 0;
    let mut d = Dragon::new(Vec3::new(0.5, 80.0, 0.5), 0.0, SEED);
    d.phase = Phase::Holding { path: None, target: None, clockwise: false };
    d.id = 900;
    g.dragon.replace(d);
    for t in 0..TICKS {
        g.tick_dragon();
        let d = g.dragon.as_ref().unwrap();
        let _ = writeln!(out, "tick {t} {} {:08x} {}", bits(d.position), d.yaw.to_bits(), d.phase.id());
    }
    out
}

/// The player of the scenarios (Horizon's host player; the ECS side has the same one at the same feet).
const PLAYER: (f64, f64, f64) = (10.5, 64.0, 0.5);

fn put_player(g: &mut Game<'_>) {
    g.player = Some(PlayerMotion::new(g.motion, Vec3::new(PLAYER.0, PLAYER.1, PLAYER.2)).unwrap());
}

fn fresh(g: &mut Game<'_>, phase: i32, health: f32) {
    let mut d = Dragon::new(Vec3::new(0.5, 80.0, 0.5), 0.0, SEED);
    d.phase = Phase::from_id(phase);
    d.health = health;
    d.id = 900;
    g.dragon.replace(d);
}

/// The dragon's own state after a step: health, phase, the three hurt clocks, the perch damage, velocity.
fn snap(g: &Game<'_>) -> String {
    let d = g.dragon.as_ref().unwrap();
    format!(
        "h{:08x} p{} t{} w{} l{:08x} s{:08x} v{}",
        d.health.to_bits(),
        d.phase.id(),
        d.hurt_time,
        d.invulnerable,
        d.last_hurt.to_bits(),
        d.sitting_damage.to_bits(),
        bits(d.velocity)
    )
}

fn show(h: Hurt) -> &'static str {
    match h {
        Hurt::Landed { full: true } => "full",
        Hurt::Landed { full: false } => "part",
        Hurt::Refused => "refused",
        Hurt::Shrugged => "shrugged",
    }
}

/// One blow on part `part`: `K` player attack, `A` wind charge, `F` fire, `M` mob (not counted), `E` explosion.
fn blow(g: &mut Game<'_>, part: usize, dmg: f32, k: char) -> Hurt {
    let host = Some(FightWho::Host);
    match k {
        'K' => g.hurt_dragon_part(part, dmg, true, host, DamageKind::PlayerAttack, None),
        'A' => {
            let origin = DragonHitOrigin { direct: Some(&ProjectileKind::WindCharge), entity_id: None, from: None, invulnerability_checked: false };
            g.hurt_dragon_part_at(part, dmg, true, host, DamageKind::PlayerAttack, origin)
        }
        'F' => g.hurt_dragon_part(part, dmg, false, None, DamageKind::InFire, None),
        'M' => g.hurt_dragon_part(part, dmg, false, None, DamageKind::MobAttack, None),
        _ => g.hurt_dragon_part(part, dmg, true, None, DamageKind::Explosion, None),
    }
}

/// The hurt table: every phase takes the same blows; `part dmg kind` then the state.
const BLOWS: [(usize, f32, char); 8] = [(0, 5.0, 'K'), (3, 9.0, 'K'), (0, 12.0, 'K'), (0, 5.0, 'A'), (0, 0.001, 'K'), (0, 4.0, 'F'), (0, 4.0, 'M'), (2, 30.0, 'E')];

fn lines_hurt(g: &mut Game<'_>) -> String {
    let mut out = String::new();
    put_player(g);
    for phase in 0..=10 {
        fresh(g, phase, 200.0);
        for (i, (part, dmg, k)) in BLOWS.into_iter().enumerate() {
            let r = blow(g, part, dmg, k);
            let _ = writeln!(out, "hurt {phase} {i} {} {}", show(r), snap(g));
        }
        // The perch: 40 then 20 with the window cleared in between (more than a quarter of 200 takes the dragon off).
        fresh(g, phase, 200.0);
        for (i, dmg) in [40.0_f32, 20.0].into_iter().enumerate() {
            let r = blow(g, 0, dmg, 'K');
            let _ = writeln!(out, "perch {phase} {i} {} {}", show(r), snap(g));
            let d = g.dragon.as_mut().unwrap();
            d.invulnerable = 0;
            d.hurt_time = 0;
        }
        // The killing blow: sitting phases fall to 0, the others fly their dying flight at health 1.
        fresh(g, phase, 3.0);
        let r = blow(g, 0, 9.0, 'K');
        let _ = writeln!(out, "kill {phase} 0 {} {}", show(r), snap(g));
    }
    // The clocks run down with the ticks: a blow, twelve ticks (the window is 20, `hurt_time` 10), the same blow again.
    fresh(g, 0, 200.0);
    g.dragon.as_mut().unwrap().phase = Phase::Holding { path: None, target: None, clockwise: false };
    let r = blow(g, 0, 5.0, 'K');
    let _ = writeln!(out, "clock 0 {} {}", show(r), snap(g));
    for t in 1..=24 {
        g.tick_dragon();
        let _ = writeln!(out, "clock {t} - {}", snap(g));
        if t == 12 || t == 24 {
            let r = blow(g, 0, 5.0, 'K');
            let _ = writeln!(out, "clock {t} {} {}", show(r), snap(g));
        }
    }
    out
}

/// What the game ticks each tick for the fight (`tick_end` and the projectile pass): the dragon, the breath clouds, the fireballs.
fn world_tick(g: &mut Game<'_>) {
    // ServerPlayer.tick: the player's invulnerability window counts down (else only its first hit would land).
    if let Some(p) = g.player.as_mut() {
        p.health.advance_clock();
    }
    g.tick_dragon();
    g.tick_ecs().unwrap();
}

fn fireball_select() -> Select {
    Select { kind: Some("minecraft:dragon_fireball".into()), ..Default::default() }
}

/// The positions of the dragon's fireballs in the ECS level.
fn fireball_positions(g: &Game<'_>) -> Vec<Vec3> {
    let Some(world) = g.ecs.world() else {
        return Vec::new();
    };
    g.ecs
        .select(&fireball_select())
        .into_iter()
        .filter_map(|e| horizon_ecs_proto::bridge::view::view(world, e))
        .map(|v| Vec3::new(v.position[0], v.position[1], v.position[2]))
        .collect()
}

/// A tick's state: the dragon, the fireballs in flight, the breath clouds. A fireball's cloud is placed half a block under the
/// fireball by Horizon (`fireball_cloud_origin(next.y - 0.5)`): its height is left out here (logged in `ecs-ecarts.md`).
fn frame(g: &Game<'_>, name: &str, t: usize) -> String {
    let d = g.dragon.as_ref().unwrap();
    let mut fb: Vec<String> = fireball_positions(g).into_iter().map(bits).collect();
    fb.sort();
    let mut cl: Vec<String> = g
        .breath_clouds()
        .iter()
        .map(|c| format!("{:016x}/{:016x}/{:016x}/{:016x}/{:08x}", c.at[0].to_bits(), c.at[1].to_bits(), c.at[2].to_bits(), c.radius.to_bits(), c.age))
        .collect();
    cl.sort();
    format!(
        "{name} {t} {} {:08x} p{} h{:08x} fb[{}] cl[{}]",
        bits(d.position),
        d.yaw.to_bits(),
        d.phase.id(),
        d.health.to_bits(),
        fb.join(";"),
        cl.join(";")
    )
}

/// The three attack scenarios, each from its phase with the player near: a strafe (the fireball and its cloud), a charge, and the
/// whole perch (landing, scanning, the attack, four flames, the takeoff).
fn lines_combat(g: &mut Game<'_>) -> (String, String) {
    let mut out = String::new();
    let mut rng = String::new();
    let fp: Vec<String> = g.fight_players().iter().map(|p| format!("{:?}/{}/{}", p.who, p.enemy(), bits(p.position))).collect();
    let _ = writeln!(rng, "diag difficulty {} players {}", g.session.difficulty, fp.join(";"));
    // An earlier scenario of this same game may have broken the ground under the podium: the combat starts on a whole slab.
    let stone = g.pack.assets.registry.default_state("minecraft:stone").unwrap();
    for x in -90..=90 {
        for z in -90..=90 {
            g.world.set(x, GROUND - 1, z, stone).unwrap();
        }
    }
    // The podium's own block is bedrock, as in the End: the dragon does not break it, so the heightmap at (0, 0) stays at GROUND.
    let bedrock = g.pack.assets.registry.default_state("minecraft:bedrock").unwrap();
    g.world.set(0, GROUND - 1, 0, bedrock).unwrap();
    put_player(g);
    let me = Vec3::new(PLAYER.0, PLAYER.1, PLAYER.2);
    for (name, ticks) in [("strafe", 700), ("charge", 80), ("perch", 1900)] {
        let phase = match name {
            "strafe" => 1,
            "charge" => 8,
            _ => 2,
        };
        put_player(g);
        fresh(g, phase, 200.0);
        let d = g.dragon.as_mut().unwrap();
        match name {
            "strafe" => d.phase = Phase::Strafe { charge: 0, path: None, target: None, clockwise: false, player: Some(FightWho::Host) },
            "charge" => d.phase = Phase::Charging { target: Some(me), since: 0 },
            _ => {}
        }
        if name == "strafe" {
            g.strafe_begin(me);
        }
        for t in 0..ticks {
            world_tick(g);
            let _ = writeln!(out, "{}", frame(g, name, t));
            let _ = writeln!(rng, "{name} {t} {:012x} ph{} tg {} pa {} ac{}", g.dragon.as_ref().map_or(0, |d| d.random.state()), g.player.as_ref().map_or(0.0, |p| p.health.health()), g.dragon.as_ref().and_then(|d| d.phase.target()).map_or("-".to_string(), bits), match g.dragon.as_ref().map(|d| &d.phase) { Some(Phase::LandingApproach { path: Some(p), .. }) => format!("{:?}", p.0.nodes), _ => String::new() }, g.fight.alive_crystals);
        }
        for e in g.ecs.select(&fireball_select()) {
            g.ecs.despawn(e);
        }
        g.clear_breath_clouds();
    }
    (out, rng)
}

fn orb_total(g: &Game<'_>) -> i64 {
    g.ecs.orb_views().iter().map(|o| i64::from(o.value) * i64::from(o.count)).sum()
}

/// The blocks around the podium and the gateway that are not air or the ground (the ECS test reads the same boxes).
fn lines_blocks(g: &Game<'_>, gateway: [i32; 3]) -> Vec<String> {
    let boxes = [([-5, 55, -5], [5, 95, 5]), ([gateway[0] - 2, 72, gateway[2] - 2], [gateway[0] + 2, 78, gateway[2] + 2])];
    let mut out = Vec::new();
    for (lo, hi) in boxes {
        for y in lo[1]..=hi[1] {
            for z in lo[2]..=hi[2] {
                for x in lo[0]..=hi[0] {
                    let name = &g.res.facts().get(g.world.block(x, y, z)).name;
                    if &**name != "minecraft:air" && !(&**name == "minecraft:stone" && y <= GROUND) && !(&**name == "minecraft:bedrock" && (x, y, z) == (0, GROUND - 1, 0)) {
                        out.push(format!("blk {x} {y} {z} {name}"));
                    }
                }
            }
        }
    }
    out
}

/// A dragon at health 0 dies for 200 ticks (the value of the orbs per tick), then the fight's end: portal, gateway, egg, flags.
fn lines_death(g: &mut Game<'_>) -> String {
    put_player(g);
    fresh(g, 10, 0.0);
    g.fight.gateways = vec![3, 7];
    let mut out = String::new();
    for t in 1..=200 {
        g.tick_dragon();
        match g.dragon.as_ref() {
            Some(d) => writeln!(out, "death {t} {} dt{} orbs{}", bits(d.position), d.death_time, orb_total(g)).unwrap(),
            None => writeln!(out, "death {t} gone orbs{}", orb_total(g)).unwrap(),
        }
    }
    g.dragon_killed();
    for l in lines_blocks(g, [(96.0 * (2.0 * (-std::f64::consts::PI + std::f64::consts::PI / 20.0 * 7.0)).cos()).floor() as i32, 75, (96.0 * (2.0 * (-std::f64::consts::PI + std::f64::consts::PI / 20.0 * 7.0)).sin()).floor() as i32]) {
        writeln!(out, "{l}").unwrap();
    }
    writeln!(out, "flags killed{} previously{} gateways{:?}", g.fight.killed, g.fight.previously_killed, g.fight.gateways).unwrap();
    out
}

/// The spikes' crystals, healing and destruction. Hover phase (no flight draws beyond the scan), a crystal on each spike: the nearest
/// one is found by the scan roll and heals one point every tenth tick; then the nearest one is destroyed (the blast's 10 points), then
/// another that is not the nearest.
fn lines_crystals(g: &mut Game<'_>) -> String {
    let mut out = String::new();
    put_player(g);
    let spikes = horizon_world::worldgen::end_spike::spikes_for_seed(g.level.seed);
    for (i, s) in spikes.iter().enumerate() {
        writeln!(out, "spike {i} {} {} {}", s.center_x, s.center_z, s.height).unwrap();
    }
    g.clear_crystals();
    for s in spikes.iter() {
        g.spawn_crystal(Vec3::new(f64::from(s.center_x) + 0.5, f64::from(s.height + 1), f64::from(s.center_z) + 0.5), true);
    }
    g.fight.alive_crystals = spikes.len() as i32;
    fresh(g, 10, 100.0);
    let near = |g: &Game<'_>| g.dragon.as_ref().unwrap().nearest_crystal.map_or("none".to_string(), |id| bits(g.crystal(id).unwrap().position));
    let line = |g: &Game<'_>, name: &str, t: usize| format!("{name} {t} {} alive{} near {}", snap(g), g.fight.alive_crystals, near(g));
    for t in 0..400 {
        g.tick_dragon();
        writeln!(out, "{}", line(g, "heal", t)).unwrap();
    }
    for (round, name) in ["nearest", "other"].into_iter().enumerate() {
        let n = g.dragon.as_ref().unwrap().nearest_crystal;
        let Some(i) = (if round == 0 { n } else { g.crystals().iter().map(|c| c.id).find(|&id| Some(id) != n) }) else {
            writeln!(out, "{name} none").unwrap();
            continue;
        };
        writeln!(out, "{name} {}", bits(g.crystal(i).unwrap().position)).unwrap();
        g.destroy_crystal(i, Some(FightWho::Host), CrystalDamage::Explosion);
        for t in 0..60 {
            g.tick_dragon();
            writeln!(out, "{}", line(g, name, t)).unwrap();
        }
    }
    out
}

/// The respawn: four crystals around the exit portal, the stages tick by tick (stage, its time, crystals standing, a dragon or not),
/// then the iron and obsidian of each spike and the blocks around the podium.
fn lines_respawn(g: &mut Game<'_>) -> String {
    let mut out = String::new();
    g.dragon.discard();
    g.clear_crystals();
    // The egg of the earlier death in this same game stays out of the respawn's blocks.
    let air = g.pack.assets.registry.default_state("minecraft:air").expect("air");
    let _ = g.level_set([0, 65, 0], air);
    g.load_fight();
    g.fight.killed = true;
    g.fight.exit_portal = Some([0, 61, 0]);
    for d in [[0, -1], [0, 1], [-1, 0], [1, 0]] {
        let at = Vec3::new(f64::from(d[0] * 3) + 0.5, 62.0, f64::from(d[1] * 3) + 0.5);
        g.spawn_crystal(at, true);
    }
    g.try_respawn();
    writeln!(out, "started {:?}", g.fight.respawn).unwrap();
    for t in 0..800 {
        g.tick_respawn();
        writeln!(out, "respawn {t} {:?} crystals{} dragon{}", g.fight.respawn, g.crystals().len(), g.dragon.is_some()).unwrap();
    }
    for (i, s) in horizon_world::worldgen::end_spike::spikes_for_seed(g.level.seed).iter().enumerate() {
        let (mut obsidian, mut bars) = (0, 0);
        for y in 0..=s.height + 10 {
            for z in s.center_z - s.radius - 1..=s.center_z + s.radius + 1 {
                for x in s.center_x - s.radius - 1..=s.center_x + s.radius + 1 {
                    match &*g.res.facts().get(g.world.block(x, y, z)).name {
                        "minecraft:obsidian" => obsidian += 1,
                        "minecraft:iron_bars" => bars += 1,
                        _ => {}
                    }
                }
            }
        }
        writeln!(out, "spike {i} obsidian{obsidian} bars{bars}").unwrap();
    }
    for l in lines_blocks(g, [0, 200, 0]) {
        writeln!(out, "{l}").unwrap();
    }
    out
}

#[test]
#[ignore = "daily"]
fn write_dragon_goldens() {
    flat_game(|g| {
        let (l1, flight) = (lines_l1(), lines_flight(g));
        let hurt = lines_hurt(g);
        let (combat, combat_rng) = lines_combat(g);
        let death = lines_death(g);
        if let Some(dir) = std::env::var_os("HORIZON_DRAGON_GOLDEN") {
            let dir = PathBuf::from(dir);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("L1.txt"), l1).unwrap();
            std::fs::write(dir.join("flight.txt"), flight).unwrap();
            std::fs::write(dir.join("hurt.txt"), hurt).unwrap();
            std::fs::write(dir.join("combat.txt"), combat).unwrap();
            std::fs::write(dir.join("combat_rng.txt"), combat_rng).unwrap();
            std::fs::write(dir.join("death.txt"), death).unwrap();
        }
        // The two that touch the most of the game: written last, each alone, so a panic in one leaves the others.
        let crystals = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| lines_crystals(g)));
        if let (Some(dir), Ok(t)) = (std::env::var_os("HORIZON_DRAGON_GOLDEN"), &crystals) {
            std::fs::write(PathBuf::from(dir).join("crystals.txt"), t).unwrap();
        }
        let respawn = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| lines_respawn(g)));
        if let (Some(dir), Ok(t)) = (std::env::var_os("HORIZON_DRAGON_GOLDEN"), &respawn) {
            std::fs::write(PathBuf::from(dir).join("respawn.txt"), t).unwrap();
        }
        assert!(crystals.is_ok() && respawn.is_ok(), "a generator panicked (see above)");
    });
}
