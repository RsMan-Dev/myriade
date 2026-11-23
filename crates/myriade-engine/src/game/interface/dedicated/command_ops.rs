//! The control socket's `op <player> <json>`: one operation of the command harness
//! (the integrated server's `command_harness.rs`, removed), run as the named guest.
//! Its record has the keys the harness wrote to its output file; a `null` reply is an operation
//! that records nothing. Errors reply `{"error"}`.
use crate::game::{Dim, Game};
use myriade_engine::{Result, text::Text};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

/// A message as `[text, style]` runs with adjacent equal styles merged (the style lists colour,
/// bold, italic, underlined, strikethrough, obfuscated).
pub(super) fn spans(text: &Text) -> Value {
    let mut out: Vec<(String, Value)> = Vec::new();
    for span in &text.spans {
        if span.text.is_empty() {
            continue;
        }
        let s = &span.style;
        let style = json!([
            s.color,
            s.bold == Some(true),
            s.italic == Some(true),
            s.underlined == Some(true),
            s.strikethrough == Some(true),
            s.obfuscated == Some(true),
        ]);
        match out.last_mut() {
            Some((t, last)) if *last == style => t.push_str(&span.text),
            _ => out.push((span.text.clone(), style)),
        }
    }
    Value::Array(out.into_iter().map(|(t, s)| json!([t, s])).collect())
}

/// A UUID as the harness writes it, with or without dashes.
fn uuid_of(value: &Value) -> Result<u128> {
    let text = value.as_str().ok_or("missing UUID")?.replace('-', "");
    Ok(u128::from_str_radix(&text, 16)?)
}

/// The deadline of a wait, as the harness's `HORIZON_HARNESS_TIMEOUT` gave it (600 s by default).
fn guard() -> Instant {
    let secs = std::env::var("HORIZON_HARNESS_TIMEOUT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(600);
    Instant::now() + Duration::from_secs(secs)
}

impl Game<'_> {
    /// `op <player> <json>`: the reply is the operation's record, or `{"error"}`.
    pub(super) fn op_reply(&mut self, rest: &str) -> Value {
        let Some((name, json)) = rest.trim().split_once(' ') else {
            return json!({ "error": "op takes a player and an operation" });
        };
        let guest = self.lan.as_ref().and_then(|h| {
            h.hub
                .guests
                .iter()
                .find(|g| g.name == name)
                .map(|g| g.conn)
        });
        let Some(conn) = guest else {
            return json!({ "error": format!("no player {name}") });
        };
        let op: Value = match serde_json::from_str(json) {
            Ok(op) => op,
            Err(e) => return json!({ "error": format!("op: {e}") }),
        };
        self.op_record(conn, &op)
            .unwrap_or_else(|e| json!({ "error": e.to_string() }))
    }

    /// The guest's body and level in place for `f`: the harness's owner swap, for the named guest.
    fn op_as_guest<T>(&mut self, conn: u32, f: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        let dim = self
            .lan
            .as_ref()
            .and_then(|h| h.hub.guest(conn).filter(|g| g.placed))
            .map(|g| Dim::from_id(&g.dimension))
            .ok_or("op: player is not connected")?;
        let back = self.dim;
        self.enter_level(dim)?;
        let swap = self
            .enter_guest_in_dimension(conn, back)
            .ok_or("op: player is already acting")?;
        let result = f(self);
        self.leave_guest(swap);
        if let Some(host) = self.lan.as_mut() {
            host.hub.sync_all_effects();
        }
        self.run_level_teleports();
        let restored = self.enter_level(back);
        restored?;
        result
    }

    /// The guest's authoritative block or crafting menu, without client prediction:
    /// (connection, own slots, window).
    fn op_menu(&self, conn: u32) -> Result<(u32, usize, i32)> {
        let host = self.lan.as_ref().ok_or("menu state: no hub")?;
        let guest = host
            .hub
            .guest(conn)
            .filter(|p| p.placed)
            .ok_or("menu state: player is not connected")?;
        if let Some(menu) = &guest.menu {
            return Ok((conn, menu.size, menu.window));
        }
        if let Some((window, _, menu, _)) = host
            .menus
            .get(&conn)
            .and_then(|p| p.crafting.table.as_ref())
        {
            return Ok((conn, menu.slot_count() - 36, *window));
        }
        if let Some(station) = host.menus.get(&conn).and_then(|p| p.station.as_ref()) {
            return Ok((conn, station.menu.slot_count() - 36, station.window));
        }
        Ok((conn, 0, 0))
    }

    /// The portal the guest stands in finishes its trip (the oracle's player is not ticked).
    fn op_portal(&mut self, conn: u32) -> Result<()> {
        let (acting, plan) = self.op_as_guest(conn, |g| {
            let p = g.player.as_ref().ok_or("command harness: portal without a player")?;
            let entry = horizon_core::position::BlockPos::containing(p.position);
            let block = g.world.block(entry.x, entry.y, entry.z);
            if &*g.res.facts().get(block).name != "minecraft:nether_portal" {
                return Err("command harness: the player is not in a nether portal".into());
            }
            let plan = g.portal_plan(
                entry,
                p.position,
                p.dimensions.width,
                p.dimensions.height,
                true,
            );
            Ok((g.lan_acting.ok_or("command harness: no acting owner")?, plan))
        })?;
        // The trip starts after the guest's body is restored: change_level updates its hub record.
        if self.lan.is_none() {
            return Err("command harness: no hub".into());
        }
        self.start_server_trip(acting, plan.dim, Some(plan), false);
        Ok(())
    }

    /// Load terrain until the player stands in lit chunks, or the deadline passes.
    /// `terrain_only`: the wait ends once the asked chunks are loaded, even while more terrain generates.
    fn op_settle(&mut self, deadline: Instant, required: Option<&Value>, terrain_only: bool) -> Result<()> {
        loop {
            self.update_world();
            // The oracle's settle takes its tickets at once; `/execute if loaded` reads them.
            self.update_chunk_tickets();
            if let Some(p) = &self.player {
                let (cx, cz) = (
                    (p.position.x.floor() as i32) >> 4,
                    (p.position.z.floor() as i32) >> 4,
                );
                if (terrain_only
                    || !self.streaming.busy())
                    && required.and_then(Value::as_array).map_or_else(
                        || (-1..=1).all(|dz| (-1..=1).all(|dx| self.streaming.is_lit(cx + dx, cz + dz))),
                        |chunks| {
                            chunks.iter().all(|chunk| {
                                chunk[0]
                                    .as_i64()
                                    .zip(chunk[1].as_i64())
                                    .is_some_and(|(x, z)| self.streaming.is_lit(x as i32, z as i32))
                            })
                        },
                    )
                {
                    // The oracle's settle also waits for the entities saved in those chunks.
                    self.spawn_loaded_entities();
                    self.world.entities_pending.clear();
                    return Ok(());
                }
            }
            if Instant::now() > deadline {
                return Err("command harness: terrain did not load".into());
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    /// The model's `cmd` from the console source: the command runs as the console in the guest's body,
    /// and its messages and results are the record's.
    fn op_console(&mut self, conn: u32, op: &Value) -> Result<Value> {
        let raw = op["cmd"].as_str().unwrap_or("");
        let command = raw.strip_prefix('/').unwrap_or(raw).to_owned();
        let (messages, results) = self.op_as_guest(conn, |g| {
            g.chat_capture = Some(Vec::new());
            g.cmds.results = Some(Vec::new());
            let previous = g.command_source;
            let mut source = g.server_source();
            source.silent = false;
            g.command_source = Some(source);
            g.as_console(|g| g.run_command(&command));
            g.command_source = previous;
            Ok((g.chat_capture.take().unwrap_or_default(), g.cmds.results.take().unwrap_or_default()))
        })?;
        if op["record"].as_bool() == Some(false) {
            return Ok(Value::Null);
        }
        Ok(json!({"cmd": raw,
            "messages": messages.iter().map(spans).collect::<Vec<_>>(),
            "results": results.iter().map(|(s, v)| json!([s, v])).collect::<Vec<_>>()}))
    }

    /// One operation for the guest `conn`: its record, `Null` when the operation records nothing.
    fn op_record(&mut self, conn: u32, op: &Value) -> Result<Value> {
        let label = op["label"].as_str().unwrap_or("");
        Ok(match op["op"].as_str().unwrap_or("") {
            "cmd" if op["source"].as_str() == Some("console") => self.op_console(conn, op)?,
            "cmd" => {
                let raw = op["cmd"].as_str().unwrap_or("");
                let command = raw.strip_prefix('/').unwrap_or(raw).to_owned();
                let permission = self
                    .lan
                    .as_ref()
                    .and_then(|h| h.hub.guest(conn))
                    .map(|g| g.permission)
                    .ok_or("op: player is not connected")?;
                let (result, ok, lines) = self
                    .run_guest_result(crate::game::lan_actions::GuestCommand {
                        conn,
                        command,
                        permission,
                        quiet: false,
                    })
                    .ok_or("op: player is not connected")?;
                if op["record"].as_bool() == Some(false) {
                    return Ok(Value::Null);
                }
                json!({"cmd": raw,
                    "messages": lines.iter().map(spans).collect::<Vec<_>>(),
                    "results": [[ok, result]]})
            }
            "tick" => {
                let paced = Instant::now();
                self.chat_capture = Some(Vec::new());
                for i in 0..op["n"].as_u64().unwrap_or(1) {
                    // The profile resolver answers at once; the services refuse on a thread a tick can outrun.
                    let asked = Instant::now();
                    while self.profile_fetches_pending() && asked.elapsed() < Duration::from_secs(10) {
                        std::thread::sleep(Duration::from_millis(1));
                    }
                    self.tick_server(false)?;
                    if let Some(ms) = op["pace_ms"].as_u64() {
                        std::thread::sleep(
                            Duration::from_millis(ms * (i + 1)).saturating_sub(paced.elapsed()),
                        );
                    }
                }
                let messages = self.chat_capture.take().unwrap_or_default();
                if op["record"].as_bool() != Some(true) {
                    return Ok(Value::Null);
                }
                json!({"cmd": "#tick", "messages": messages.iter().map(spans).collect::<Vec<_>>(), "results": []})
            }
            "settle" => {
                let deadline = guard();
                // Finish portal arrivals using connection-only passes too.
                while !self.server_trips.is_empty() {
                    self.tick_server(true)?;
                    if Instant::now() > deadline {
                        return Err("command harness: portal did not arrive".into());
                    }
                    std::thread::sleep(Duration::from_millis(2));
                }
                self.op_as_guest(conn, |g| g.op_settle(deadline, op.get("required_chunks"), op.get("terrain_only").and_then(Value::as_bool).unwrap_or(false)))?;
                Value::Null
            }
            "wait_chunks" => {
                let deadline = guard();
                let targets: Vec<(i32, i32)> = op["chunks"]
                    .as_array()
                    .ok_or("missing chunks")?
                    .iter()
                    .map(|p| (p[0].as_i64().unwrap_or(0) as i32, p[1].as_i64().unwrap_or(0) as i32))
                    .collect();
                self.op_as_guest(conn, |g| {
                    loop {
                        g.update_world();
                        g.spawn_loaded_entities();
                        if targets.iter().all(|&(x, z)| {
                            g.world.chunks.contains_key(&(x, z))
                                && g.streaming.is_lit(x, z)
                                && !g.world.loaded_entities.iter().any(|(key, _)| *key == (x, z))
                        }) {
                            // The oracle waits for `areEntitiesLoaded` too.
                            g.world.entities_pending.clear();
                            break;
                        }
                        if Instant::now() > deadline {
                            return Err("initial target readiness timed out".into());
                        }
                        std::thread::sleep(Duration::from_millis(2));
                    }
                    Ok(())
                })?;
                Value::Null
            }
            "use_item" => {
                // With `sequence`, the packet is the owner's normal route and is acknowledged;
                // without it, the packet is sent at zero angles and nothing is checked.
                let seq_form = op.get("sequence").is_some();
                let sequence = if seq_form { op["sequence"].as_i64().unwrap_or(0) as i32 } else { 0 };
                let angle = |key: &str| if seq_form { op[key].as_f64().unwrap_or(0.0) as f32 } else { 0.0 };
                let (yaw, pitch) = (angle("yaw"), angle("pitch"));
                self.op_as_guest(conn, |g| -> Result<()> {
                    g.lan_guest_packet(
                        conn,
                        horizon_protocol::packets::play::Serverbound::UseItem(
                            horizon_protocol::packets::play::UseItem { hand: 0, yaw, pitch, sequence },
                        ),
                    );
                    if seq_form {
                        let ack = g.lan.as_ref().is_some_and(|host| {
                            host.hub.guest(conn).is_some_and(|guest| guest.ack_up_to.get() >= sequence)
                        });
                        if !ack {
                            return Err("command harness: UseItem was not acknowledged".into());
                        }
                    }
                    Ok(())
                })?;
                Value::Null
            }
            // Java produced the fixture file before the world was opened; map_state loads it.
            "crafter_max_map_fixture" => Value::Null,
            "use_block" => {
                let pos: [i32; 3] = std::array::from_fn(|i| op["pos"][i].as_i64().unwrap_or(0) as i32);
                let sequence = op["sequence"].as_i64().unwrap_or(0) as i32;
                let face = op["face"].as_i64().unwrap_or(1) as i32;
                self.op_as_guest(conn, |g| -> Result<()> {
                    g.lan_guest_packet(
                        conn,
                        horizon_protocol::packets::play::Serverbound::UseItemOn(
                            horizon_protocol::packets::play::UseItemOn {
                                hand: 0,
                                pos,
                                face,
                                cursor: [0.5, 1.0, 0.5],
                                inside: false,
                                world_border: false,
                                sequence,
                            },
                        ),
                    );
                    if !g.lan.as_ref().and_then(|h| h.hub.guest(conn)).is_some_and(|p| p.ack_up_to.get() >= sequence) {
                        return Err("use block: owner packet was not acknowledged".into());
                    }
                    Ok(())
                })?;
                Value::Null
            }
            "break" => {
                let pos: [i32; 3] = std::array::from_fn(|i| op["pos"][i].as_i64().unwrap_or(0) as i32);
                self.op_as_guest(conn, |g| g.break_block(pos))?;
                Value::Null
            }
            "portal" => {
                self.op_portal(conn)?;
                Value::Null
            }
            "destroy_progress" => {
                let pos: [i32; 3] = std::array::from_fn(|i| op["pos"][i].as_i64().unwrap_or(0) as i32);
                let bits = self.op_as_guest(conn, |g| {
                    if let Some(player) = g.player.as_mut() {
                        player.ground.on_ground = true;
                    }
                    Ok(g.destroy_progress(pos).to_bits())
                })?;
                json!({"cmd": "#destroy_progress", "messages": [], "results": [[true, bits]]})
            }
            "falling" => {
                // Physical falling blocks are not command-selector targets yet: read the level itself.
                let dimension = op["dimension"].as_str().unwrap_or("minecraft:overworld");
                let block = op["block"].as_str().unwrap_or("minecraft:sand");
                let no_gravity = op["no_gravity"].as_bool().unwrap_or(false);
                let back = self.dim;
                self.enter_level(Dim::from_id(dimension))?;
                let matched: Vec<u32> = self
                    .ecs
                    .species_ids("minecraft:falling_block")
                    .into_iter()
                    .filter(|&id| {
                        let Some(tree) = self.ecs_mob_tree(id) else {
                            return false;
                        };
                        let named = matches!(tree.get(&"BlockState".into()), Some(horizon_core::nbt::Tag::String(s)) if s.to_string_lossy() == block);
                        let gravity = matches!(tree.get(&"NoGravity".into()), Some(horizon_core::nbt::Tag::Byte(v)) if *v != 0);
                        named && gravity == no_gravity
                    })
                    .collect();
                let count = matched.len();
                if op["discard"].as_bool() == Some(true) {
                    for id in matched {
                        if let Some(e) = self.ecs.entity_of(id) {
                            self.ecs.despawn(e);
                        }
                    }
                }
                self.enter_level(back)?;
                json!({"cmd": format!("#falling {dimension} {block} {no_gravity}"), "messages": [], "results": [[true, count]]})
            }
            "player_loop" => {
                let ticks = op["n"].as_u64().unwrap_or(1);
                let count = |g: &mut Self| {
                    Ok(g.stats
                        .values
                        .get("minecraft:custom")
                        .and_then(|m| m.get("minecraft:play_time"))
                        .copied()
                        .unwrap_or(0))
                };
                let before = self.op_as_guest(conn, count)?;
                self.chat_capture = Some(Vec::new());
                for _ in 0..ticks {
                    self.tick_server(false)?;
                }
                let messages = self.chat_capture.take().unwrap_or_default();
                let delta = self.op_as_guest(conn, count)? - before;
                if delta != ticks as i32 {
                    return Err(format!("player loop did not advance play_time: {delta}/{ticks}").into());
                }
                json!({"cmd": format!("#player_loop {label}"), "messages": messages.iter().map(spans).collect::<Vec<_>>(), "results": [[true, delta]]})
            }
            "player_state" => {
                let state = self.op_as_guest(conn, |g| {
                    let p = g.player.as_ref().ok_or("player state: no actor")?;
                    let mut loaded = Vec::new();
                    let mut done = std::collections::BTreeMap::new();
                    for a in g.adv.tree.list.iter().filter(|a| a.id.starts_with("h311:")) {
                        loaded.push(a.id.clone());
                        done.insert(a.id.clone(), g.adv.progress.done(a));
                    }
                    loaded.sort();
                    Ok(json!({"health": p.health.health(),
                        "poison_ticks": g.potions.effects.list.iter().find(|e| e.id == "poison").map_or(0, |e| e.duration),
                        "loaded": loaded, "done": done}))
                })?;
                json!({"cmd": format!("#player_state {label}"), "messages": [], "results": [], "state": state})
            }
            "projectile_saved_type" => {
                // The model stopped here: its projectile list is gone, so no record is produced.
                let uuid = uuid_of(&op["uuid"])?;
                self.op_as_guest(conn, |g| -> Result<Value> {
                    let Some(crate::game::commands::Target::Projectile(id)) = g.target_by_uuid(uuid) else {
                        return Err("projectile type: missing loaded projectile".into());
                    };
                    let actual = *g.persist.uuids.get(&id).ok_or("projectile type: missing actual UUID")?;
                    if myriade_engine::lan::playerdata::ints_uuid(actual) != uuid {
                        return Err("projectile type: UUID mismatch".into());
                    }
                    Err("projectile type: missing model".into())
                })?
            }
            "raid_state" => {
                let state = self.op_as_guest(conn, |g| g.raid_fixture_state(op))?;
                json!({"cmd": format!("#raid_state {} {}", label, op["id"]), "messages": [], "results": [], "state": state})
            }
            "mob_anger" => {
                let uuid = uuid_of(&op["uuid"])?;
                let index = op["index"].as_u64().ok_or("anger state: missing index")? as u8;
                let state = self.op_as_guest(conn, |g| {
                    let Some(crate::game::commands::Target::Mob(id)) = g.target_by_uuid(uuid) else {
                        return Err("anger state: UUID is not a loaded mob".into());
                    };
                    let actual = g.persist.uuids.get(&id).copied().ok_or("anger state: no saved UUID")?;
                    if myriade_engine::lan::playerdata::ints_uuid(actual) != uuid {
                        return Err("anger state: UUID mismatch".into());
                    }
                    if !g.ecs.alive(id) {
                        return Err("anger state: missing live actor".into());
                    }
                    let kind = g.ecs.kind(id).ok_or("anger state: missing live actor")?;
                    if Some(kind) != op["kind"].as_str() {
                        return Err("anger state: wrong actor type".into());
                    }
                    g.lan.as_ref().ok_or("anger state: no connected hub")?;
                    let mut meta = Vec::new();
                    let defaults: Vec<(u8, i32, horizon_protocol::entity_data::Value)> = Vec::new();
                    let e = g.ecs.entity_of(id).ok_or("anger state: missing live actor")?;
                    g.ecs.metadata(e, &mut meta);
                    let entry = meta
                        .iter()
                        .find(|e| e.0 == index)
                        .or_else(|| defaults.iter().find(|e| e.0 == index))
                        .ok_or("anger state: metadata entry/default absent")?;
                    if entry.1 != horizon_protocol::entity_data::serializer::LONG {
                        return Err("anger state: metadata serializer is not LONG".into());
                    }
                    let horizon_protocol::entity_data::Value::Long(value) = &entry.2 else {
                        return Err("anger state: metadata value is not long".into());
                    };
                    Ok(json!({"uuid": actual, "kind": kind, "index": index,
                        "serializer": entry.1, "value": value, "game_time": g.world.game_time}))
                })?;
                json!({"cmd": format!("#mob_anger {label}"), "messages": [], "results": [], "state": state})
            }
            "server_distances" => {
                let state = self.op_as_guest(conn, |g| {
                    let hub = &g.lan.as_ref().ok_or("no connected hub")?.hub;
                    Ok([hub.config.view_distance, hub.config.simulation_distance])
                })?;
                json!({"cmd": "#server_distances", "messages": [], "results": state.map(|n| json!([true, n]))})
            }
            "chunk_ready" => {
                let [x, z] = std::array::from_fn(|i| op["chunk"][i].as_i64().unwrap_or(0) as i32);
                let state = self.op_as_guest(conn, |g| {
                    let resident = g.world.chunks.contains_key(&(x, z));
                    Ok([
                        resident,
                        g.streaming.is_lit(x, z),
                        resident && !g.world.loaded_entities.iter().any(|(key, _)| *key == (x, z)),
                    ])
                })?;
                json!({"cmd": format!("#chunk_ready {x} {z}"), "messages": [], "results": state.map(|ready| json!([true, i32::from(ready)]))})
            }
            "weather_state" => {
                // Saved state guards, separate from effective-weather predicates.
                let state = self.op_as_guest(conn, |g| {
                    let w = g.world.weather;
                    Ok([i32::from(w.raining), i32::from(w.thundering), w.clear_time, w.rain_time, w.thunder_time])
                })?;
                json!({"cmd": format!("#weather_state {label}"), "messages": [],
                    "results": state.iter().map(|value| json!([true, value])).collect::<Vec<_>>()})
            }
            "frame_map_state" => {
                let uuid = uuid_of(&op["uuid"])?;
                let map_id = op["id"].as_i64().ok_or("frame map: missing map id")? as i32;
                let state = self.op_as_guest(conn, |g| {
                    let Some(crate::game::commands::Target::Frame(id)) = g.target_by_uuid(uuid) else {
                        return Err("frame map: UUID is not a live frame".into());
                    };
                    let actual = *g.persist.uuids.get(&id).ok_or("frame map: no UUID")?;
                    let frame = g.ecs.entity_of(id).and_then(|e| g.frame_from_ecs(e)).ok_or("frame map: missing frame")?;
                    if myriade_engine::lan::playerdata::ints_uuid(actual) != uuid {
                        return Err("frame map: UUID mismatch".into());
                    }
                    let map = g.map(map_id).ok_or("frame map: no backed map")?;
                    let saved = map.to_nbt();
                    let data = saved.get("data").ok_or("frame map: no saved data compound")?;
                    let records = match data.get("frames") {
                        None => &[][..], // the CODEC omits an empty list
                        Some(horizon_core::nbt::Tag::List(list)) => list.as_slice(),
                        _ => return Err("frame map: frames is not a list".into()),
                    };
                    let record = records
                        .iter()
                        .find(|r| myriade_engine::save::number(r.get("entity_id")) == Some(i64::from(id)));
                    let pos = record
                        .map(|r| match r.get("pos") {
                            Some(horizon_core::nbt::Tag::IntArray(pos)) => Ok(pos.clone()),
                            _ => Err("frame map: invalid saved position"),
                        })
                        .transpose()?;
                    let rotation = record
                        .map(|r| myriade_engine::save::number(r.get("rotation")).ok_or("frame map: missing rotation"))
                        .transpose()?;
                    Ok(json!({"id": map_id, "uuid": actual, "kind": frame.kind(), "pos": frame.pos,
                        "center": map.center, "scale": map.scale, "dimension": map.dimension,
                        "saved": record.is_some(), "saved_pos": pos, "saved_rotation": rotation,
                        "decoration": map.decorations.iter().any(|d| d.key == crate::game::maps::Key::Frame(id as i32)),
                        "frame_records": records.len(),
                        "frame_decorations": map.decorations.iter().filter(|d| d.sprite == "map/decorations/frame").count()}))
                })?;
                json!({"cmd": format!("#frame_map {label}"), "messages": [], "results": [], "state": state})
            }
            "map_carried" => {
                self.op_as_guest(conn, |g| {
                    g.tick_maps();
                    Ok(())
                })?;
                Value::Null
            }
            "craft_menu_click" | "craft_menu_state" | "craft_menu_close" => {
                let op_name = op["op"].as_str().unwrap_or("");
                let state = self.op_as_guest(conn, |g| -> Result<Value> {
                    let host = g.lan.as_ref().ok_or("no hub")?;
                    let (window, revision) = host
                        .menus
                        .get(&conn)
                        .and_then(|p| p.crafting.table.as_ref())
                        .map(|(window, revision, _, _)| (*window, *revision))
                        .ok_or("craft fixture: crafting menu not open")?;
                    if op_name == "craft_menu_click" {
                        // Revision last sent by the real table-content sync, never a fixed 0.
                        g.lan_guest_packet(
                            conn,
                            horizon_protocol::packets::play::Serverbound::ContainerClick(
                                horizon_protocol::packets::play::ContainerClick {
                                    container: window,
                                    state: revision,
                                    slot: op["slot"].as_i64().ok_or("click slot")? as i16,
                                    button: op["button"].as_i64().unwrap_or(0) as i8,
                                    input: op["input"].as_i64().unwrap_or(0) as i32,
                                    changed: Vec::new(),
                                    carried: None,
                                },
                            ),
                        );
                        return Ok(Value::Null);
                    }
                    if op_name == "craft_menu_close" {
                        g.lan_guest_packet(
                            conn,
                            horizon_protocol::packets::play::Serverbound::ContainerClose(
                                horizon_protocol::packets::play::ContainerClose { container: window },
                            ),
                        );
                        return Ok(Value::Null);
                    }
                    let (_, _, menu, _) = host
                        .menus
                        .get(&conn)
                        .and_then(|p| p.crafting.table.as_ref())
                        .ok_or("craft fixture: crafting menu not open")?;
                    if menu.kind() != myriade_engine::menu::Kind::Crafting {
                        return Err("craft fixture: wrong menu kind".into());
                    }
                    let guest = host.hub.guest(conn).ok_or("no guest")?;
                    let stack = |s: Option<&myriade_engine::items::ItemStack>| {
                        s.map(|s| json!([&*s.item, s.count, s.data().and_then(|d| d.map_id), s.data().and_then(|d| d.map_post_processing)]))
                    };
                    // The table's ten slots plus the guest's live inventory in menu order.
                    let slots: Vec<_> = (0..10)
                        .map(|i| stack(menu.slot(i)))
                        .chain((9..36).chain(0..9).map(|i| stack(guest.inventory[i].as_ref())))
                        .collect();
                    Ok(json!({"window": window, "kind": "minecraft:crafting", "slots": slots,
                        "carried": stack(guest.carried.as_ref()).unwrap_or_else(|| json!([]))}))
                })?;
                if op_name != "craft_menu_state" {
                    return Ok(Value::Null);
                }
                json!({"cmd": format!("#craft_menu {label}"), "state": state, "messages": [], "results": []})
            }
            "close_menu" => {
                let (_, _, window) = self.op_menu(conn)?;
                self.op_as_guest(conn, |g| {
                    g.lan_guest_packet(
                        conn,
                        horizon_protocol::packets::play::Serverbound::ContainerClose(
                            horizon_protocol::packets::play::ContainerClose { container: window },
                        ),
                    );
                    Ok(())
                })?;
                Value::Null
            }
            "menu_state" => {
                let (_, own_slots, _) = self.op_menu(conn)?;
                json!({"cmd": format!("#menu_state {label}"), "messages": [], "results": [], "state": {"own_slots": own_slots}})
            }
            "map_files" => {
                let state = self.op_as_guest(conn, |g| {
                    use horizon_core::nbt::Tag;
                    g.save_maps();
                    let directory = g.world_root.join("data/minecraft/maps");
                    let maps: Vec<_> = op["ids"]
                        .as_array()
                        .ok_or("map IDs")?
                        .iter()
                        .map(|id| {
                            let path = directory.join(format!("{}.dat", id.as_i64().unwrap()));
                            myriade_engine::save::read_dat(&path).map(|root| {
                                let data = root.get("data").unwrap_or(&root);
                                let number = |key| match data.get(key) {
                                    Some(Tag::Int(v)) => *v,
                                    Some(Tag::Byte(v)) => i32::from(*v),
                                    _ => 0,
                                };
                                let colors = match data.get("colors") {
                                    Some(Tag::ByteArray(bytes)) => Some(bytes.len()),
                                    _ => None,
                                };
                                json!({"scale": number("scale"), "locked": number("locked") != 0,
                                    "center": [number("xCenter"), number("zCenter")],
                                    "dimension": myriade_engine::save::text(data.get("dimension")),
                                    "byte_array_length": colors})
                            })
                        })
                        .collect();
                    let last = myriade_engine::save::read_dat(&directory.join("last_id.dat")).and_then(|tag| {
                        match tag.get("data").and_then(|d| d.get("map")) {
                            Some(Tag::Int(v)) => Some(*v),
                            _ => None,
                        }
                    });
                    Ok(json!({"maps": maps, "last_id": last}))
                })?;
                json!({"cmd": "#map_files", "state": state, "messages": [], "results": []})
            }
            "map_state" => {
                let id = op["id"].as_i64().ok_or("command harness: map id")? as i32;
                let value = self.op_as_guest(conn, |g| {
                    Ok(match g.map(id) {
                        Some(m) => {
                            let mut value = format!("scale={},locked={},center={},{}", m.scale, m.locked, m.center[0], m.center[1]);
                            if op["red_x"].as_bool().unwrap_or(false) {
                                let count = m.decorations.iter().filter(|d| d.sprite == "map/decorations/red_x").count();
                                value.push_str(&format!(",red_x={count}"));
                            }
                            value
                        }
                        None => "missing".to_owned(),
                    })
                })?;
                json!({"cmd": format!("#map_state {id}"), "messages": [spans(&Text::literal(value))], "results": []})
            }
            "light" => {
                let pos: [i32; 3] = std::array::from_fn(|i| op["pos"][i].as_i64().unwrap_or(0) as i32);
                let levels = self.op_as_guest(conn, |g| {
                    use myriade_engine::mobs::spawn_rules::SpawnLevel;
                    let level = g.natural_level();
                    Ok([level.block_light(pos[0], pos[1], pos[2]), level.sky_light(pos[0], pos[1], pos[2])])
                })?;
                json!({"cmd": format!("#light {}", op["pos"]), "messages": [], "results": levels.map(|v| (true, v))})
            }
            "merchant_save" => {
                self.op_as_guest(conn, |g| g.save_game())?;
                Value::Null
            }
            "merchant_saved_stats" => {
                let (state, raw) = self.op_as_guest(conn, |g| {
                    g.load_stats();
                    if !g.stats.loaded {
                        return Err("merchant stats not loaded".into());
                    }
                    g.save_stats();
                    let uuid = g.level.player_uuid.ok_or("merchant stats UUID")?;
                    let path = crate::game::stats::player_json(&g.world_root, "stats", uuid);
                    let raw = std::fs::read_to_string(path)?;
                    let saved: Value = serde_json::from_str(&raw)?;
                    let value = |ty: &str, key: &str| saved["stats"][ty][key].as_i64().unwrap_or(0);
                    Ok((json!({"uuid": uuid, "books": value("minecraft:crafted", "minecraft:bookshelf"),
                        "glass": value("minecraft:crafted", "minecraft:glass"),
                        "trades": value("minecraft:custom", "minecraft:traded_with_villager")}), raw))
                })?;
                // The raw file lands next to the run's output, named by the label, as the model wrote it.
                let output = op["output"].as_str().ok_or("merchant stats: missing output path")?;
                std::fs::write(
                    std::path::Path::new(output).with_file_name(format!("native-merchant-stats-{label}.json")),
                    raw,
                )?;
                json!({"cmd": format!("#merchant_saved_stats {label}"), "messages": [], "results": [], "state": state})
            }
            "merchant_action" | "merchant_state" => {
                let observe = op["op"] == "merchant_state";
                let state = self.op_as_guest(conn, |g| -> Result<Value> {
                    let host = g.lan.as_ref().ok_or("merchant oracle: no hub")?;
                    if observe {
                        return Ok(g.harness_merchant(host, conn).ok_or("merchant oracle: no live menu")?.2);
                    }
                    use horizon_protocol::packets::play::{self, Serverbound as S};
                    let packet = match op["action"].as_str().unwrap_or("") {
                        "open" => {
                            let uuid = uuid_of(&op["uuid"])?;
                            let Some(crate::game::commands::Target::Mob(id)) = g.target_by_uuid(uuid) else {
                                return Err("merchant oracle: missing live mob".into());
                            };
                            S::Interact(play::Interact { id: id as i32, hand: 0, location: [0.0; 3], secondary: false })
                        }
                        "select" => S::SelectTrade(play::SelectTrade { item: op["index"].as_i64().unwrap_or(0) as i32 }),
                        "click" => {
                            let (container, state, _) = g.harness_merchant(host, conn).ok_or("merchant oracle: no menu")?;
                            S::ContainerClick(play::ContainerClick {
                                container,
                                state,
                                slot: op["slot"].as_i64().ok_or("merchant oracle: slot")? as i16,
                                button: op["button"].as_i64().unwrap_or(0) as i8,
                                input: op["input"].as_i64().unwrap_or(0) as i32,
                                changed: Vec::new(),
                                carried: None,
                            })
                        }
                        "close" => S::ContainerClose(play::ContainerClose {
                            container: g.harness_merchant(host, conn).ok_or("merchant oracle: no menu")?.0,
                        }),
                        _ => return Err("merchant oracle: unknown action".into()),
                    };
                    g.lan_guest_packet(conn, packet);
                    Ok(Value::Null)
                })?;
                if !observe {
                    return Ok(Value::Null);
                }
                let mut state = state;
                if !op["offer_packets"].as_bool().unwrap_or(false) {
                    state.as_object_mut().ok_or("merchant state")?.remove("sent_offers");
                    for offer in state["offers"].as_array_mut().ok_or("merchant offers")? {
                        offer.as_object_mut().ok_or("merchant offer")?.remove("demand");
                    }
                }
                json!({"cmd": format!("#merchant_state {label}"), "messages": [], "results": [], "state": state})
            }
            "sync_randoms" => {
                // docs/parity-workflow.md "Randoms that repeat": the uniquifier is the thread's own.
                let field = |key: &str| op[key].as_i64().ok_or_else(|| format!("sync_randoms: no {key}"));
                let (uniquifier, entity_id, level_seed) = (field("uniquifier")?, field("entity_id")?, field("level_seed")?);
                myriade_engine::random_source::set_thread_uniquifier(uniquifier)?;
                self.op_as_guest(conn, |g| {
                    g.next_entity = u32::try_from(entity_id)?;
                    horizon_core::random::RandomSource::set_seed(&mut g.world.random, level_seed);
                    Ok(())
                })?;
                Value::Null
            }
            "crafter_tick" => {
                let [x, y, z] = std::array::from_fn(|i| op["pos"][i].as_i64().unwrap_or(0) as i32);
                self.op_as_guest(conn, |g| {
                    let state = g.world.block(x, y, z);
                    let name = &*g.res.facts().get(state).name;
                    if name != "minecraft:crafter" && name != "minecraft:dispenser" {
                        return Err("command harness: crafter tick without a crafter or dispenser".into());
                    }
                    g.dispense_from(horizon_core::position::BlockPos::new(x, y, z), state)
                })?;
                Value::Null
            }
            "boat_interact" => {
                let uuid = uuid_of(&op["uuid"])?;
                self.op_as_guest(conn, |g| -> Result<()> {
                    let Some(crate::game::commands::Target::Boat(id)) = g.target_by_uuid(uuid) else {
                        return Err("boat click: missing live boat".into());
                    };
                    g.lan_guest_packet(
                        conn,
                        horizon_protocol::packets::play::Serverbound::Interact(horizon_protocol::packets::play::Interact {
                            id: id as i32,
                            hand: 0,
                            location: [0.0; 3],
                            secondary: false,
                        }),
                    );
                    Ok(())
                })?;
                Value::Null
            }
            "mob_interact" => {
                let uuid = uuid_of(&op["uuid"])?;
                self.op_as_guest(conn, |g| -> Result<()> {
                    let id = g
                        .mob_by_uuid(myriade_engine::lan::playerdata::uuid_ints(uuid))
                        .filter(|id| g.ecs.alive(*id))
                        .ok_or("mob click: missing live mob")?;
                    g.lan_guest_packet(
                        conn,
                        horizon_protocol::packets::play::Serverbound::Interact(horizon_protocol::packets::play::Interact {
                            id: id as i32,
                            hand: 0,
                            location: [0.0; 3],
                            secondary: false,
                        }),
                    );
                    Ok(())
                })?;
                Value::Null
            }
            "player_command" => {
                let action = match op["action"].as_str() {
                    Some("start_riding_jump") => 3,
                    Some("stop_riding_jump") => 4,
                    _ => return Err("player command: unknown action".into()),
                };
                let data = op["data"].as_i64().ok_or("player command: missing data")? as i32;
                self.op_as_guest(conn, |g| -> Result<()> {
                    let id = g
                        .lan
                        .as_ref()
                        .and_then(|h| h.hub.guest(conn))
                        .map(|p| p.entity_id)
                        .ok_or("player command: player not connected")?;
                    g.lan_guest_packet(
                        conn,
                        horizon_protocol::packets::play::Serverbound::PlayerCommand(
                            horizon_protocol::packets::play::PlayerCommand { id, action, data },
                        ),
                    );
                    Ok(())
                })?;
                Value::Null
            }
            "dragon_contacts" => {
                // The seven cases of VanillaDragonContactsOracle: the target at (1,0,0), the rule of the case on a list of one.
                use horizon_ecs_proto::dragon::phases::HOLDING;
                let mut rows = Vec::new();
                for name in ["flying", "sitting", "two_wings", "team", "head", "recent_victim"] {
                    let dragon = self.ecs_spawn("minecraft:ender_dragon", [0.0; 3], None).ok_or("dragon_contacts: no dragon")?;
                    let phase = if name == "sitting" { horizon_ecs_proto::species::ender_dragon::START_PHASE } else { HOLDING };
                    self.ecs_with(|ecs, mw| ecs.set_dragon_phase(mw, dragon, phase as i32));
                    let mob = self.ecs_spawn("minecraft:enderman", [1.0, 0.0, 0.0], None).ok_or("dragon_contacts: no enderman")?;
                    self.ecs.set_no_ai(mob, true);
                    let team = name == "team";
                    if team {
                        let (uuid_d, uuid_m) = ([0, 0, 0, dragon as i32], [0, 0, 0, mob as i32]);
                        self.persist.uuids.insert(dragon, uuid_d);
                        self.persist.uuids.insert(mob, uuid_m);
                        let members = [self.scoreboard_name(dragon, uuid_d), self.scoreboard_name(mob, uuid_m)];
                        let board_team = self.score.board.add_team("contact");
                        board_team.allow_friendly_fire = false;
                        for m in &members {
                            board_team.players.insert(m);
                        }
                        self.push_team_event("contact");
                        self.ecs.set_team_friendly_fire("contact", false);
                    }
                    if name == "recent_victim" {
                        let now = self.ecs.ecs_mut().map_or(0, |w| w.world().resource::<horizon_ecs_proto::cow::Clock>().0);
                        let by = self.ecs.entity_of(dragon).map(horizon_ecs_proto::targeting::Who::Entity).ok_or("dragon_contacts: no dragon entity")?;
                        self.ecs.set_last_hurt_by(mob, by, now.saturating_sub(1));
                    }
                    let who = [horizon_ecs_proto::targeting::Who::Entity(self.ecs.entity_of(mob).ok_or("dragon_contacts: no enderman entity")?)];
                    match name {
                        "head" => self.ecs.dragon_head(dragon, &who),
                        "two_wings" => {
                            self.ecs.dragon_wing(dragon, &who);
                            self.ecs.dragon_wing(dragon, &who);
                        }
                        _ => self.ecs.dragon_wing(dragon, &who),
                    }
                    let [vx, vy, vz] = self.ecs_mob_velocity(mob).map_or([0.0; 3], |v| [v.x, v.y, v.z]);
                    let health = self.ecs_living_health(mob).unwrap_or(0.0);
                    let revenge = matches!(self.ecs.last_hurt_by(mob), Some(crate::mobs::Who::Mob(d)) if d == dragon);
                    rows.push(json!({"case": name, "health": health, "velocity": [vx, vy, vz], "revenge": revenge}));
                    if team {
                        self.score.board.remove_team("contact");
                        self.push_team_event("contact");
                        self.ecs.set_team_friendly_fire("contact", true);
                    }
                    for id in [mob, dragon] {
                        if let Some(e) = self.ecs.entity_of(id) {
                            self.ecs.despawn(e);
                        }
                    }
                }
                Value::Array(rows)
            }
            "villager_farming" => {
                // The class and maturity of every state of the nine blocks of VanillaVillagerFarmingOracle, in its order.
                use horizon_ecs_proto::rules::farm::{crop_max_age, is_ripe};
                use horizon_world::registry::BlockStateId;
                // `blocks` names the states to read (a mod's blocks too); the default is the oracle's nine vanilla blocks.
                let blocks: Vec<String> = match op.get("blocks").and_then(Value::as_array) {
                    Some(list) => list.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect(),
                    None => ["wheat", "carrots", "potatoes", "beetroots", "torchflower_crop", "pitcher_crop", "cocoa", "farmland", "stone"]
                        .map(|b| format!("minecraft:{b}"))
                        .to_vec(),
                };
                self.ecs_with(|_, mw| {
                    let mut rows = Vec::new();
                    for name in blocks {
                        for i in 0..mw.registry.len() {
                            let id = BlockStateId(i as u32);
                            let Some(state) = mw.registry.state(id) else { continue };
                            if state.name != name {
                                continue;
                            }
                            let facts = mw.facts.get(id);
                            let crop = facts.has_class("CropBlock");
                            let max = crop_max_age(crop, facts.has_class("BeetrootBlock"), facts.has_class("TorchflowerCropBlock"));
                            let age = state.properties.get("age").and_then(|v| v.parse::<i32>().ok());
                            let ripe = crop && is_ripe(max, age);
                            let farmland = facts.has_class("FarmlandBlock");
                            rows.push(Value::String(format!("{i} {crop} {ripe} {farmland}")));
                        }
                    }
                    Value::Array(rows)
                })
            }
            "happy_ghast_leash" => self.op_happy_ghast_leash(op)?,
            "shulker_space" => self.op_shulker_space(op)?,
            "mob_breathing" => self.op_mob_breathing(op)?,
            "mob_fall_reset" => self.op_mob_fall_reset(op)?,
            "villager_witness" => self.op_villager_witness(op)?,
            "natural_spawn" => self.op_natural_spawn(op)?,
            "entity_inventory" => {
                // The stacks of the entity whose UUID is `uuid` (a villager's inventory, read through the ECS bridge).
                let uuid = uuid_of(&op["uuid"])?;
                let Some(crate::game::commands::Target::Mob(id)) = self.target_by_uuid(uuid) else {
                    return Err("entity inventory: no entity with that UUID".into());
                };
                Value::Array(self.ecs.inventory_stacks(id).iter().map(|s| json!({"item": &*s.item, "count": s.count})).collect())
            }
            other => match self.op_fixture(other, op)? {
                Some(v) => v,
                None => return Err(format!("op: unknown operation {other}").into()),
            },
        })
    }
}
