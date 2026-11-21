//! The rule `IGNITE`: a player lights the fuse of the entity with an item of a tag (`Creeper.mobInteract`: flint and steel, fire charge).
//! The sound depends on the item, the item wears or is used up (`held_wear::spend_one`). Nothing here is about one species.
use crate::bridge::view;
use crate::cow::Pos;
use crate::explosion::{set_dir, Fuse};
use crate::goals::*;
use crate::held_wear::spend_one;
use crate::rules::{no_attrs, params, Rule};
use bevy_ecs::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct IgniteParams {
    /// The tag (or names) of the items that light the fuse.
    pub items: &'static [&'static str],
    /// The sound by item (the first match), else `sound`.
    pub sounds: &'static [(&'static str, &'static str)],
    pub sound: &'static str,
}
impl IgniteParams {
    /// `Creeper.mobInteract`: `#creeper_igniters`; FIRECHARGE_USE for the fire charge, FLINTANDSTEEL_USE otherwise.
    pub const CREEPER: Self = Self {
        items: &["#minecraft:creeper_igniters"],
        sounds: &[("minecraft:fire_charge", "item.firecharge.use")],
        sound: "item.flintandsteel.use",
    };
}
pub static IGNITE: Rule = Rule { name: "ignite", equip: |_, _, _| {}, attrs: no_attrs, interact };

fn interact(w: &mut World, e: Entity, p: &'static (dyn std::any::Any + Sync), i: &crate::behaviours::Interaction) -> bool {
    let p = *params::<IgniteParams>(p);
    if i.item == 0 || !p.items.iter().any(|n| crate::items::is(n, i.item)) || w.get::<Fuse>(e).is_none() {
        return false;
    }
    let Some(at) = w.get::<Pos>(e).map(|p| p.0) else { return false };
    let sound = p.sounds.iter().find(|(n, _)| crate::items::is(n, i.item)).map_or(p.sound, |(_, s)| *s);
    let pitch = with_rng(w, e, |r| r.next_float()) * 0.4 + 0.8;
    w.resource_mut::<Events>().host.sound(sound, [at.x, at.y, at.z], 1.0, pitch);
    if let Some(mut f) = w.get_mut::<Fuse>(e) {
        f.ignited = true;
    }
    set_dir(w, e, 1);
    view::mark(w, e);
    spend_one(w, i);
    true
}
