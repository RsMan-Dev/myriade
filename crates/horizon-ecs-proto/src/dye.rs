//! The rule `DYE`: a player paints a part of the entity with a dye (the collar of a tamed wolf or cat, the fleece of a sheep). One rule, the
//! part painted and the items accepted come from the parameters. The collar is a rare state (`Collar`, sparse); no colour set means red.
use crate::bridge::view;
use crate::cow::Pos;
use crate::farm::Wool;
use crate::goals::*;
use crate::rules::{no_attrs, params, Rule};
use crate::species::cushion::DYES;
use crate::tame::Owner;
use crate::targeting::Who;
use bevy_ecs::prelude::*;

/// The colour of a collar (`setCollarColor`), set once a dye was used; absent: `DEFAULT_COLLAR`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
#[component(storage = "SparseSet", on_insert = crate::bridge::view::touched, on_remove = crate::bridge::view::touched)]
pub struct Collar(pub u8);
/// Red (`DyeColor.RED`), the collar of an undyed wolf or cat.
pub const DEFAULT_COLLAR: u8 = 14;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Paints {
    /// The collar of a tamed entity, for its owner only.
    Collar,
    /// The fleece of an unsheared animal (`Wool.color`).
    Fleece,
}
#[derive(Clone, Copy, Debug)]
pub struct DyeParams {
    pub items: &'static [&'static str],
    pub paints: Paints,
    pub sound: Option<&'static str>,
    /// The entity stays for good once dyed (`setPersistenceRequired`).
    pub keep: bool,
}
impl DyeParams {
    /// `Wolf.mobInteract`: `#wolf_collar_dyes`.
    pub const WOLF: Self = Self { items: &["#minecraft:wolf_collar_dyes"], paints: Paints::Collar, sound: None, keep: false };
    /// `Cat.mobInteract`: `#cat_collar_dyes`, and the cat stays.
    pub const CAT: Self = Self { items: &["#minecraft:cat_collar_dyes"], paints: Paints::Collar, sound: None, keep: true };
    /// `DyeItem.interactLivingEntity` on a sheep.
    pub const SHEEP: Self = Self { items: &["#minecraft:dyes"], paints: Paints::Fleece, sound: Some("item.dye.use"), keep: false };
}
pub static DYE: Rule = Rule { name: "dye", equip: |_, _, _| {}, attrs: no_attrs, interact };

/// The colour (index in `DYES`) of the item's `minecraft:dye` component.
pub fn dye_color(item: u32) -> Option<u8> {
    let color = crate::items::dye_name(item)?;
    DYES.iter().position(|d| *d == color).map(|c| c as u8)
}

fn interact(w: &mut World, e: Entity, p: &'static (dyn std::any::Any + Sync), i: &crate::behaviours::Interaction) -> bool {
    let p = *params::<DyeParams>(p);
    if i.item == 0 || !p.items.iter().any(|n| crate::items::is(n, i.item)) {
        return false;
    }
    let Some(color) = dye_color(i.item) else { return false };
    match p.paints {
        Paints::Collar => {
            if w.get::<Owner>(e).and_then(|o| o.0) != Some(Who::Player(i.player)) || w.get::<Collar>(e).map_or(DEFAULT_COLLAR, |c| c.0) == color {
                return false;
            }
            w.entity_mut(e).insert(Collar(color));
            if p.keep {
                crate::despawn::keep(w, e);
            }
        }
        Paints::Fleece => {
            let Some(mut wool) = w.get_mut::<Wool>(e) else { return false };
            if wool.sheared || wool.color == color {
                return false;
            }
            wool.color = color;
        }
    }
    if let (Some(s), Some(at)) = (p.sound, w.get::<Pos>(e).map(|p| p.0)) {
        w.resource_mut::<Events>().host.sound(s, [at.x, at.y, at.z], 1.0, 1.0);
    }
    view::mark(w, e);
    crate::give_item::take_from_hand(w, i);
    true
}
