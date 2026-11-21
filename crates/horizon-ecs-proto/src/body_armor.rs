//! The rule `WEAR`: the owner of a tamed entity puts a body armor on it (`Wolf.mobInteract`: wolf armor) or repairs the one it wears with a
//! repair item while it sits. The armor goes in the body slot with the whole stack the player held (`used_stack`: wear, enchantments, name).
use crate::cow::GrowAt;
use crate::equip_stack;
use crate::equipment::set_slot;
use crate::equipment_items::durability_of;
use crate::give_item::take_from_hand;
use crate::goals::*;
use crate::rules::{no_attrs, params, Rule};
use crate::tame::Owner;
use crate::targeting::Who;
use bevy_ecs::prelude::*;
use horizon_client_lib::items::ItemStack;

/// The body slot in `equip_stack::Equipment` (`EquipmentSlot.BODY`).
const BODY: usize = 6;

#[derive(Clone, Copy, Debug)]
pub struct WearParams {
    pub repair_items: &'static [&'static str],
    /// The share of the durability one repair item gives back.
    pub repair_share: f32,
    pub repair_sound: &'static str,
}
impl WearParams {
    /// `Wolf.mobInteract`: an armadillo scute gives back an eighth of the armor's durability.
    pub const WOLF: Self = Self { repair_items: &["minecraft:armadillo_scute"], repair_share: 0.125, repair_sound: "item.wolf_armor.repair" };
}
pub static WEAR: Rule = Rule { name: "body_armor", equip: |_, _, _| {}, attrs: no_attrs, interact };

fn interact(w: &mut World, e: Entity, p: &'static (dyn std::any::Any + Sync), i: &crate::behaviours::Interaction) -> bool {
    let p = *params::<WearParams>(p);
    if i.item == 0 || w.get::<Owner>(e).and_then(|o| o.0) != Some(Who::Player(i.player)) {
        return false;
    }
    let worn = equipped(w, e, Slot::Body);
    if worn == 0 {
        if crate::items::worn_in(i.item) != Some(Slot::Body) || w.get::<GrowAt>(e).is_some() {
            return false;
        }
        let Some(name) = crate::items::item_name(i.item) else { return false };
        set_slot(w, e, Slot::Body, i.item);
        // The whole stack the player held (wear, enchantments, name) when the host gave it, else a bare one.
        let worn_stack = crate::used_stack::of(w, i.item).map_or_else(
            || ItemStack::new(name, 1),
            |u| ItemStack { count: 1, ..u.stack.clone() },
        );
        equip_stack::set(w, e, BODY, Some(worn_stack));
        take_from_hand(w, i);
        return true;
    }
    if !is_occupied(w, e, OCC_SITTING) || !p.repair_items.iter().any(|n| crate::items::is(n, i.item)) {
        return false;
    }
    let Some(mut stack) = equip_stack::get(w, e, BODY) else { return false };
    if stack.damage == 0 {
        return false;
    }
    let back = (durability_of(worn) as f32 * p.repair_share) as u32;
    stack.damage = stack.damage.saturating_sub(back);
    equip_stack::set(w, e, BODY, Some(stack));
    if let Some(at) = w.get::<crate::cow::Pos>(e).map(|p| p.0) {
        w.resource_mut::<Events>().host.sound(p.repair_sound, [at.x, at.y, at.z], 1.0, 1.0);
    }
    take_from_hand(w, i);
    true
}
