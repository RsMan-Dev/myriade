use crate::*;
use horizon_core::nbt::JavaString;
use std::fmt;

/// A data component's id, `namespace:path` (`minecraft:damage`, `mymod:charge`), checked once
/// where a mod names it: what [`ItemResource`]'s component methods take, as NeoForge's take a
/// `DataComponentType`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ComponentId(Box<str>);

impl ComponentId {
    /// `id` as a component id, or None where it isn't a namespaced identifier
    /// (`Identifier.parse`'s characters, a namespace and a path): `Damage`, `!minecraft:food`
    /// and `my mod:charge` aren't.
    pub fn new(id: &str) -> Option<Self> {
        horizon_core::identifier::Identifier::parse_namespaced(id)?;
        Some(Self(id.into()))
    }
    /// The id, as saved (`minecraft:damage`).
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A visible item bar: its width out of thirteen pixels and its RGB colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemBar {
    /// Width out of thirteen pixels.
    pub width: i32,
    /// RGB color of the filled bar.
    pub color: u32,
}

/// Effective attack range for an actor, including the component's hitbox margin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ItemAttackRange {
    /// Minimum reach before the margin.
    pub min: f32,
    /// Maximum reach before the margin.
    pub max: f32,
    /// Margin added to either end of the range.
    pub hitbox_margin: f32,
}

impl ItemAttackRange {
    /// Inclusive vanilla float bounds compared with a double distance and optional server buffer.
    pub fn contains(self, distance: f64, buffer: f64) -> bool {
        distance >= f64::from(self.min - self.hitbox_margin) - buffer
            && distance <= f64::from(self.max + self.hitbox_margin) + buffer
    }
}
