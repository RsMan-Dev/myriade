//! The Myriade behaviour probe: the server's counterpart of `tools/vanilla-probe`. A server `Game` hosts eight parcels on 127.0.0.1:25582; mobs
//! are born by `/summon`, the scene is made by commands, the player is a protocol bot that joins like a client, the ticks are the server's.
//! Run: `MYRIADE_BEHAVIOUR=out.tsv HORIZON_TEST_RESOURCES=<prepared> cargo test -p horizon-client --lib behaviour_probe -- --ignored --nocapture`.
//! `PROBE_SPECIES` filters the species (comma list), `PROBE_TICKS` and `PROBE_TIME` as the Java probe. Errors go to `<out>.log`.
use super::behaviour_measures::*;
use super::test_support::active_server_game;
use crate::game::Game;
use horizon_client::chunk::Chunk;
use horizon_client::lan::{NetConfig, hub::HubConfig};
use horizon_protocol::packets::{Empty, configuration as cfg, handshake, login, play};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::{io::Write, time::{Duration, Instant}};

const PORT: u16 = 25582;
const REPS: usize = 8;
const FLOOR: i32 = -61;
const SURFACE: i32 = FLOOR + 1;

struct Parcel {
    ox: i32,
    oz: i32,
}

fn parcels() -> Vec<Parcel> {
    (0..REPS as i32).map(|i| Parcel { ox: 1024 + (i % 4) * 160, oz: 1024 + (i / 4) * 160 }).collect()
}

fn env_ticks() -> usize {
    std::env::var("PROBE_TICKS").ok().and_then(|v| v.parse().ok()).unwrap_or(400)
}

fn say(log: &mut std::fs::File, line: &str) {
    println!("[probe] {line}");
    let _ = writeln!(log, "{line}");
}

/// A protocol client that joins as `name`, loads, answers keep-alives and drops every other packet until `stop`.
fn bot(name: String, stop: Arc<AtomicBool>) -> Result<(), String> {
    use horizon_protocol::{PROTOCOL_VERSION, connection::Connection, crypto::offline_uuid};
    let e = |e: horizon_protocol::Error| e.to_string();
    let mut c = Connection::connect("127.0.0.1", PORT, Duration::from_secs(10)).map_err(e)?;
    c.reader.set_read_timeout(Some(Duration::from_secs(60))).map_err(e)?;
    let hello = handshake::Intention { protocol: PROTOCOL_VERSION, host: "127.0.0.1".into(), port: PORT, intent: handshake::LOGIN };
    c.send(&handshake::Serverbound::Intention(hello).encode()).map_err(e)?;
    c.send(&login::Serverbound::Hello(login::ServerHello { name: name.clone(), uuid: offline_uuid(&name) }).encode()).map_err(e)?;
    loop {
        let (id, body) = c.recv().map_err(e)?;
        match login::Clientbound::decode(id, &body).map_err(e)? {
            login::Clientbound::Compression(p) => c.set_compression(p.threshold),
            login::Clientbound::LoginFinished(_) => break c.send(&login::Serverbound::LoginAcknowledged(Empty).encode()).map_err(e)?,
            login::Clientbound::Disconnect(d) => return Err(d.reason_json),
            other => return Err(format!("login: {other:?}")),
        }
    }
    c.send(&cfg::Serverbound::ClientInformation(cfg::ClientInformation::new(2)).encode()).map_err(e)?;
    loop {
        let (id, body) = c.recv().map_err(e)?;
        match cfg::Clientbound::decode(id, &body).map_err(e)? {
            cfg::Clientbound::SelectKnownPacks(p) => c.send(&cfg::Serverbound::SelectKnownPacks(cfg::SelectKnownPacks { packs: p.packs }).encode()).map_err(e)?,
            cfg::Clientbound::KeepAlive(k) => c.send(&cfg::Serverbound::KeepAlive(k).encode()).map_err(e)?,
            cfg::Clientbound::FinishConfiguration(_) => break c.send(&cfg::Serverbound::FinishConfiguration(Empty).encode()).map_err(e)?,
            _ => {}
        }
    }
    c.reader.set_read_timeout(Some(Duration::from_millis(200))).map_err(e)?;
    while !stop.load(Ordering::Relaxed) {
        match c.recv() {
            Ok((id, body)) => match play::Clientbound::decode(id, &body) {
                Ok(play::Clientbound::Login(_)) => c.send(&play::Serverbound::PlayerLoaded(Empty).encode()).map_err(e)?,
                Ok(play::Clientbound::KeepAlive(k)) => c.send(&play::Serverbound::KeepAlive(k).encode()).map_err(e)?,
                Ok(play::Clientbound::Disconnect(d)) => return Err(format!("{d:?}")),
                _ => {}
            },
            Err(x) if x.is_timed_out() => {}
            Err(x) => return Err(x.to_string()),
        }
    }
    Ok(())
}

fn command(game: &mut Game<'_>, log: &mut std::fs::File, line: &str) -> bool {
    game.chat_capture = Some(Vec::new());
    let ok = game.run_command(line);
    let said: Vec<String> = game.chat_capture.take().unwrap_or_default().iter().map(|t| t.plain()).collect();
    if !ok {
        say(log, &format!("command refused: {line}: {}", said.join(" / ")));
    }
    ok
}

fn tick(game: &mut Game<'_>) {
    game.tick_server(false).expect("the server tick");
}

fn region(p: &Parcel, y: i32) -> String {
    format!("{} {y} {} {} {y} {}", p.ox - 2, p.oz - 3, p.ox + 2, p.oz + 3)
}

/// Despawns the creatures this probe made and clears the pool area.
fn clear(game: &mut Game<'_>, log: &mut std::fs::File, spawned: &mut Vec<u32>, scene: &[Parcel]) {
    for id in spawned.drain(..) {
        if let Some(e) = game.ecs.entity_of(id) {
            game.ecs.despawn(e);
        }
    }
    for p in scene {
        game.run_command_quietly(&format!("fill {} {} {} {} {} {} air", p.ox - 2, SURFACE, p.oz - 3, p.ox + 2, SURFACE + 1, p.oz + 3));
    }
}

fn read_row(game: &Game<'_>, id: u32, p: &Parcel, prev: Option<&Row>) -> Row {
    let Some(at) = game.ecs.position(id) else {
        let mut r = prev.copied().unwrap_or([0.0; 12]);
        (r[VX], r[VY], r[VZ], r[ALIVE], r[TARGET]) = (0.0, 0.0, 0.0, 0.0, 0.0);
        return r;
    };
    let (x, y, z) = (at[0] - f64::from(p.ox), at[1] - f64::from(SURFACE) + 1.0, at[2] - f64::from(p.oz));
    let yaw = game.ecs.yaw(id).map_or(f64::NAN, f64::from);
    let head = game.ecs.head_body_yaw(id).filter(|(h, _)| !h.is_nan());
    let target = game.ecs.entity_of(id).is_some_and(|e| game.ecs.ecs().is_some_and(|w| w.ai_target(e).is_some()));
    let health = game.ecs.health(id).map_or_else(|| prev.map_or(0.0, |r| r[HEALTH]), f64::from);
    let (vx, vy, vz) = prev.map_or((0.0, 0.0, 0.0), |r| (x - r[X], y - r[Y], z - r[Z]));
    [x, y, z, vx, vy, vz, head.map_or(yaw, |(_, b)| f64::from(b)), health, f64::from(u8::from(game.ecs.alive(id))), f64::from(u8::from(target)), yaw, head.map_or(yaw, |(h, _)| f64::from(h))]
}

/// Keeps each bot's guest 4 blocks east of its mob, immobile and unhurt but still a target.
fn place_players(game: &mut Game<'_>, scene: &[Parcel], index: &[usize]) {
    let hub = &mut game.lan.as_mut().expect("the hub").hub;
    for (p, i) in scene.iter().zip(index) {
        hub.guests.update(*i, |g| {
            g.position = [f64::from(p.ox) + 4.5, f64::from(SURFACE), f64::from(p.oz) + 0.5];
            g.previous_position = g.position;
            g.health = 20.0;
        });
    }
}

/// One species in the eight parcels at once: summon, `ticks` server ticks (or until every creature is gone), the rows of each parcel.
fn batch(game: &mut Game<'_>, log: &mut std::fs::File, sp: &str, pool: bool, players: Option<&[usize]>, scene: &[Parcel], spawned: &mut Vec<u32>) -> Result<Vec<Vec<Row>>, String> {
    clear(game, log, spawned, scene);
    for _ in 0..3 {
        if let Some(i) = players {
            place_players(game, scene, i);
        }
        tick(game);
    }
    let mut ids = Vec::new();
    for p in scene {
        if pool {
            command(game, log, &format!("fill {} water", region(p, SURFACE)));
            command(game, log, &format!("fill {} water", region(p, SURFACE + 1)));
            let block = game.res.facts().get(game.world.block(p.ox, SURFACE, p.oz)).name.to_string();
            if !block.ends_with("water") {
                return Err(format!("the pool holds {block}, not water"));
            }
        }
        let before: std::collections::HashSet<u32> = game.ecs.all_ids().into_iter().collect();
        if !command(game, log, &format!("summon {sp} {}.5 {SURFACE} {}.5", p.ox, p.oz)) {
            return Err("summon refused".into());
        }
        let mut born: Vec<u32> = game.ecs.all_ids().into_iter().filter(|n| !before.contains(n)).collect();
        born.sort();
        let short = sp.trim_start_matches("minecraft:");
        let id = *born.iter().find(|n| game.ecs.kind(**n).is_some_and(|k| k.trim_start_matches("minecraft:") == short))
            .ok_or_else(|| format!("summon made no {sp} (born {})", born.len()))?;
        spawned.extend(&born);
        for extra in born.iter().filter(|n| **n != id) {
            if let Some(e) = game.ecs.entity_of(*extra) {
                game.ecs.despawn(e);
            }
        }
        let at = game.ecs.position(id).ok_or("the creature has no position")?;
        command(game, log, &format!("data merge entity @e[type={sp},x={},y={},z={},sort=nearest,limit=1] {{Age:0,IsBaby:0b,Rotation:[0.0f,0.0f]}}", at[0], at[1], at[2]));
        ids.push(id);
    }
    let mut rows: Vec<Vec<Row>> = scene.iter().zip(&ids).map(|(p, id)| vec![read_row(game, *id, p, None)]).collect();
    for _ in 0..env_ticks() {
        if let Some(i) = players {
            place_players(game, scene, i);
        }
        tick(game);
        let mut any = false;
        for (k, (p, id)) in scene.iter().zip(&ids).enumerate() {
            if rows[k].last().is_some_and(|r| r[ALIVE] != 1.0) {
                continue;
            }
            let row = read_row(game, *id, p, rows[k].last());
            any |= row[ALIVE] == 1.0;
            rows[k].push(row);
        }
        if !any {
            break;
        }
    }
    Ok(rows.into_iter().filter(|r| r.len() > 1).collect())
}

fn scene_up(game: &mut Game<'_>, log: &mut std::fs::File, scene: &[Parcel]) {
    for line in ["difficulty normal", "gamerule spawn_mobs false", "gamerule spawn_monsters false", "gamerule spawn_patrols false", "gamerule spawn_phantoms false",
        "gamerule spawn_wandering_traders false", "gamerule spawn_wardens false", "gamerule advance_time false", "gamerule advance_weather false",
        "gamerule mob_griefing false", "weather clear"] {
        command(game, log, line);
    }
    command(game, log, &format!("time set {}", std::env::var("PROBE_TIME").unwrap_or_else(|_| "18000".into())));
    for p in scene {
        for cx in (p.ox >> 4) - 4..=(p.ox >> 4) + 4 {
            for cz in (p.oz >> 4) - 4..=(p.oz >> 4) + 4 {
                game.world.chunks.insert((cx, cz), Arc::new(Chunk::empty(cx, cz)));
                game.block_ticks.add_empty_chunk(horizon_core::position::BlockPos::new(cx * 16, 0, cz * 16).chunk()).expect("block ticks chunk");
                game.ecs_load_chunk(cx, cz, &[]);
            }
        }
        let (x1, z1, x2, z2) = (p.ox - 64, p.oz - 64, p.ox + 79, p.oz + 79);
        command(game, log, &format!("forceload add {x1} {z1} {x2} {z2}"));
        for (block, lo, hi) in [("bedrock", -64, -64), ("dirt", -63, -63), ("dirt", -62, -62), ("grass_block", FLOOR, FLOOR)] {
            command(game, log, &format!("fill {x1} {lo} {z1} {x2} {hi} {z2} {block}"));
        }
    }
    for _ in 0..40 {
        tick(game);
    }
}

fn join_players(game: &mut Game<'_>, stop: &Arc<AtomicBool>) -> Result<(Vec<std::thread::JoinHandle<Result<(), String>>>, Vec<usize>), String> {
    let settings = crate::game::lan::LanSettings { port: PORT, guest_commands: false, force_game_mode: false };
    let net = NetConfig::loopback(PORT, "Probe".into(), REPS + 1);
    let config = HubConfig {
        default_game_mode: 0, guest_commands: false, force_game_mode: false, hardcore: false, view_distance: 2, simulation_distance: 2,
        dimension: "minecraft:overworld".into(), allow_flight: true, seed: 0, max_players: REPS + 1, levels: Default::default(), level_seeds: Default::default(),
    };
    game.lan_host(settings, net, config).map_err(|e| e.to_string())?;
    let bots: Vec<_> = (0..REPS).map(|i| {
        let stop = stop.clone();
        std::thread::spawn(move || bot(format!("Probe{i}"), stop))
    }).collect();
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        tick(game);
        let hub = &game.lan.as_ref().expect("the hub").hub;
        let index: Vec<Option<usize>> = (0..REPS).map(|i| hub.guests.iter().position(|g| g.placed && g.name == format!("Probe{i}"))).collect();
        if index.iter().all(Option::is_some) {
            return Ok((bots, index.into_iter().flatten().collect()));
        }
        if Instant::now() > deadline || bots.iter().any(|b| b.is_finished()) {
            return Err("the bots did not all join".into());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
#[ignore = "a probe, not a test: run it by hand"]
fn behaviour_probe() {
    let Some(out) = std::env::var_os("MYRIADE_BEHAVIOUR") else {
        println!("MYRIADE_BEHAVIOUR is not set");
        return;
    };
    let mut log = std::fs::File::create(std::path::Path::new(&out).with_extension("log")).expect("the probe journal");
    let mut resources = None;
    let mut game = active_server_game(&mut resources);
    game.build_registries();
    game.set_allow_commands(true);
    let scene = parcels();
    let only: Vec<String> = std::env::var("PROBE_SPECIES").unwrap_or_default().split(',').map(str::trim).filter(|s| !s.is_empty())
        .map(|s| if s.contains(':') { s.to_string() } else { format!("minecraft:{s}") }).collect();
    let mut species: Vec<(String, bool)> = game.res.mob_types().iter().filter(|(id, _)| only.is_empty() || only.contains(id))
        .map(|(id, t)| (id.clone(), matches!(t.category.trim_start_matches("minecraft:"), "water_creature" | "water_ambient" | "underground_water_creature"))).collect();
    species.sort();
    scene_up(&mut game, &mut log, &scene);
    let (stop, mut spawned, started) = (Arc::new(AtomicBool::new(false)), Vec::new(), Instant::now());
    let mut tsv = String::from("species\tscenario\tmetric\tmean\tmin\tmax\tn\n");
    let mut failed = std::collections::HashSet::new();
    let (bots, index) = match join_players(&mut game, &stop) {
        Ok(joined) => joined,
        Err(e) => {
            say(&mut log, &format!("fatal player: {e}"));
            return;
        }
    };
    let players = Some(index);
    for scenario in ["alone", "player"] {
        // The server simulates a creature only near a player: alone, the connected player is a spectator, which no creature targets.
        command(&mut game, &mut log, if scenario == "alone" { "gamemode spectator @a" } else { "gamemode survival @a" });
        for (sp, pool) in &species {
            if failed.contains(sp) {
                continue;
            }
            let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| batch(&mut game, &mut log, sp, *pool, players.as_deref(), &scene, &mut spawned)));
            match run {
                Ok(Ok(rows)) if !rows.is_empty() => {
                    let runs: Vec<Measures> = rows.iter().map(|r| measures(r, scenario == "player")).collect();
                    tsv.push_str(&aggregate(sp, scenario, &runs));
                }
                Ok(Ok(_)) => say(&mut log, &format!("{sp} {scenario}: no run")),
                Ok(Err(e)) => {
                    say(&mut log, &format!("error {sp} {scenario}: {e}"));
                    failed.insert(sp.clone());
                }
                Err(p) => {
                    let msg = p.downcast_ref::<String>().cloned().or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
                    say(&mut log, &format!("error {sp} {scenario}: panic {}", msg.replace(['\t', '\n'], " ")));
                    failed.insert(sp.clone());
                }
            }
        }
        say(&mut log, &format!("phase {scenario} done after {} s", started.elapsed().as_secs()));
    }
    stop.store(true, Ordering::Relaxed);
    for b in bots {
        if let Err(e) = b.join().expect("a bot") {
            say(&mut log, &format!("bot ended: {e}"));
        }
    }
    std::fs::write(&out, tsv).expect("the behaviour file");
    say(&mut log, &format!("elapsed {} s", started.elapsed().as_secs()));
}
