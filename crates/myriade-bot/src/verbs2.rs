//! The verbs that act: movement, look, fighting, use and break, the inventory and its screens, quiet
//! commands, packet capture, the cheats, teleport acks and resource packs.
use crate::{
    session::Session,
    state::{Capture, Cheat, Menu, Slot, uuid_of},
    verbs::{num_arg, str_arg},
};
use horizon_protocol::{
    packets::{
        configuration::{CustomPayload, ResourcePackResponse},
        play::{self, Serverbound as S},
    },
    quiet,
};
use serde_json::{Value, json};

/// The next sequence number of the block and item actions.
fn next_sequence(s: &mut Session) -> i32 {
    s.st.sequence += 1;
    s.st.sequence
}

fn on_ground_flag(s: &Session) -> u8 {
    u8::from(s.st.player.on_ground || s.st.cheat.ground)
}

/// Sets the bot's position; the next tick sends it, with `on_ground` when given (a scripted fall).
pub fn verb_move(s: &mut Session, args: &Value) -> Result<Value, String> {
    s.st.player.pos = [num_arg(args, "x")?, num_arg(args, "y")?, num_arg(args, "z")?];
    if let Some(on_ground) = args.get("on_ground").and_then(Value::as_bool) {
        s.st.player.on_ground = on_ground;
    }
    Ok(json!({}))
}

pub fn verb_look(s: &mut Session, args: &Value) -> Result<Value, String> {
    let yaw = num_arg(args, "yaw")? as f32;
    let pitch = num_arg(args, "pitch")? as f32;
    s.st.player.yaw = yaw;
    s.st.player.pitch = pitch;
    let flags = on_ground_flag(s);
    s.send(S::MovePlayerRot(play::MovePlayerRot { yaw, pitch, flags }))?;
    Ok(json!({}))
}

/// Turns to face a point: vanilla's yaw and pitch for the offset from the eyes' position.
pub fn verb_look_at(s: &mut Session, args: &Value) -> Result<Value, String> {
    let at = [num_arg(args, "x")?, num_arg(args, "y")?, num_arg(args, "z")?];
    let p = s.st.player.pos;
    let (dx, dy, dz) = (at[0] - p[0], at[1] - p[1] - 1.62, at[2] - p[2]);
    let yaw = (-dx).atan2(dz).to_degrees() as f32;
    let pitch = (-dy).atan2(dx.hypot(dz)).to_degrees() as f32;
    verb_look(s, &json!({"yaw": yaw, "pitch": pitch}))
}

pub fn verb_attack(s: &mut Session, args: &Value) -> Result<Value, String> {
    let id = num_arg(args, "id")? as i32;
    s.send(S::Attack(play::Attack { id }))?;
    if !s.st.cheat.no_swing {
        s.swing()?;
    }
    Ok(json!({}))
}

pub fn verb_interact(s: &mut Session, args: &Value) -> Result<Value, String> {
    let id = num_arg(args, "id")? as i32;
    let location = [0.0; 3];
    s.send(S::Interact(play::Interact { id, hand: 0, location, secondary: args.get("secondary").and_then(Value::as_bool).unwrap_or(false) }))?;
    Ok(json!({}))
}

pub fn verb_use_item(s: &mut Session, _args: &Value) -> Result<Value, String> {
    let sequence = next_sequence(s);
    let (yaw, pitch) = (s.st.player.yaw, s.st.player.pitch);
    s.send(S::UseItem(play::UseItem { hand: 0, sequence, yaw, pitch }))?;
    Ok(json!({}))
}

/// Lets go of the item in use: vanilla's `releaseUsingItem`, RELEASE_USE_ITEM at the origin, facing down.
pub fn verb_release_use_item(s: &mut Session, _args: &Value) -> Result<Value, String> {
    s.send(S::PlayerAction(play::PlayerAction { action: 6, pos: [0; 3], direction: 0, sequence: 0 }))?;
    Ok(json!({}))
}

/// Holds hotbar slot `slot` (0 to 8).
pub fn verb_set_carried_item(s: &mut Session, args: &Value) -> Result<Value, String> {
    let slot = num_arg(args, "slot")? as i16;
    if !(0..=8).contains(&slot) {
        return Err(format!("slot {slot}: hotbar slots are 0 to 8"));
    }
    s.send(S::SetCarriedItem(play::SetCarriedItem { slot }))?;
    Ok(json!({}))
}

/// Whether the bot flies (serverbound player abilities, flag 2).
pub fn verb_player_abilities(s: &mut Session, args: &Value) -> Result<Value, String> {
    let flags = if bool_arg(args, "flying")? { 2 } else { 0 };
    s.send(S::PlayerAbilities(play::PlayerAbilitiesRequest { flags }))?;
    Ok(json!({}))
}

/// Uses the held item on the block at `x y z`, from the face `face` (vanilla's face ids).
pub fn verb_use_block(s: &mut Session, args: &Value) -> Result<Value, String> {
    let pos = [num_arg(args, "x")? as i32, num_arg(args, "y")? as i32, num_arg(args, "z")? as i32];
    let face = num_arg(args, "face")? as i32;
    let sequence = next_sequence(s);
    let cursor = [0.5; 3];
    let message = play::UseItemOn {
        hand: 0,
        pos,
        face,
        cursor,
        inside: false,
        world_border: false,
        sequence,
    };
    s.send(S::UseItemOn(message))?;
    Ok(json!({}))
}

/// Breaks the block at `x y z`: the start of the digging, then its stop (the creative break).
pub fn verb_break(s: &mut Session, args: &Value) -> Result<Value, String> {
    let pos = [num_arg(args, "x")? as i32, num_arg(args, "y")? as i32, num_arg(args, "z")? as i32];
    let direction = num_arg(args, "face").unwrap_or(1.0) as u8;
    for action in [0, 3] {
        let sequence = next_sequence(s);
        s.send(S::PlayerAction(play::PlayerAction { action, pos, direction, sequence }))?;
    }
    Ok(json!({}))
}

fn slots_json(slots: &[Slot]) -> Value {
    slots
        .iter()
        .map(|slot| match slot {
            Some((item, count)) => json!({"item": item, "count": count}),
            None => Value::Null,
        })
        .collect::<Vec<Value>>()
        .into()
}

fn menu_json(m: &Menu) -> Value {
    json!({"id": m.id, "kind": m.kind, "state": m.state, "slots": slots_json(&m.slots)})
}

pub fn verb_inventory(s: &mut Session, _args: &Value) -> Result<Value, String> {
    Ok(json!({"slots": slots_json(&s.st.inventory.player)}))
}

pub fn verb_menu(s: &mut Session, _args: &Value) -> Result<Value, String> {
    let menu = s.st.inventory.menu.as_ref().map(menu_json);
    Ok(json!({"menu": menu}))
}

/// A click on `slot` with `button`, in the open screen (or the player's inventory with none).
pub fn verb_click_slot(s: &mut Session, args: &Value) -> Result<Value, String> {
    let slot = num_arg(args, "slot")? as i16;
    let button = num_arg(args, "button").unwrap_or(0.0) as i8;
    let (container, state) = match &s.st.inventory.menu {
        Some(m) => (m.id, m.state),
        None => (0, 0),
    };
    let click = play::ContainerClick {
        container,
        state,
        slot,
        button,
        input: 0,
        changed: Vec::new(),
        carried: None,
    };
    s.send(S::ContainerClick(click))?;
    Ok(json!({}))
}

pub fn verb_close_menu(s: &mut Session, _args: &Value) -> Result<Value, String> {
    let container = s.st.inventory.menu.take().map_or(0, |m| m.id);
    s.send(S::ContainerClose(play::ContainerClose { container }))?;
    Ok(json!({}))
}

/// Sends the commands as quiet-command payloads (`horizon:quiet_commands`); `mod` names the sender.
pub fn verb_quiet(s: &mut Session, args: &Value) -> Result<Value, String> {
    let lines: Vec<String> = args
        .get("commands")
        .and_then(Value::as_array)
        .ok_or("missing array argument: commands")?
        .iter()
        .map(|c| c.as_str().map(str::to_string).ok_or("commands: a list of strings"))
        .collect::<Result<_, _>>()?;
    let mod_id = args.get("mod").and_then(Value::as_str).unwrap_or("myriade-bot");
    for data in quiet::encode(mod_id, &lines) {
        let channel = quiet::CHANNEL.to_string();
        s.send(S::CustomPayload(CustomPayload { channel, data }))?;
    }
    Ok(json!({"sent": lines.len()}))
}

/// `start` (with the ids to keep, all when absent) or `stop`, which returns the packets captured.
pub fn verb_capture(s: &mut Session, args: &Value) -> Result<Value, String> {
    match str_arg(args, "action")?.as_str() {
        "start" => {
            let filter = args
                .get("filter")
                .and_then(Value::as_array)
                .map(|ids| ids.iter().filter_map(Value::as_i64).map(|i| i as i32).collect())
                .unwrap_or_default();
            s.st.capture = Some(Capture { filter, packets: Vec::new() });
            Ok(json!({}))
        }
        "stop" => {
            let packets: Vec<Value> = s
                .st
                .capture
                .take()
                .ok_or("no capture running")?
                .packets
                .iter()
                .map(|p| json!({"tick": p.tick, "id": p.id, "hex": p.hex}))
                .collect();
            Ok(json!({"packets": packets}))
        }
        other => Err(format!("capture: start or stop, not {other}")),
    }
}

/// Sets the cheat flags: `timer` (speed, 0.1 to 20), `ground` and `no_swing`; absent flags stay.
pub fn verb_cheat(s: &mut Session, args: &Value) -> Result<Value, String> {
    let Cheat { mut timer, mut ground, mut no_swing } = s.st.cheat;
    if let Some(t) = args.get("timer").and_then(Value::as_f64) {
        if !(0.1..=20.0).contains(&t) {
            return Err(format!("timer {t}: use 0.1 to 20"));
        }
        timer = t;
    }
    ground = args.get("ground").and_then(Value::as_bool).unwrap_or(ground);
    no_swing = args.get("no_swing").and_then(Value::as_bool).unwrap_or(no_swing);
    s.st.cheat = Cheat { timer, ground, no_swing };
    Ok(json!({}))
}

/// `auto` turns the automatic teleport acks on or off; `id` acks that teleport now.
pub fn verb_tp_ack(s: &mut Session, args: &Value) -> Result<Value, String> {
    if let Some(auto) = args.get("auto").and_then(Value::as_bool) {
        s.auto_ack = auto;
    }
    if let Some(id) = args.get("id").and_then(Value::as_i64) {
        s.ack(id as i32)?;
    }
    Ok(json!({}))
}

fn bool_arg(args: &Value, key: &str) -> Result<bool, String> {
    args.get(key)
        .and_then(Value::as_bool)
        .ok_or_else(|| format!("missing boolean argument: {key}"))
}

/// A position given as `[x, y, z]`.
fn pos_arg(args: &Value) -> Result<[f64; 3], String> {
    let a = args
        .get("pos")
        .and_then(Value::as_array)
        .filter(|a| a.len() == 3)
        .ok_or("missing array argument: pos (3 numbers)")?;
    let mut p = [0.0; 3];
    for (out, v) in p.iter_mut().zip(a) {
        *out = v.as_f64().ok_or("pos: 3 numbers")?;
    }
    Ok(p)
}

/// Moves the vehicle the bot rides to `pos`, facing `yaw` and `pitch` (serverbound move vehicle).
pub fn verb_vehicle_move(s: &mut Session, args: &Value) -> Result<Value, String> {
    let position = pos_arg(args)?;
    let yaw = num_arg(args, "yaw")? as f32;
    let pitch = num_arg(args, "pitch")? as f32;
    s.send(S::MoveVehicle(play::MoveVehicleRequest { position, yaw, pitch, on_ground: false }))?;
    Ok(json!({}))
}

/// The boat's paddles: `left` and `right` held or not.
pub fn verb_paddle(s: &mut Session, args: &Value) -> Result<Value, String> {
    let left = bool_arg(args, "left")?;
    let right = bool_arg(args, "right")?;
    s.send(S::PaddleBoat(play::PaddleBoat { left, right }))?;
    Ok(json!({}))
}

/// The keys held, as vanilla's input byte: forward 1, backward 2, left 4, right 8, jump 16, shift 32, sprint 64.
pub fn verb_player_input(s: &mut Session, args: &Value) -> Result<Value, String> {
    const KEYS: [(&str, u8); 7] = [
        ("forward", 1),
        ("backward", 2),
        ("left", 4),
        ("right", 8),
        ("jump", 16),
        ("shift", 32),
        ("sprint", 64),
    ];
    let mut flags = 0;
    for (key, bit) in KEYS {
        if bool_arg(args, key)? {
            flags |= bit;
        }
    }
    s.send(S::PlayerInput(play::PlayerInput { flags }))?;
    Ok(json!({}))
}

/// Answers a resource pack offer the bot was asked to answer. `status` is the protocol's name, and
/// `id` the pack's UUID (dashed or not). The bot never answers an offer by itself.
pub fn verb_pack_status(s: &mut Session, args: &Value) -> Result<Value, String> {
    let id = uuid_of(&str_arg(args, "id")?);
    let action = match str_arg(args, "status")?.as_str() {
        "successfully_loaded" => 0,
        "declined" => 1,
        "failed_download" => 2,
        "accepted" => 3,
        "downloaded" => 4,
        "invalid_url" => 5,
        "failed_reload" => 6,
        "discarded" => 7,
        other => return Err(format!("unknown pack status: {other}")),
    };
    s.send(S::ResourcePack(ResourcePackResponse { id, action }))?;
    Ok(json!({}))
}
