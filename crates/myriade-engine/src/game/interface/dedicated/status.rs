//! What `horizon-server` asks the dedicated server on its control endpoint (`--control-socket`, docs/server.md
//! section 2): `server-status` answers with the tick rate, the tick times `/tick query` reports, the players
//! (including their positions) and the loaded chunks; `console <line>` runs a console command and answers with the lines it logged;
//! `server-events <since>` answers the typed events after that number (events.rs); `management <connection>
//! <json>` edits the lists as vanilla's management protocol does (management.rs).
//! `run_as <player> <command>` runs a command as a connected player and answers its result, success and the
//! plain text of its messages; `block x y z`, `blocks x1 y1 z1 x2 y2 z2`, `entities_server`, `state <key>`,
//! `save` and `restart` read or act on the server's own state (the proxy's and the test tools' control socket);
//! `op <player> <json>` runs one operation of the command harness as that player and answers its record (command_ops.rs).
//! `player-expect <uuid> [seconds]`, `player-drop <uuid>`, `player-export <uuid> [freeze]`,
//! `player-release <uuid> ...`, `player-import <uuid> <json>` and `player-confirm <uuid> ...` are a proxy's,
//! for a player it moves between two servers, and `transfer-status` the counts it reads first (transfer.rs).
use super::{ServerState, capture_log};
use crate::game::{Game, lan_actions::GuestCommand};
use serde_json::{Value, json};
use std::{
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

/// The window the tick rate is measured over.
const TPS_WINDOW: Duration = Duration::from_secs(5);
/// The most blocks one `blocks` request reads.
const BOX_LIMIT: i64 = 4096;

impl ServerState {
    /// One pass of the server loop ended.
    pub(super) fn ticked(&mut self, at: Instant) {
        self.ticks.push_back(at);
        while self
            .ticks
            .front()
            .is_some_and(|t| at.duration_since(*t) > TPS_WINDOW)
        {
            self.ticks.pop_front();
        }
    }
    /// Ticks per second over the last few seconds.
    pub(super) fn tps(&self) -> f64 {
        match (self.ticks.front(), self.ticks.back()) {
            (Some(first), Some(last)) if self.ticks.len() > 1 => {
                (self.ticks.len() - 1) as f64 / last.duration_since(*first).as_secs_f64().max(1e-9)
            }
            _ => 0.0,
        }
    }
}

impl Game<'_> {
    /// Answers the requests waiting on the control endpoint.
    pub(super) fn poll_server_control(&mut self, state: &mut ServerState) {
        while let Some(request) = self.control.as_ref().and_then(|c| c.try_recv()) {
            // A `console stop` just before stops this server for the next request too.
            self.note_stopping(state.lists.lock().unwrap().stop);
            let reply = match request.line.trim().split_once(' ') {
                _ if request.line.trim() == "server-status" => self.server_status(state),
                // `the control socket's memory command`: what holds the server's memory (memory.rs).
                _ if request.line.trim() == "memory" => self.memory_value(),
                // A proxy reading whether it may send players here (transfer.rs).
                _ if request.line.trim() == "transfer-status" => self.transfer_status(),
                // The control socket's reads of the server's own state (`block`, `blocks`, `state`, `entities_server`).
                _ if request.line.trim() == "entities_server" => self.entities_server(),
                _ if request.line.trim() == "save" => self.save_reply(),
                // A stop, as `console stop` makes it: the relaunch is the supervisor's (not in this crate).
                _ if request.line.trim() == "restart" => {
                    state.lists.lock().unwrap().stop = true;
                    json!({ "stopping": true })
                }
                Some(("run_as", rest)) => self.run_as(rest),
                Some(("block", rest)) => self.block_reply(rest),
                Some(("blocks", rest)) => self.blocks_reply(rest),
                Some(("state", key)) => self.state_reply(key),
                // One operation of the command harness, as the named player (command_ops.rs).
                Some(("op", rest)) => self.op_reply(rest),
                Some(("server-events", since)) => {
                    super::events::since(since.trim().parse().unwrap_or(0))
                }
                Some(("management", rest)) => self.management(state, rest),
                // A proxy moving a player here or away (transfer.rs, lan/arrivals.rs).
                Some((verb, rest)) if verb.starts_with("player-") => {
                    self.player_request(verb, rest)
                }
                Some(("console", line)) => {
                    // Command feedback is logged as system chat; the reply carries the text alone.
                    let output: Vec<String> = capture_log(|| self.console_command(state, line))
                        .into_iter()
                        .map(|l| {
                            l.strip_prefix("System chat: ")
                                .map_or(l.clone(), str::to_owned)
                        })
                        .collect();
                    json!({ "output": output })
                }
                // An RCON command: run as the console, with the `Rcon` source, whose feedback the
                // operators are told of when `broadcast-rcon-to-ops` is on.
                Some(("rcon", line)) => {
                    let output: Vec<String> = capture_log(|| self.rcon_line(state, line))
                        .into_iter()
                        .map(|l| {
                            l.strip_prefix("System chat: ")
                                .map_or(l.clone(), str::to_owned)
                        })
                        .collect();
                    json!({ "output": output })
                }
                _ => json!({ "error": format!("unknown request {:?}", request.line) }),
            };
            let _ = request.reply.send(reply.to_string());
        }
    }

    /// Each level's loaded chunks and entities (players and block entities apart), the level in place
    /// first: what the Prometheus text reports by dimension.
    fn status_levels(&self) -> Value {
        let mut levels = vec![json!({
            "dimension": self.dim.id(),
            "chunks": self.world.chunks.len(),
            "entities": self.entity_count(),
        })];
        for dim in self.levels.parked_dims() {
            if let (Some(world), Some(entities)) = (
                self.levels.parked_world(dim),
                self.levels.parked_entity_count(dim),
            ) {
                levels.push(json!({
                    "dimension": dim.id(),
                    "chunks": world.chunks.len(),
                    "entities": entities,
                }));
            }
        }
        Value::Array(levels)
    }

    /// The entities of the level in place: mobs, items, orbs, projectiles, vehicles, falling blocks,
    /// hangings, armor stands, cushions, tridents, rockets and primed TNT.
    fn entity_count(&self) -> usize {
        self.ecs.entity_count()
            + self.ecs.item_ids().len()
            + self.ecs.cart_ids().len()
            + self.ecs.boat_ids().len()
            + self.ecs.species_count("minecraft:falling_block")
            + self.ecs_paintings().len()
            + self.ecs_frames().len()
            + self.armor_stand_list().len()
            + self.cushion_ids().len()
            + self.ecs.species_count("minecraft:trident")
            + self.ecs.species_count("minecraft:firework_rocket")
            + self.ecs.species_count("minecraft:tnt")
    }

    fn server_status(&self, state: &ServerState) -> Value {
        let millis = |nanos: u64| nanos as f64 / 1e6;
        let samples = self.clock.sorted_samples();
        // The percentiles `/tick query` prints (TickCommand.percentileToString).
        let at = |q: f64| {
            let i = ((samples.len() as f64 * q) as usize).min(samples.len().saturating_sub(1));
            samples.get(i).copied().map_or(0.0, millis)
        };
        let (players, max_players) = self.lan.as_ref().map_or((Vec::new(), 0), |h| {
            let players = h
                .hub
                .guests
                .iter()
                .filter(|g| g.participates_online())
                .map(|g| {
                    player_status(
                        &g.name,
                        g.uuid,
                        &g.address.ip().to_string(),
                        g.latency.load(Ordering::Relaxed),
                        &g.dimension,
                        g.position,
                    )
                })
                .collect();
            (players, h.hub.lan.shared.max_players())
        });
        json!({
            "tps": state.tps(),
            "target_tps": self.clock.rate(),
            "mspt": {
                "average": millis(self.clock.average_nanos()),
                "p50": at(0.5),
                "p95": at(0.95),
                "p99": at(0.99),
            },
            "players": players,
            "max_players": max_players,
            "chunks": self.world.chunks.len(),
            "levels": self.status_levels(),
            "paused": state.paused,
        })
    }

    /// The block state at `x y z`: its name and properties.
    fn block_value(&self, x: i32, y: i32, z: i32) -> Value {
        let Some(state) = self.pack.assets.registry.state(self.world.block(x, y, z)) else {
            return json!({ "error": "unknown block state" });
        };
        let properties: serde_json::Map<String, Value> = state
            .properties
            .iter()
            .map(|(k, v)| (k.to_string(), json!(v.to_string())))
            .collect();
        json!({ "x": x, "y": y, "z": z, "id": state.name, "properties": properties })
    }

    fn block_reply(&self, rest: &str) -> Value {
        match coords::<3>(rest) {
            Some([x, y, z]) => self.block_value(x, y, z),
            None => json!({ "error": "block takes x y z" }),
        }
    }

    fn blocks_reply(&self, rest: &str) -> Value {
        let Some([x1, y1, z1, x2, y2, z2]) = coords::<6>(rest) else {
            return json!({ "error": "blocks takes x1 y1 z1 x2 y2 z2" });
        };
        let Some((lo, hi)) = box_corners([x1, y1, z1], [x2, y2, z2]) else {
            return json!({ "error": format!("blocks: more than {BOX_LIMIT} blocks") });
        };
        let mut blocks = Vec::new();
        for x in lo[0]..=hi[0] {
            for y in lo[1]..=hi[1] {
                for z in lo[2]..=hi[2] {
                    blocks.push(self.block_value(x, y, z));
                }
            }
        }
        json!({ "blocks": blocks })
    }

    /// The entities of the ECS (id, type, position) and the players online, for the level in place.
    fn entities_server(&self) -> Value {
        let entities: Vec<Value> = self
            .ecs
            .all_ids()
            .into_iter()
            .filter_map(|id| {
                Some(json!({ "id": id, "type": self.ecs.species(id)?, "pos": self.ecs.position(id)? }))
            })
            .collect();
        let players: Vec<Value> = self.lan.as_ref().map_or(Vec::new(), |h| {
            h.hub
                .guests
                .iter()
                .filter(|g| g.participates_online())
                .map(|g| {
                    json!({
                        "name": g.name,
                        "uuid": horizon_protocol::crypto::uuid_string(g.uuid),
                        "pos": g.position,
                    })
                })
                .collect()
        });
        json!({ "entities": entities, "players": players })
    }

    /// `run_as <player> <command>`: the command run as that connected player (position, rights, source),
    /// with the result, the success and the plain text of each message sent to the source.
    fn run_as(&mut self, rest: &str) -> Value {
        let Some((name, command)) = rest.trim().split_once(' ') else {
            return json!({ "error": "run_as takes a player and a command" });
        };
        let guest = self.lan.as_ref().and_then(|h| {
            h.hub
                .guests
                .iter()
                .find(|g| g.name == name)
                .map(|g| (g.conn, g.permission))
        });
        let Some((conn, permission)) = guest else {
            return json!({ "error": format!("no player {name}") });
        };
        let command = GuestCommand {
            conn,
            command: command.to_owned(),
            permission,
            quiet: false,
        };
        match self.run_guest_result(command) {
            Some((result, success, lines)) => {
                let messages: Vec<String> = lines.iter().map(|l| l.plain()).collect();
                run_as_reply(result, success, &messages)
            }
            None => json!({ "error": format!("no player {name}") }),
        }
    }

    /// `save`: the world written as `/save-all` writes it.
    fn save_reply(&mut self) -> Value {
        match self.save_game() {
            Ok(()) => json!({ "saved": true }),
            Err(e) => json!({ "saved": false, "error": format!("{e}") }),
        }
    }

    /// `state <key>`: raid ids (`raid`), rain and thunder levels (`meteo`), sky and block light at `x y z`
    /// (`lumiere x y z`), the mobs angry at a target (`mob_anger`), a merchant's offers (`marchand <id>`),
    /// a map's data, loaded or saved (`cartes <id>`).
    fn state_reply(&self, rest: &str) -> Value {
        let mut words = rest.split_whitespace();
        match words.next() {
            Some("raid") => json!({ "raids": self.raids_in_place().map_or(Vec::new(), |r| r.ids()) }),
            Some("meteo") => {
                let (rain, thunder) = self.weather_levels(1.0);
                json!({ "rain": rain, "thunder": thunder })
            }
            Some("lumiere") => match coords::<3>(&words.collect::<Vec<_>>().join(" ")) {
                Some([x, y, z]) => {
                    let light = i32::from(self.world.light(x, y, z));
                    json!({ "sky": light >> 4, "block": light & 15 })
                }
                None => json!({ "error": "state lumiere takes x y z" }),
            },
            Some("mob_anger") => {
                let angry: Vec<Value> = self
                    .ecs
                    .all_ids()
                    .into_iter()
                    .filter(|&id| self.ecs.is_angry(id))
                    .map(|id| {
                        json!({ "id": id, "type": self.ecs.species(id), "target": self.ecs.ai_target(id).is_some() })
                    })
                    .collect();
                json!({ "mobs": angry })
            }
            Some("marchand") => match words.next().and_then(|id| id.parse::<u32>().ok()) {
                Some(id) => match self.ecs.trade_offers(id) {
                    Some(offers) => json!({ "offers": offers.iter().map(|o| json!({
                        "result": o.result.item.to_string(),
                        "count": o.result.count,
                        "uses": o.uses,
                        "max_uses": o.max_uses,
                        "xp": o.xp,
                    })).collect::<Vec<_>>() }),
                    None => json!({ "error": format!("no merchant {id}") }),
                },
                None => json!({ "error": "state marchand takes an entity id" }),
            },
            Some("cartes") => match words.next().and_then(|id| id.parse::<i32>().ok()) {
                Some(id) => self
                    .with_map(id, |m| {
                        json!({ "scale": m.scale, "dimension": m.dimension, "locked": m.locked, "center": m.center, "colors": m.colors.len() })
                    })
                    .unwrap_or_else(|| json!({ "error": format!("no map {id}") })),
                None => json!({ "error": "state cartes takes a map id" }),
            },
            _ => json!({ "error": format!("unknown state {rest:?}") }),
        }
    }
}

/// The `run_as` reply: the command's result, whether it succeeded, and the plain text of its messages.
fn run_as_reply(result: i32, success: bool, messages: &[String]) -> Value {
    json!({ "result": result, "success": success, "messages": messages })
}

/// The integer arguments of a request, `None` unless there are exactly `N`.
fn coords<const N: usize>(rest: &str) -> Option<[i32; N]> {
    let values: Vec<i32> = rest
        .split_whitespace()
        .map(|s| s.parse().ok())
        .collect::<Option<_>>()?;
    values.try_into().ok()
}

/// The low and high corners of the box between two block positions; `None` above `BOX_LIMIT` blocks.
fn box_corners(a: [i32; 3], b: [i32; 3]) -> Option<([i32; 3], [i32; 3])> {
    let lo = [a[0].min(b[0]), a[1].min(b[1]), a[2].min(b[2])];
    let hi = [a[0].max(b[0]), a[1].max(b[1]), a[2].max(b[2])];
    let volume: i64 = (0..3).map(|i| i64::from(hi[i]) - i64::from(lo[i]) + 1).product();
    (volume <= BOX_LIMIT).then_some((lo, hi))
}

fn player_status(
    name: &str,
    uuid: u128,
    address: &str,
    ping_ms: i32,
    dimension: &str,
    position: [f64; 3],
) -> Value {
    json!({
        "name": name,
        "uuid": horizon_protocol::crypto::uuid_string(uuid),
        "address": address,
        "ping_ms": ping_ms,
        "dimension": dimension,
        "position": position,
    })
}

#[cfg(test)]
mod tests {
    use super::{BOX_LIMIT, box_corners, coords, player_status, run_as_reply};
    use serde_json::json;

    #[test]
    fn run_as_reply_keeps_the_result_success_and_messages() {
        assert_eq!(
            run_as_reply(3, true, &["Set".to_owned()]),
            json!({"result": 3, "success": true, "messages": ["Set"]})
        );
        assert_eq!(
            run_as_reply(0, false, &[]),
            json!({"result": 0, "success": false, "messages": []})
        );
    }

    #[test]
    fn coords_take_exactly_the_asked_integers() {
        assert_eq!(coords::<3>("1 -64 3"), Some([1, -64, 3]));
        assert_eq!(coords::<3>("1 2"), None);
        assert_eq!(coords::<3>("1 2 x"), None);
    }

    #[test]
    fn box_corners_order_the_corners_and_bound_the_volume() {
        assert_eq!(box_corners([3, 0, -1], [1, 2, 0]), Some(([1, 0, -1], [3, 2, 0])));
        assert_eq!(box_corners([0, 0, 0], [15, 15, 15]), Some(([0, 0, 0], [15, 15, 15])));
        assert_eq!(box_corners([0, 0, 0], [15, 15, 16]), None);
        assert_eq!(BOX_LIMIT, 4096);
    }

    #[test]
    fn player_status_includes_coordinates() {
        assert_eq!(
            player_status(
                "Alex",
                1,
                "203.0.113.7",
                42,
                "minecraft:the_nether",
                [1.5, 64.0, -2.25]
            ),
            json!({
                "name": "Alex",
                "uuid": "00000000-0000-0000-0000-000000000001",
                "address": "203.0.113.7",
                "ping_ms": 42,
                "dimension": "minecraft:the_nether",
                "position": [1.5, 64.0, -2.25],
            })
        );
    }
}
