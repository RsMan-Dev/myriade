//! Passive entities (`Display`, `Interaction`, `Marker`): no goal, no clock, no move, so no system visits them. The compound is kept
//! as loaded (its free keys in `NbtData`): the ECS reads none of a display's settings. An interaction's box and answer (`Interact`) are
//! declared keys (`FIELDS`). Saving writes the position and the fields Horizon's `addAdditionalSaveData` writes.
use crate::cow::Pos;
use crate::nbt::{owner, Field, Kind};
use bevy_ecs::prelude::*;
use horizon_core::geometry::{Aabb, Vec3};
use horizon_core::nbt::{Compound, JavaString, Tag};
use std::borrow::Cow;

pub use crate::species::lists::DISPLAYS;
use crate::species::lists::{BLOCK_DISPLAY, INTERACTION, ITEM_DISPLAY, TEXT_DISPLAY};

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

/// `Interaction.readAdditionalSaveData` with no key: the size 1 x 1, no answer, no action.
impl Default for Interact {
    fn default() -> Self {
        Self { width: 1.0, height: 1.0, response: false, attack: None, interaction: None }
    }
}

/// A display's box (`width`, `height`), as saved: zero when absent.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct DisplayBox {
    pub width: f32,
    pub height: f32,
}

/// The keys of an `Interaction` and of a display, declared to the registry. `width` and `height` belong to whichever of `Interact` or
/// `DisplayBox` the entity has: the default of each is its own (1 for an interaction, 0 for a display).
pub const FIELDS: &[Field] = &[
    Field { key: Cow::Borrowed("width"), write: write_width, read: read_width, accepts: crate::nbt::accept::any, owner: Some(owner::<Interact>) },
    Field { key: Cow::Borrowed("height"), write: write_height, read: read_height, accepts: crate::nbt::accept::any, owner: Some(owner::<Interact>) },
    Field { key: Cow::Borrowed("width"), write: write_display_width, read: read_display_width, accepts: crate::nbt::accept::any, owner: Some(owner::<DisplayBox>) },
    Field { key: Cow::Borrowed("height"), write: write_display_height, read: read_display_height, accepts: crate::nbt::accept::any, owner: Some(owner::<DisplayBox>) },
    Field { key: Cow::Borrowed("response"), write: write_response, read: read_response, accepts: crate::nbt::accept::any, owner: Some(owner::<Interact>) },
    Field { key: Cow::Borrowed("attack"), write: write_attack, read: read_attack, accepts: crate::nbt::accept::any, owner: Some(owner::<Interact>) },
    Field { key: Cow::Borrowed("interaction"), write: write_interaction, read: read_interaction, accepts: crate::nbt::accept::any, owner: Some(owner::<Interact>) },
];

fn write_display_width(w: &World, e: Entity, c: &mut Compound) {
    if let Some(b) = w.get::<DisplayBox>(e) {
        c.insert("width".into(), real_tag(b.width));
    }
}

fn read_display_width(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Some(mut b) = w.get_mut::<DisplayBox>(e) else { return false };
    b.width = num(Some(t)).unwrap_or(0.0) as f32;
    true
}

fn write_display_height(w: &World, e: Entity, c: &mut Compound) {
    if let Some(b) = w.get::<DisplayBox>(e) {
        c.insert("height".into(), real_tag(b.height));
    }
}

fn read_display_height(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Some(mut b) = w.get_mut::<DisplayBox>(e) else { return false };
    b.height = num(Some(t)).unwrap_or(0.0) as f32;
    true
}

/// A `Float` value, zero stored as +0.0.
fn real_tag(v: f32) -> Tag {
    Tag::Float(if v == 0.0 { 0 } else { v.to_bits() })
}

fn write_width(w: &World, e: Entity, c: &mut Compound) {
    if let Some(i) = w.get::<Interact>(e) {
        c.insert("width".into(), Tag::Float(i.width.to_bits()));
    }
}

fn read_width(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Some(mut i) = w.get_mut::<Interact>(e) else { return false };
    i.width = num(Some(t)).unwrap_or(1.0) as f32;
    true
}

fn write_height(w: &World, e: Entity, c: &mut Compound) {
    if let Some(i) = w.get::<Interact>(e) {
        c.insert("height".into(), Tag::Float(i.height.to_bits()));
    }
}

fn read_height(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Some(mut i) = w.get_mut::<Interact>(e) else { return false };
    i.height = num(Some(t)).unwrap_or(1.0) as f32;
    true
}

fn write_response(w: &World, e: Entity, c: &mut Compound) {
    if let Some(i) = w.get::<Interact>(e) {
        c.insert("response".into(), Tag::Byte(i8::from(i.response)));
    }
}

fn read_response(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Some(mut i) = w.get_mut::<Interact>(e) else { return false };
    i.response = num(Some(t)).is_some_and(|v| (v.floor() as i32) as i8 != 0);
    true
}

fn write_attack(w: &World, e: Entity, c: &mut Compound) {
    if let Some(a) = w.get::<Interact>(e).and_then(|i| i.attack.as_deref()) {
        c.insert("attack".into(), a.to_tag());
    }
}

fn read_attack(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Some(mut i) = w.get_mut::<Interact>(e) else { return false };
    i.attack = Act::from_tag(Some(t));
    true
}

fn write_interaction(w: &World, e: Entity, c: &mut Compound) {
    if let Some(a) = w.get::<Interact>(e).and_then(|i| i.interaction.as_deref()) {
        c.insert("interaction".into(), a.to_tag());
    }
}

fn read_interaction(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Some(mut i) = w.get_mut::<Interact>(e) else { return false };
    i.interaction = Act::from_tag(Some(t));
    true
}

impl Interact {
    /// `Interaction.refreshDimensions`: `width` square about x and z, from the feet (`at`) up `height`.
    pub fn bounds(&self, at: Vec3) -> Aabb {
        let half = f64::from(self.width) / 2.0;
        Aabb::new(Vec3::new(at.x - half, at.y, at.z - half), Vec3::new(at.x + half, at.y + f64::from(self.height.max(0.0)), at.z + half))
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

/// A passive entity of `kind` at `at`, with the compound it is saved as (`id` is set when absent). The position asked for wins over the
/// compound's `Pos`, as `place_summoned` snaps a summoned entity. An interaction starts with its box and answer at their defaults.
pub fn spawn(w: &mut World, kind: &str, at: Vec3, mut tag: Tag) -> Entity {
    if let Tag::Compound(c) = &mut tag {
        c.entry("id".into()).or_insert_with(|| Tag::String(kind.into()));
        c.remove(&JavaString::from("Pos"));
    }
    let e = w.spawn((Pos(at), crate::goals::Uid(crate::goals::next_uid()), Kind(kind.into()), crate::nbt::Saved)).id();
    if kind == INTERACTION {
        w.entity_mut(e).insert(Interact::default());
    } else if DISPLAYS.contains(&kind) {
        w.entity_mut(e).insert(DisplayBox::default());
    }
    if let Tag::Compound(c) = &tag {
        crate::nbt::load(w, e, c);
    }
    e
}

/// The compound of the entity as Horizon saves it (`other_to_nbt`): the registry's compound (position, interaction, free keys), and the
/// fields of its kind.
pub fn save(w: &World, e: Entity) -> Tag {
    let mut tag = Tag::Compound(crate::nbt::save(w, e).unwrap_or_default());
    let kind = text_of(tag.get("id")).unwrap_or_default();
    let fields = if DISPLAYS.contains(&kind.as_str()) { display_fields(&tag, &kind) } else { Vec::new() };
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
