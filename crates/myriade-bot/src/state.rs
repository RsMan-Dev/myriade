//! What the bot knows of the game, written by the reader thread and read by the verbs.
use horizon_core::nbt::Tag;
use horizon_protocol::{
    crypto::offline_uuid,
    entity_data::Value as Meta,
    packets::play::{self, Clientbound as C, relative},
    registry::Builtin,
    text,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};

/// The bot's player: where it is and what the server last said about it.
#[derive(Clone, Debug, Default)]
pub struct Player {
    pub pos: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub health: f32,
    pub gamemode: i32,
    pub on_ground: bool,
    pub teleport_id: i32,
}

/// An entity the server announced: its UUID, its registry id and where it is.
#[derive(Clone, Debug)]
pub struct Tracked {
    pub uuid: u128,
    pub kind: i32,
    pub pos: [f64; 3],
    /// The velocity the server sent (at spawn or in a motion packet).
    pub vel: Option<[f64; 3]>,
    /// The metadata by index: the serializer id and the value.
    pub data: BTreeMap<u8, (i32, Meta)>,
}

/// An entity event the server sent (`EntityEvent`), with the game tick it arrived at.
#[derive(Clone, Debug)]
pub struct Event {
    pub tick: u64,
    pub id: i32,
    pub event: i8,
}

/// The entities around the bot, by id.
#[derive(Default)]
pub struct Entities {
    pub list: BTreeMap<i32, Tracked>,
}

impl Entities {
    fn apply(&mut self, p: &C) {
        match p {
            C::AddEntity(e) => {
                let tracked = Tracked {
                    uuid: e.uuid,
                    kind: e.kind,
                    pos: e.position,
                    vel: Some(e.velocity),
                    data: BTreeMap::new(),
                };
                self.list.insert(e.id, tracked);
            }
            C::MoveEntityPos(play::MoveEntityPos(m)) | C::MoveEntityPosRot(play::MoveEntityPosRot(m)) => {
                if let (Some(t), Some(d)) = (self.list.get_mut(&m.id), &m.delta) {
                    t.pos = d.end(t.pos);
                }
            }
            C::EntityPositionSync(s) => {
                if let Some(t) = self.list.get_mut(&s.id) {
                    t.pos = s.path.end();
                }
            }
            C::RemoveEntities(r) => {
                for id in &r.ids {
                    self.list.remove(id);
                }
            }
            C::SetEntityData(d) => {
                if let Some(t) = self.list.get_mut(&d.id) {
                    for (index, kind, value) in &d.values {
                        t.data.insert(*index, (*kind, value.clone()));
                    }
                }
            }
            C::SetEntityMotion(m) => {
                if let Some(t) = self.list.get_mut(&m.id) {
                    t.vel = Some(m.velocity);
                }
            }
            _ => {}
        }
    }
}

/// One line of chat or a system message, stamped with the game time it arrived at.
#[derive(Clone, Debug)]
pub struct Message {
    pub tick: u64,
    pub plain: String,
    /// The component as the server sent it (`translate`, `with`, `extra`, colours ...).
    pub json: Value,
}

/// The chat and system messages received, in order.
#[derive(Default)]
pub struct Messages {
    pub items: Vec<Message>,
}

/// One slot: the item id and its count.
pub type Slot = Option<(i32, i32)>;

/// A container screen the server opened for the bot.
#[derive(Clone, Debug)]
pub struct Menu {
    pub id: i32,
    pub kind: i32,
    pub state: i32,
    pub slots: Vec<Slot>,
}

/// The player's own inventory (container 0) and the screen open over it, if any.
#[derive(Default)]
pub struct Inventory {
    pub player: Vec<Slot>,
    pub menu: Option<Menu>,
}

/// The chunks the server sent and has not forgotten, and the batches it finished.
#[derive(Default)]
pub struct Chunks {
    pub held: HashSet<(i32, i32)>,
    pub received: usize,
    pub batches: usize,
}

/// Tick speed: `timer` multiplies vanilla's 20 Hz (1.0 by default); `no_swing` skips the arm swing.
#[derive(Clone, Copy, Debug)]
pub struct Cheat {
    pub timer: f64,
    pub ground: bool,
    pub no_swing: bool,
}

impl Default for Cheat {
    fn default() -> Self {
        Self {
            timer: 1.0,
            ground: false,
            no_swing: false,
        }
    }
}

/// A resource pack the server offered and the bot has not seen removed.
#[derive(Clone, Debug)]
pub struct Pack {
    pub id: u128,
    pub url: String,
    pub hash: String,
    pub required: bool,
    pub prompt: Option<String>,
}

/// A capture of clientbound packets: only the ids in `filter` (all when empty).
pub struct Capture {
    pub filter: Vec<i32>,
    pub packets: Vec<Packet>,
}

/// One captured packet: the game tick it arrived at, its id and its body in hex.
#[derive(Clone, Debug)]
pub struct Packet {
    pub tick: u64,
    pub id: i32,
    pub hex: String,
}

#[derive(Default)]
pub struct State {
    /// The built-in registries from `join`'s `registries` file: entity type names.
    pub registry: Option<Builtin>,
    /// Entity events received since the `entities` verb last read them.
    pub events: Vec<Event>,
    pub name: String,
    pub uuid: u128,
    pub player: Player,
    pub entities: Entities,
    pub messages: Messages,
    pub inventory: Inventory,
    pub chunks: Chunks,
    /// Game time, from the server's time packets (one every 20 ticks).
    pub tick: u64,
    /// The bot has its spawn position; it may move.
    pub spawned: bool,
    /// The bot told the server it is loaded.
    pub loaded: bool,
    /// The view (chunk radius) and simulation distances of the play Login packet.
    pub login_distances: Option<(i32, i32)>,
    /// The bot's own entity id (the play Login packet), and its synced data (flags at index 0, sprinting at bit 3).
    pub self_id: Option<i32>,
    pub self_data: BTreeMap<u8, (i32, Meta)>,
    pub error: Option<String>,
    /// The sequence number of the block and item actions.
    pub sequence: i32,
    pub cheat: Cheat,
    pub capture: Option<Capture>,
    /// Resource packs offered and not removed (the bot answers only when asked to).
    pub packs: Vec<Pack>,
}

/// The UUID of a player name (offline), or a UUID as written with dashes.
pub fn uuid_of(name: &str) -> u128 {
    let hex: String = name.chars().filter(|&c| c != '-').collect();
    match u128::from_str_radix(&hex, 16) {
        Ok(uuid) if name.contains('-') && hex.len() == 32 => uuid,
        _ => offline_uuid(name),
    }
}

impl State {
    /// Applies a clientbound play packet. Returns the id of a teleport the bot must acknowledge.
    pub fn apply(&mut self, p: &C) -> Option<i32> {
        match p {
            C::PlayerPosition(t) => {
                let rel = t.relatives;
                let axis = |i: usize, bit: i32, base: f64| {
                    if rel & bit != 0 {
                        base + t.position[i]
                    } else {
                        t.position[i]
                    }
                };
                let player = &mut self.player;
                player.pos = [
                    axis(0, relative::X, player.pos[0]),
                    axis(1, relative::Y, player.pos[1]),
                    axis(2, relative::Z, player.pos[2]),
                ];
                player.yaw = if rel & relative::Y_ROT != 0 { player.yaw + t.yaw } else { t.yaw };
                player.pitch = if rel & relative::X_ROT != 0 { player.pitch + t.pitch } else { t.pitch };
                player.teleport_id = t.teleport;
                self.spawned = true;
                return Some(t.teleport);
            }
            C::AddEntity(_) | C::MoveEntityPos(_) | C::MoveEntityPosRot(_) | C::EntityPositionSync(_) | C::RemoveEntities(_) => {
                self.entities.apply(p)
            }
            C::SetEntityData(d) => {
                if Some(d.id) == self.self_id {
                    for (index, kind, value) in &d.values {
                        self.self_data.insert(*index, (*kind, value.clone()));
                    }
                }
                self.entities.apply(p)
            }
            C::SetEntityMotion(_) => self.entities.apply(p),
            C::EntityEvent(e) => self.events.push(Event { tick: self.tick, id: e.id, event: e.event }),
            C::Login(l) => {
                self.login_distances = Some((l.chunk_radius, l.simulation_distance));
                self.self_id = Some(l.player_id);
            }
            C::SetHealth(h) => self.player.health = h.health,
            C::SetTime(t) => self.tick = t.game_time.max(0) as u64,
            C::ResourcePackPush(p) => self.packs.push(Pack {
                id: p.id,
                url: p.url.clone(),
                hash: p.hash.clone(),
                required: p.required,
                prompt: p.prompt.as_ref().map(|t| text::plain(t, &Default::default())),
            }),
            C::ResourcePackPop(p) => match p.id {
                Some(id) => self.packs.retain(|k| k.id != id),
                None => self.packs.clear(),
            },
            C::GameEvent(g) if g.event == 3 => self.player.gamemode = g.param as i32,
            C::SystemChat(c) => self.push_message(text::plain(&c.content, &Default::default()), tag_json(&c.content)),
            C::PlayerChat(c) => self.push_message(c.content.clone(), json!({"text": c.content})),
            C::ChunkBatchFinished(_) => self.chunks.batches += 1,
            C::LevelChunkWithLight(c) => {
                self.chunks.held.insert((c.x, c.z));
                self.chunks.received += 1;
            }
            C::ForgetLevelChunk(f) => {
                self.chunks.held.remove(&(f.pos[0], f.pos[1]));
            }
            C::OpenScreen(o) => {
                self.inventory.menu = Some(Menu {
                    id: o.container,
                    kind: o.menu,
                    state: 0,
                    slots: Vec::new(),
                });
            }
            C::ContainerClose(c) => {
                if self.inventory.menu.as_ref().is_some_and(|m| m.id == c.container) {
                    self.inventory.menu = None;
                }
            }
            C::ContainerSetContent(c) => {
                let slots: Vec<Slot> = c.items.iter().map(|s| s.as_ref().map(|s| (s.item, s.count))).collect();
                if c.container == 0 {
                    self.inventory.player = slots;
                } else if let Some(m) = self.inventory.menu.as_mut().filter(|m| m.id == c.container) {
                    m.slots = slots;
                    m.state = c.state;
                }
            }
            C::ContainerSetSlot(c) => {
                let slot = c.item.as_ref().map(|s| (s.item, s.count));
                if c.container == 0 {
                    if let Some(s) = self.inventory.player.get_mut(c.slot as usize) {
                        *s = slot;
                    }
                } else if let Some(m) = self.inventory.menu.as_mut().filter(|m| m.id == c.container) {
                    if let Some(s) = m.slots.get_mut(c.slot as usize) {
                        *s = slot;
                    }
                    m.state = c.state;
                }
            }
            _ => {}
        }
        None
    }

    /// Keeps a copy of a clientbound packet while a capture runs.
    pub fn record(&mut self, id: i32, body: &[u8]) {
        if let Some(c) = self.capture.as_mut()
            && (c.filter.is_empty() || c.filter.contains(&id))
        {
            let hex = body.iter().map(|b| format!("{b:02x}")).collect();
            c.packets.push(Packet { tick: self.tick, id, hex });
        }
    }

    fn push_message(&mut self, plain: String, json: Value) {
        self.messages.items.push(Message { tick: self.tick, plain, json });
    }
}

/// A chat component as JSON, in the shape the server sent it.
fn tag_json(tag: &Tag) -> Value {
    match tag {
        Tag::End => Value::Null,
        Tag::Byte(v) => json!(v),
        Tag::Short(v) => json!(v),
        Tag::Int(v) => json!(v),
        Tag::Long(v) => json!(v),
        Tag::Float(v) => json!(f32::from_bits(*v)),
        Tag::Double(v) => json!(f64::from_bits(*v)),
        Tag::ByteArray(v) => json!(v),
        Tag::String(s) => json!(s.to_string_lossy()),
        Tag::List(v) => Value::Array(v.iter().map(tag_json).collect()),
        Tag::Compound(c) => Value::Object(c.iter().map(|(k, v)| (k.to_string_lossy(), tag_json(v))).collect()),
        Tag::IntArray(v) => json!(v),
        Tag::LongArray(v) => json!(v),
    }
}

/// Entity metadata as JSON: plain values as they are, text as its component, the rest as debug text.
pub fn meta_json(v: &Meta) -> Value {
    match v {
        Meta::Byte(x) => json!(x),
        Meta::Int(x) | Meta::Id(x) => json!(x),
        Meta::Long(x) => json!(x),
        Meta::Float(x) => json!(x),
        Meta::String(x) => json!(x),
        Meta::Bool(x) => json!(x),
        Meta::Text(t) => tag_json(t),
        Meta::Pos(p) => json!(p),
        Meta::Floats(x) => json!(x),
        Meta::Ids(x) => json!(x),
        other => json!(format!("{other:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_teleport_moves_only_the_flagged_axes() {
        let mut state = State::default();
        state.player.pos = [10.0, 20.0, 30.0];
        let teleport = play::PlayerPosition {
            teleport: 3,
            position: [1.0, 2.0, 3.0],
            delta: [0.0; 3],
            yaw: 0.0,
            pitch: 0.0,
            relatives: relative::X | relative::Z,
        };
        assert_eq!(state.apply(&C::PlayerPosition(teleport)), Some(3));
        assert_eq!(state.player.pos, [11.0, 2.0, 33.0]);
        assert!(state.spawned);
    }
}
