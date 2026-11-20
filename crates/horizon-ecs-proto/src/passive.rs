//! Passive entities (`Display`, `Interaction`, `Marker`): no goal, no clock, no move, so no system visits them. The compound is kept
//! as loaded (`Raw`): the ECS reads none of a display's settings. An interaction also keeps its box and answer (`Interact`) and
//! drops those keys from the compound. Saving writes the position and the fields Horizon's `addAdditionalSaveData` writes.
use crate::cow::Pos;
use bevy_ecs::prelude::*;
use horizon_core::geometry::{Aabb, Vec3};
use horizon_core::nbt::{Compound, Tag};

pub use crate::species::lists::DISPLAYS;
use crate::species::lists::{BLOCK_DISPLAY, INTERACTION, ITEM_DISPLAY, TEXT_DISPLAY};
const INTERACTION_KEYS: [&str; 5] = ["width", "height", "response", "attack", "interaction"];

/// The compound of the entity as Horizon keeps it (`persist.raw`): the state the ECS does not read.
#[derive(Component, Clone, Debug)]
pub struct Raw(pub Box<Tag>);

/// `Interaction.PlayerAction`: the player (four ints) and the game time of its last attack or use.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Act {
    pub player: [i32; 4],
    pub timestamp: i64,
}

/// An `Interaction`'s box (`width`, `height`) and answer (`response`), and its last attack and use.
#[derive(Component, Clone, Debug)]
pub struct Interact {
    pub width: f32,
    pub height: f32,
    pub response: bool,
    pub attack: Option<Box<Act>>,
    pub interaction: Option<Box<Act>>,
}

/// Any numeric tag as a number (`Codec.FLOAT` and `int_value` read the same kinds).
fn num(t: Option<&Tag>) -> Option<f64> {
    Some(match t? {
        Tag::Byte(v) => f64::from(*v),
        Tag::Short(v) => f64::from(*v),
        Tag::Int(v) => f64::from(*v),
        Tag::Long(v) => *v as f64,
        Tag::Float(bits) => f64::from(f32::from_bits(*bits)),
        Tag::Double(bits) => f64::from_bits(*bits),
        _ => return None,
    })
}
fn int(src: &Tag, key: &str, default: i32) -> i32 {
    num(src.get(key)).map_or(default, |v| v.floor() as i32)
}
fn flag(src: &Tag, key: &str) -> bool {
    num(src.get(key)).is_some_and(|v| (v.floor() as i32) as i8 != 0)
}
/// A `Float` field: the value as read (`f32`), zero stored as +0.0.
fn real(src: &Tag, key: &str, default: f32) -> Tag {
    let v = num(src.get(key)).map_or(default, |v| v as f32);
    Tag::Float(if v == 0.0 { 0 } else { v.to_bits() })
}
fn text_of(t: Option<&Tag>) -> Option<String> {
    match t? {
        Tag::String(s) => Some(s.to_string_lossy()),
        _ => None,
    }
}
/// A string field of a fixed set of names; any other value is the default.
fn named(v: Option<&Tag>, default: &str, names: &[&str]) -> Tag {
    let s = text_of(v).filter(|s| names.contains(&s.as_str()));
    Tag::String(s.as_deref().unwrap_or(default).into())
}

impl Act {
    fn from_tag(t: Option<&Tag>) -> Option<Box<Self>> {
        let t = t?;
        let Tag::IntArray(player) = t.get("player")? else {
            return None;
        };
        let timestamp = num(t.get("timestamp"))? as i64;
        Some(Box::new(Self { player: player.as_slice().try_into().ok()?, timestamp }))
    }
    fn to_tag(&self) -> Tag {
        Tag::compound([("player", Tag::IntArray(self.player.to_vec())), ("timestamp", Tag::Long(self.timestamp))])
    }
}

impl Interact {
    /// `Interaction.readAdditionalSaveData`: the size (1 x 1 when absent), the answer and the actions.
    pub fn load(tag: &Tag) -> Self {
        Self {
            width: num(tag.get("width")).unwrap_or(1.0) as f32,
            height: num(tag.get("height")).unwrap_or(1.0) as f32,
            response: flag(tag, "response"),
            attack: Act::from_tag(tag.get("attack")),
            interaction: Act::from_tag(tag.get("interaction")),
        }
    }
    /// `Interaction.refreshDimensions`: `width` square about x and z, from the feet (`at`) up `height`.
    pub fn bounds(&self, at: Vec3) -> Aabb {
        let half = f64::from(self.width) / 2.0;
        Aabb::new(Vec3::new(at.x - half, at.y, at.z - half), Vec3::new(at.x + half, at.y + f64::from(self.height.max(0.0)), at.z + half))
    }
    /// `addAdditionalSaveData`: each key with its value, `None` where the key is removed.
    fn fields(&self) -> Vec<(&'static str, Option<Tag>)> {
        vec![
            ("width", Some(Tag::Float(self.width.to_bits()))),
            ("height", Some(Tag::Float(self.height.to_bits()))),
            ("attack", self.attack.as_deref().map(Act::to_tag)),
            ("interaction", self.interaction.as_deref().map(Act::to_tag)),
            ("response", Some(Tag::Byte(i8::from(self.response)))),
        ]
    }
}

/// `Display.addAdditionalSaveData` for a display of `kind`: its settings as the codecs read them, defaults included. The transformation,
/// the block state and the item are kept as loaded (Horizon re-encodes them; not ported yet).
fn display_fields(src: &Tag, kind: &str) -> Vec<(&'static str, Option<Tag>)> {
    let light = |key: &str| num(src.get("brightness").and_then(|b| b.get(key))).map(|v| v as i32).filter(|l| (0..=15).contains(l)).map(Tag::Int);
    let brightness = match (light("block"), light("sky")) {
        (Some(block), Some(sky)) => Some(Tag::compound([("block", block), ("sky", sky)])),
        _ => None,
    };
    let mut f = vec![
        ("start_interpolation", None),
        ("brightness", brightness),
        ("transformation", src.get("transformation").cloned()),
        ("billboard", Some(named(src.get("billboard"), "fixed", &["vertical", "horizontal", "center"]))),
        ("interpolation_duration", Some(Tag::Int(int(src, "interpolation_duration", 0)))),
        ("teleport_duration", Some(Tag::Int(int(src, "teleport_duration", 0).clamp(0, 59)))),
        ("view_range", Some(real(src, "view_range", 1.0))),
        ("shadow_radius", Some(real(src, "shadow_radius", 0.0))),
        ("shadow_strength", Some(real(src, "shadow_strength", 1.0))),
        ("width", Some(real(src, "width", 0.0))),
        ("height", Some(real(src, "height", 0.0))),
        ("glow_color_override", Some(Tag::Int(int(src, "glow_color_override", -1)))),
    ];
    match kind {
        TEXT_DISPLAY => {
            let text = src
                .get("text")
                .filter(|v| match v {
                    Tag::String(_) => true,
                    Tag::Compound(c) => !c.is_empty(),
                    Tag::List(l) => !l.is_empty(),
                    _ => false,
                })
                .cloned()
                .unwrap_or_else(|| Tag::String("".into()));
            f.extend([
                ("text", Some(text)),
                ("line_width", Some(Tag::Int(int(src, "line_width", 200)))),
                ("text_opacity", Some(Tag::Byte(int(src, "text_opacity", -1) as i8))),
                ("background", Some(Tag::Int(int(src, "background", 0x4000_0000)))),
                ("alignment", Some(named(src.get("alignment"), "center", &["left", "right"]))),
                ("shadow", Some(Tag::Byte(i8::from(flag(src, "shadow"))))),
                ("see_through", Some(Tag::Byte(i8::from(flag(src, "see_through"))))),
                ("default_background", Some(Tag::Byte(i8::from(flag(src, "default_background"))))),
            ]);
        }
        BLOCK_DISPLAY => {
            f.push(("block_state", Some(src.get("block_state").cloned().unwrap_or_else(|| Tag::String("minecraft:air".into())))));
        }
        ITEM_DISPLAY => {
            let context = named(
                src.get("item_display"),
                "none",
                &["thirdperson_lefthand", "thirdperson_righthand", "firstperson_lefthand", "firstperson_righthand", "head", "gui", "ground", "fixed", "on_shelf"],
            );
            f.push(("item_display", Some(context)));
            f.push(("item", src.get("item").cloned()));
        }
        _ => {}
    }
    f
}

/// A passive entity of `kind` at `at`, with the compound it is saved as (`id` is set when absent). An interaction's box and answer
/// leave the compound for `Interact`.
pub fn spawn(w: &mut World, kind: &str, at: Vec3, mut tag: Tag) -> Entity {
    if let Tag::Compound(c) = &mut tag {
        c.entry("id".into()).or_insert_with(|| Tag::String(kind.into()));
    }
    let interact = (kind == INTERACTION).then(|| Interact::load(&tag));
    if let (Some(_), Tag::Compound(c)) = (&interact, &mut tag) {
        for key in INTERACTION_KEYS {
            c.remove(&key.into());
        }
    }
    let e = w.spawn((Pos(at), Raw(Box::new(tag)), crate::goals::Uid(crate::goals::next_uid()))).id();
    if let Some(i) = interact {
        w.entity_mut(e).insert(i);
    }
    e
}

/// The compound of the entity as Horizon saves it (`other_to_nbt`): the kept compound, its position, and the fields of its kind.
pub fn save(w: &World, e: Entity) -> Tag {
    let mut tag = w.get::<Raw>(e).map_or_else(|| Tag::Compound(Compound::default()), |r| (*r.0).clone());
    let kind = text_of(tag.get("id")).unwrap_or_default();
    let mut fields = if DISPLAYS.contains(&kind.as_str()) { display_fields(&tag, &kind) } else { Vec::new() };
    if let Some(p) = w.get::<Pos>(e) {
        fields.push(("Pos", Some(Tag::List([p.0.x, p.0.y, p.0.z].map(|v| Tag::Double(v.to_bits())).to_vec()))));
    }
    if let Some(i) = w.get::<Interact>(e) {
        fields.extend(i.fields());
    }
    if let Tag::Compound(c) = &mut tag {
        for (key, value) in fields {
            match value {
                Some(v) => {
                    c.insert(key.into(), v);
                }
                None => {
                    c.remove(&key.into());
                }
            }
        }
    }
    tag
}
