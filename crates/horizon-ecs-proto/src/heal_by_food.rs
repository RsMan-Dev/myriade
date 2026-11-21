//! The rule `HEAL_BY_FOOD` (`AbstractNautilus.mobInteract`, `TamableAnimal.feed`, `AbstractNautilus.usePlayerItem`): a tame adult that is hurt
//! eats a food of the tag: it heals `nutrition * per_nutrition` (`heal_without` when the item has no food component), plays its eating sound, the item
//! is used up; a bucket of `bucket_tag` comes back as `bucket_back` (not in creative). A baby, a wild one and one at full health refuse.
//! Not done: the bucket goes to the player through `held_given` (the jar puts it in the hand slot when the stack empties).
use crate::cow::*;
use crate::goals::Events;
use crate::rules::{no_attrs, params, Rule};
use bevy_ecs::prelude::*;
use horizon_client_lib::items::ItemStack;

#[derive(Clone, Copy, Debug)]
pub struct HealParams {
    pub food: &'static str,
    pub per_nutrition: f32,
    pub heal_without: f32,
    pub sound: &'static str,
    pub bucket_tag: &'static str,
    pub bucket_back: &'static str,
}
impl HealParams {
    pub const NAUTILUS: Self = Self {
        food: "#minecraft:nautilus_food",
        per_nutrition: 2.0,
        heal_without: 1.0,
        sound: "minecraft:entity.nautilus.eat",
        bucket_tag: "#minecraft:nautilus_bucket_food",
        bucket_back: "minecraft:water_bucket",
    };
}

pub static HEAL_BY_FOOD: Rule = Rule { name: "heal_by_food", equip: |_, _, _| {}, attrs: no_attrs, interact };

fn interact(w: &mut World, e: Entity, p: &'static (dyn std::any::Any + Sync), i: &crate::behaviours::Interaction) -> bool {
    let p = *params::<HealParams>(p);
    if i.item == 0 || !crate::items::is(p.food, i.item) || w.get::<GrowAt>(e).is_some() {
        return false;
    }
    if !w.get::<crate::tame::Owner>(e).is_some_and(|o| o.0.is_some()) {
        return false;
    }
    let (Some(h), Some(m)) = (w.get::<Health>(e).map(|h| h.0), w.get::<MaxHealth>(e).map(|m| m.0)) else { return false };
    if h >= m {
        return false;
    }
    let amount = crate::items::nutrition_of(i.item).map_or(p.heal_without, |n| n as f32 * p.per_nutrition);
    w.get_mut::<Health>(e).unwrap().0 = crate::rules::regen::healed(h, amount, m);
    let at = w.get::<Pos>(e).map_or([0.0; 3], |q| [q.0.x, q.0.y, q.0.z]);
    w.resource_mut::<Events>().host.sound(p.sound, at, 1.0, 1.0);
    let creative = w.get_resource::<crate::used_stack::UsedStack>().is_some_and(|u| u.creative);
    crate::give_item::take_from_hand(w, i);
    if !creative && crate::items::is(p.bucket_tag, i.item) {
        w.resource_mut::<Events>().host.held_given(i.player, ItemStack::new(p.bucket_back, 1));
    }
    true
}
