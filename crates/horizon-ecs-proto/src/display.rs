//! The displays (`Display`, `BlockDisplay`, `ItemDisplay`, `TextDisplay`): passive, nothing ticks them. Their data are components: `Transform`
//! and `Shown` for every display, then `BlockState`, `Stack` or the sparse `TextData` for its kind. A write goes through `set`, which emits
//! one `HostEvent::DisplayUpdate` when the value changed. The summon is the host's: `spawn` emits nothing.
use crate::cow::Pos;
use crate::goals::Events;
use crate::passive;
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;
use horizon_core::nbt::Tag;
use horizon_items::items::ItemStack;
use horizon_world::registry::BlockStateId;
use std::sync::Arc;

/// `Display`'s transformation: the translation, the left and right rotations (quaternions) and the scale.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub translation: [f32; 3],
    pub left_rotation: [f32; 4],
    pub right_rotation: [f32; 4],
    pub scale: [f32; 3],
}

impl Default for Transform {
    fn default() -> Self {
        Self { translation: [0.0; 3], left_rotation: [0.0, 0.0, 0.0, 1.0], right_rotation: [0.0, 0.0, 0.0, 1.0], scale: [1.0; 3] }
    }
}

/// How a display shows: its interpolation (`interpolation_delay` ticks before it starts, `interpolation_duration` long, the teleport
/// `teleport_duration`), its billboard (0 fixed, 1 vertical, 2 horizontal, 3 centre), its brightness (-1: the light at the entity, else
/// the packed levels), its view range, its shadow (radius, strength), its glow colour (-1: none) and its box (`width`, `height`).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Shown {
    pub interpolation_delay: i32,
    pub interpolation_duration: i32,
    pub teleport_duration: i32,
    pub billboard: u8,
    pub brightness: i32,
    pub view_range: f32,
    pub shadow_radius: f32,
    pub shadow_strength: f32,
    pub glow_color: i32,
    pub width: f32,
    pub height: f32,
}

impl Default for Shown {
    fn default() -> Self {
        Self {
            interpolation_delay: 0,
            interpolation_duration: 0,
            teleport_duration: 0,
            billboard: 0,
            brightness: -1,
            view_range: 1.0,
            shadow_radius: 0.0,
            shadow_strength: 1.0,
            glow_color: -1,
            width: 0.0,
            height: 0.0,
        }
    }
}

/// `BlockDisplay`'s block state.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct BlockState(pub BlockStateId);

/// `ItemDisplay`'s stack and its display context (0 none, 1 third person left hand ... 9 on shelf: vanilla's order).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Stack {
    pub item: Box<ItemStack>,
    pub context: u8,
}

/// `TextDisplay`'s text and look. Sparse: the text is shared, and the flags keep the bits as saved (shadow 1, see through 2, default
/// background 4, left 8, right 16).
#[derive(Component, Clone, Debug, PartialEq)]
#[component(storage = "SparseSet")]
pub struct TextData {
    pub text: Arc<Tag>,
    pub line_width: i32,
    pub background: i32,
    pub opacity: i8,
    pub flags: u8,
}

impl Default for TextData {
    fn default() -> Self {
        Self { text: Arc::new(Tag::String("".into())), line_width: 200, background: 0x4000_0000, opacity: -1, flags: 0 }
    }
}

/// A display at `at` with its transformation, its way of showing, and what it shows (`kind`: a `BlockState`, a `Stack` or a `TextData`).
pub fn spawn(w: &mut World, at: Vec3, t: Transform, s: Shown, kind: impl Bundle) -> Entity {
    w.spawn((Pos(at), t, s, kind)).id()
}

/// A display of `kind` at `at` from its saved compound: `passive::spawn` keeps the compound and reads the box. The transformation and the
/// way of showing start at their defaults, until a write; a block, an item or a text is set by its kind's own components.
pub fn spawn_display(w: &mut World, kind: &'static str, at: Vec3, tag: Tag) -> Entity {
    let e = passive::spawn(w, kind, at, tag);
    w.entity_mut(e).insert((Transform::default(), Shown::default()));
    e
}

/// Writes `value` into the component of `e` when it differs, and emits one update. False when nothing changed or `e` has no such component.
pub fn set<C: Component<Mutability = bevy_ecs::component::Mutable> + PartialEq>(w: &mut World, e: Entity, value: C) -> bool {
    let Some(mut c) = w.get_mut::<C>(e) else { return false };
    if *c == value {
        return false;
    }
    *c = value;
    drop(c);
    w.resource_mut::<Events>().host.display_update(e);
    true
}
