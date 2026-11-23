//! The verbs of the connection and of what the bot knows: join, leave, chat, command, waits, state, messages, entities, report.
use crate::{
    online::{self, Account},
    session::{self, Login, Session},
    state::meta_json,
};
use horizon_protocol::{
    crypto::{offline_uuid, uuid_string},
    packets::play::{self, Serverbound as S},
};
use serde_json::{Value, json};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub fn str_arg(args: &Value, key: &str) -> Result<String, String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("missing string argument: {key}"))
}

pub fn num_arg(args: &Value, key: &str) -> Result<f64, String> {
    args.get(key)
        .and_then(Value::as_f64)
        .ok_or_else(|| format!("missing number argument: {key}"))
}

pub fn pos_json(p: [f64; 3]) -> Value {
    json!([p[0], p[1], p[2]])
}

/// A chat line, unsigned: the server accepts it in offline mode.
fn chat_packet(message: String) -> S {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64);
    S::Chat(play::Chat {
        message,
        timestamp,
        salt: 0,
        signature: None,
        last_seen: play::LastSeenUpdate {
            offset: 0,
            acknowledged: [0; 3],
            checksum: 0,
        },
    })
}

/// Logs in and waits for the spawn position. Returns the session and the reply.
pub fn verb_join(args: &Value) -> Result<(Session, Value), String> {
    let host = str_arg(args, "host")?;
    let port = num_arg(args, "port")? as u16;
    let name = str_arg(args, "name")?;
    let view = args.get("render_distance").and_then(Value::as_i64).unwrap_or(4) as i8;
    let session_url = args
        .get("session_url")
        .and_then(Value::as_str)
        .unwrap_or("https://sessionserver.mojang.com")
        .to_string();
    let account: Option<Account> = match args.get("session_file").and_then(Value::as_str) {
        Some(path) => Some(online::read_account(path)?),
        None => None,
    };
    let registry = match args.get("registries").and_then(Value::as_str) {
        Some(path) => Some(horizon_protocol::registry::Builtin::load(std::path::Path::new(path)).map_err(|e| e.to_string())?),
        None => None,
    };
    let uuid = account.as_ref().map_or_else(|| offline_uuid(&name), |a| a.uuid);
    let login = Login {
        name: name.clone(),
        uuid,
        account,
        session_url,
    };
    let conn = session::connect(&host, port, &login, view)?;
    let mut s = Session::start(conn, &name, uuid);
    s.st.registry = registry;
    s.wait(Duration::from_secs(30), "the spawn position", |st| st.spawned)?;
    Ok((s, json!({"uuid": uuid_string(uuid)})))
}

pub fn verb_leave(s: &mut Session, _args: &Value) -> Result<Value, String> {
    s.w.shutdown();
    s.closed = true;
    Ok(json!({}))
}

pub fn verb_chat(s: &mut Session, args: &Value) -> Result<Value, String> {
    let text = str_arg(args, "text")?;
    s.send(chat_packet(text))?;
    Ok(json!({}))
}

pub fn verb_command(s: &mut Session, args: &Value) -> Result<Value, String> {
    let line = str_arg(args, "line")?;
    let command = line.trim_start_matches('/').to_string();
    s.send(S::ChatCommand(play::ChatCommand { command }))?;
    Ok(json!({}))
}

/// Waits for `n` game ticks to arrive from the server (time packets).
pub fn verb_wait_ticks(s: &mut Session, args: &Value) -> Result<Value, String> {
    let n = num_arg(args, "n")? as u64;
    let target = s.st.tick + n;
    let limit = Duration::from_millis(n * 50 + 10_000);
    s.wait(limit, "the game ticks", |st| st.tick >= target)?;
    Ok(json!({"tick": s.st.tick}))
}

enum Wait {
    Loaded,
    Chunks(usize),
    /// A chat or system line containing this text (a substring, not a regex).
    Message(String),
}

/// `what`: `loaded`, `chunks N` or `message TEXT`; `timeout` in seconds (default 30).
pub fn verb_wait_for(s: &mut Session, args: &Value) -> Result<Value, String> {
    let what = str_arg(args, "what")?;
    let timeout = args.get("timeout").and_then(Value::as_f64).unwrap_or(30.0);
    let wait = if what == "loaded" {
        Wait::Loaded
    } else if let Some(n) = what.strip_prefix("chunks ") {
        Wait::Chunks(n.trim().parse().map_err(|_| format!("chunks: a count, not {n}"))?)
    } else if let Some(text) = what.strip_prefix("message ") {
        Wait::Message(text.to_string())
    } else {
        return Err(format!("unknown wait: {what}"));
    };
    let done = |st: &crate::state::State| match &wait {
        Wait::Loaded => st.loaded,
        Wait::Chunks(n) => st.chunks.held.len() >= *n,
        Wait::Message(text) => st.messages.items.iter().any(|m| m.plain.contains(text.as_str())),
    };
    s.wait(Duration::from_secs_f64(timeout), &what, done)?;
    Ok(json!({}))
}

pub fn verb_state(s: &mut Session, _args: &Value) -> Result<Value, String> {
    let st = &s.st;
    let packs: Vec<Value> = st
        .packs
        .iter()
        .map(|p| {
            json!({
                "id": uuid_string(p.id),
                "url": p.url,
                "hash": p.hash,
                "required": p.required,
                "prompt": p.prompt,
            })
        })
        .collect();
    Ok(json!({
        "name": st.name,
        "uuid": uuid_string(st.uuid),
        "pos": pos_json(st.player.pos),
        "yaw": st.player.yaw,
        "pitch": st.player.pitch,
        "health": st.player.health,
        "gamemode": st.player.gamemode,
        "on_ground": st.player.on_ground,
        "tick": st.tick,
        "spawned": st.spawned,
        "loaded": st.loaded,
        "view_distance": st.login_distances.map(|d| d.0),
        "simulation_distance": st.login_distances.map(|d| d.1),
        "packs": packs,
        "entity_id": st.self_id,
        "data": st.self_data.iter().map(|(i, (k, v))| json!({"index": i, "type": k, "value": meta_json(v)})).collect::<Vec<Value>>(),
    }))
}

/// The chat and system lines from game tick `since` on (all when absent).
pub fn verb_messages(s: &mut Session, args: &Value) -> Result<Value, String> {
    let since = args.get("since").and_then(Value::as_u64).unwrap_or(0);
    let messages: Vec<Value> = s
        .st
        .messages
        .items
        .iter()
        .filter(|m| m.tick >= since)
        .map(|m| json!({"tick": m.tick, "plain": m.plain, "json": m.json}))
        .collect();
    Ok(json!({"messages": messages}))
}

/// The tracked entities, optionally of one registry name (`type`) and within `radius` of the bot, with
/// their velocity and metadata when the server sent them. Reading it also empties the entity events.
pub fn verb_entities(s: &mut Session, args: &Value) -> Result<Value, String> {
    let names = s
        .st
        .registry
        .as_ref()
        .and_then(|r| r.get("minecraft:entity_type"))
        .ok_or("entities: join with registries (the path of registries.json)")?;
    let kind = match args.get("type").and_then(Value::as_str) {
        Some(name) => Some(names.id(name).ok_or(format!("unknown entity type: {name}"))?),
        None => None,
    };
    let radius = args.get("radius").and_then(Value::as_f64);
    let me = s.st.player.pos;
    let entities: Vec<Value> = s
        .st
        .entities
        .list
        .iter()
        .filter(|(_, t)| kind.is_none_or(|k| t.kind == k))
        .filter(|(_, t)| {
            radius.is_none_or(|r| {
                let d: f64 = (0..3).map(|i| (t.pos[i] - me[i]).powi(2)).sum();
                d.sqrt() <= r
            })
        })
        .map(|(id, t)| {
            let name = names.name(t.kind).map_or_else(|| t.kind.to_string(), str::to_string);
            let mut e = json!({"id": id, "type": name, "pos": pos_json(t.pos)});
            if let Some(v) = t.vel {
                e["vel"] = json!(v);
            }
            if !t.data.is_empty() {
                let data: Vec<Value> = t
                    .data
                    .iter()
                    .map(|(i, (k, v))| json!({"index": i, "type": k, "value": meta_json(v)}))
                    .collect();
                e["data"] = data.into();
            }
            e
        })
        .collect();
    let events: Vec<Value> = std::mem::take(&mut s.st.events)
        .iter()
        .map(|e| json!({"tick": e.tick, "id": e.id, "event": e.event}))
        .collect();
    Ok(json!({"entities": entities, "events": events}))
}

/// The summary the parity tests read (as `HORIZON_NET_REPORT` does).
pub fn verb_report(s: &mut Session, _args: &Value) -> Result<Value, String> {
    let chat: Vec<&str> = s.st.messages.items.iter().map(|m| m.plain.as_str()).collect();
    Ok(json!({
        "error": s.st.error,
        "position": pos_json(s.st.player.pos),
        "chunks": s.st.chunks.held.len(),
        "chat": chat,
    }))
}
