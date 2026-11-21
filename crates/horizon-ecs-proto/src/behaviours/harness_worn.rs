//! The harness a happy ghast wears (`happy_ghast.rs` `wearing_harness`, `harnessed_adult`, `harness_texture`): an adult with no harness
//! takes an item whose name ends with `_harness` from a player's hand, and keeps its name for the texture
//! (`entity/equipment/happy_ghast_body/<name>`). A baby (`GrowAt`) never takes one. The restriction radius and the ride depend on it.
//! DOUTE : la consommation de l'objet et le son des lunettes (`harness_goggles_down/up`) sont du cadre (socle).
//! DOUTE : `goals::item_name` (identifiant vers nom) n'existe pas dans le cadre ; `is_harness` en dépend.
use crate::behaviours::Interaction;
use crate::cow::GrowAt;
use crate::rules::harness_worn::is_harness_name;
use crate::rules::{no_attrs, no_interact, Rule};
use bevy_ecs::prelude::*;

/// The worn harness, by item name.
#[derive(Component, Clone, Debug)]
pub struct Harness {
    pub item: &'static str,
}

pub static RULE: Rule = Rule { name: "happy_ghast_harness", equip, attrs: no_attrs, interact };

fn equip(_: &mut World, _: Entity, _: &'static (dyn std::any::Any + Sync)) {}

/// `harnessed_adult`: wearing a harness and not a baby.
pub fn harnessed_adult(w: &World, e: Entity) -> bool {
    w.get::<Harness>(e).is_some() && w.get::<GrowAt>(e).is_none()
}

/// A player offers an item to an adult without a harness: the harness is worn if the item is one.
fn interact(w: &mut World, e: Entity, p: &'static (dyn std::any::Any + Sync), i: &Interaction) -> bool {
    if w.get::<Harness>(e).is_some() && crate::items::item_name(i.item) == Some("minecraft:shears") {
        return shear_harness(w, e, i);
    }
    if w.get::<Harness>(e).is_some() || w.get::<GrowAt>(e).is_some() || !is_harness(i.item) {
        return no_interact(w, e, p, i);
    }
    let item: &'static str = crate::items::item_name(i.item).unwrap_or_default();
    w.entity_mut(e).insert(Harness { item });
    crate::give_item::take_from_hand(w, i);
    let at = w.get::<crate::cow::Pos>(e).map_or([0.0; 3], |q| [q.0.x, q.0.y, q.0.z]);
    w.resource_mut::<crate::goals::Events>().host.sound("minecraft:entity.happy_ghast.equip", at, 1.0, 1.0);
    true
}
/// `Mob.attemptToShearEquipment`: shears in hand take the harness off unless something rides (`canShearEquipment`). The harness is thrown out at
/// the passenger attachment (the middle of the body here), the shears wear one use, the unequip sound and the `shear` event follow.
/// DOUTE : the binding curse (`PREVENT_ARMOR_CHANGE`) is not read, the harness item is not kept as a stack (only its name).
fn shear_harness(w: &mut World, e: Entity, i: &Interaction) -> bool {
    let riders = !crate::seat::riders_of(w, e).is_empty() || !crate::seat::players_of(w, e).is_empty() || w.get::<crate::mount::Mount>(e).is_some_and(|m| m.rider.is_some() || m.passenger.is_some());
    if riders {
        return false;
    }
    let Some(h) = w.entity_mut(e).take::<Harness>() else { return false };
    let at = w.get::<crate::cow::Pos>(e).map_or([0.0; 3], |q| [q.0.x, q.0.y, q.0.z]);
    let half = w.get::<crate::cow::Body>(e).map_or(0.5, |b| f64::from(b.height) * 0.5);
    crate::behaviours::shear::drop_stacks(w, vec![horizon_client_lib::items::ItemStack::new(h.item, 1)], [at[0], at[1] + half, at[2]]);
    let ev = &mut w.resource_mut::<crate::goals::Events>().host;
    ev.sound("minecraft:entity.happy_ghast.unequip", at, 1.0, 1.0);
    ev.game_event_by(e, "minecraft:shear", at, false, false, false);
    ev.held_worn(i.player, i.item, 1);
    true
}
fn is_harness(item: u32) -> bool {
    crate::items::item_name(item).is_some_and(|n| is_harness_name(&n))
}
