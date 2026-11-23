//! `sounds.json` sound events as vanilla's `SoundManager.Preparations` reads
//! them: whether a file is valid (`is_valid_file`) and the event names it
//! normalizes to (`normalize`).
use serde_json::Value;

/// `SampledFloat` as sounds use it: constants and `MultipliedFloats`.
#[derive(Clone, Debug, PartialEq)]
pub enum Sampled {
    Constant(f32),
    Product(Vec<Sampled>),
}
/// `Sound`.
#[derive(Clone, Debug, PartialEq)]
pub struct Sound {
    /// The file identifier (`namespace:path`, under `sounds/` with `.ogg`).
    pub location: String,
    pub volume: Sampled,
    pub pitch: Sampled,
    pub weight: i32,
    /// `Sound.Type.SOUND_EVENT` (a reference to another event).
    pub event: bool,
    pub stream: bool,
    pub preload: bool,
    pub attenuation_distance: i32,
}
impl Sound {
    fn file(location: &str) -> Self {
        Self {
            location: location.to_owned(),
            volume: Sampled::Constant(1.0),
            pitch: Sampled::Constant(1.0),
            weight: 1,
            event: false,
            stream: false,
            preload: false,
            attenuation_distance: 16,
        }
    }
}
/// A `sounds.json` entry: event, `replace`, subtitle and sounds.
type Registration = (String, bool, Option<String>, Vec<Sound>);
/// Parse a `sounds.json`; `None` when Gson would reject the whole file (a
/// malformed entry anywhere in it).
fn parse_file(namespace: &str, text: &[u8]) -> Option<Vec<Registration>> {
    let Ok(Value::Object(map)) = serde_json::from_slice::<Value>(text) else {
        return None;
    };
    let mut parsed = Vec::new();
    for (name, def) in &map {
        let Value::Object(object) = def else {
            return None;
        };
        let replace = json_bool(object.get("replace").unwrap_or(&Value::Null), false);
        let subtitle = match object.get("subtitle") {
            None | Some(Value::Null) => None,
            Some(Value::String(s)) => Some(s.clone()),
            Some(_) => return None,
        };
        let mut sounds = Vec::new();
        if let Some(list) = object.get("sounds") {
            let Value::Array(list) = list else {
                return None;
            };
            for s in list {
                sounds.push(parse_sound(s)?);
            }
        }
        parsed.push((format!("{namespace}:{name}"), replace, subtitle, sounds));
    }
    Some(parsed)
}
/// Whether vanilla's `SoundManager` reads this `sounds.json` at all.
pub fn is_valid_file(text: &[u8]) -> bool {
    parse_file("minecraft", text).is_some()
}
fn split(id: &str) -> (&str, &str) {
    match id.split_once(':') {
        Some((namespace, path)) => (
            if namespace.is_empty() {
                "minecraft"
            } else {
                namespace
            },
            path,
        ),
        None => ("minecraft", id),
    }
}
pub fn normalize(id: &str) -> String {
    let (namespace, path) = split(id);
    format!("{namespace}:{path}")
}
/// `GsonHelper.getAsFloat`: Gson parses the number's text as a float.
fn json_float(v: &Value, fallback: f32) -> f32 {
    match v {
        Value::Number(n) => n.to_string().parse().unwrap_or(fallback),
        Value::String(s) => s.parse().unwrap_or(fallback),
        _ => fallback,
    }
}
fn json_int(v: &Value, fallback: i32) -> i32 {
    match v {
        Value::Number(n) => n
            .as_i64()
            .map(|i| i as i32)
            .or_else(|| n.as_f64().map(|f| f as i32))
            .unwrap_or(fallback),
        Value::String(s) => s.parse().unwrap_or(fallback),
        _ => fallback,
    }
}
pub(crate) fn json_bool(v: &Value, fallback: bool) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::String(s) => s == "true",
        _ => fallback,
    }
}
/// `SoundEventRegistrationSerializer.getSound`; None for an invalid entry
/// (vanilla rejects the whole file).
fn parse_sound(v: &Value) -> Option<Sound> {
    if let Value::String(name) = v {
        return Some(Sound::file(&normalize(name)));
    }
    let name = v.get("name")?.as_str()?;
    let event = match v.get("type") {
        None => false,
        Some(t) => match t.as_str()? {
            "file" => false,
            "event" => true,
            _ => return None,
        },
    };
    let volume = json_float(v.get("volume").unwrap_or(&Value::Null), 1.0);
    let pitch = json_float(v.get("pitch").unwrap_or(&Value::Null), 1.0);
    let weight = json_int(v.get("weight").unwrap_or(&Value::Null), 1);
    if !(volume > 0.0 && pitch > 0.0 && weight > 0) {
        return None;
    }
    Some(Sound {
        location: normalize(name),
        volume: Sampled::Constant(volume),
        pitch: Sampled::Constant(pitch),
        weight,
        event,
        stream: json_bool(v.get("stream").unwrap_or(&Value::Null), false),
        preload: json_bool(v.get("preload").unwrap_or(&Value::Null), false),
        attenuation_distance: json_int(v.get("attenuation_distance").unwrap_or(&Value::Null), 16),
    })
}
