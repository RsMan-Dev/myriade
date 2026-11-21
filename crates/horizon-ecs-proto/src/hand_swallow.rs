//! The rule `HAND_SWALLOW`: a player gives an item to a cube-like creature (`SulfurCube.mobInteract`, `equipItem`). A baby holding food of the
//! tag ages up by a tenth of the time left (the eating sound plays); an adult takes one swallowable item (one that has an archetype) as its
//! held block: refused when it already holds the same item, otherwise the block it held is thrown out from the middle of its body first. The
//! absorb sound and the `entity_interact` event follow. A baby takes no block. Not done: the fuse lit by flint and steel or a fire charge
//! (`canExplode`, `primeTime`), the bucket pickup, the eating sound of the baby (the jar has `playEatingSound`, its event is not read here).
use crate::behaviours::swallow_hold::{hold, release, Held, SearchAfter};
use crate::cow::*;
use crate::goals::Events;
use crate::rules::{no_attrs, params, Rule};
use bevy_ecs::prelude::*;
use horizon_client_lib::items::ItemStack;

#[derive(Clone, Copy, Debug)]
pub struct SwallowParams {
    /// The tag of the baby's foods.
    pub food: &'static str,
    pub absorb: &'static str,
}
impl SwallowParams {
    pub const SULFUR_CUBE: Self = Self { food: "#minecraft:sulfur_cube_food", absorb: "minecraft:entity.sulfur_cube.absorb" };
}
pub static HAND_SWALLOW: Rule = Rule { name: "hand_swallow", equip: |_, _, _| {}, attrs: no_attrs, interact };

fn interact(w: &mut World, e: Entity, p: &'static (dyn std::any::Any + Sync), i: &crate::behaviours::Interaction) -> bool {
    let p = params::<SwallowParams>(p);
    let Some(name) = crate::items::item_name(i.item) else { return false };
    let baby = w.get::<GrowAt>(e).is_some();
    if baby {
        return crate::items::is(p.food, i.item) && crate::hand_feed::age_up(w, e, i);
    }
    let Some(archetype) = horizon_client_lib::mobs::sulfur_cube::archetype_of(name) else { return false };
    let at = w.get::<Pos>(e).map_or([0.0; 3], |q| [q.0.x, q.0.y, q.0.z]);
    if let Some(h) = w.get::<Held>(e) {
        if &*h.item == name {
            return false;
        }
        if let Some(old) = release(w, e) {
            let half = w.get::<Body>(e).map_or(0.5, |b| f64::from(b.height) * 0.5);
            crate::behaviours::shear::drop_stacks(w, vec![ItemStack::new(&old, 1)], [at[0], at[1] + half, at[2]]);
            let until = now_of(w, e) + 100;
            w.entity_mut(e).insert(SearchAfter(until));
        }
    }
    hold(w, e, std::sync::Arc::from(name), archetype);
    crate::give_item::take_from_hand(w, i);
    let mut ev = w.resource_mut::<Events>();
    ev.host.sound(p.absorb, at, 1.0, 1.0);
    ev.host.game_event_by(e, "minecraft:entity_interact", at, false, false, false);
    true
}
