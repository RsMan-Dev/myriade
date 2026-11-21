//! The rule `FIT_CHEST`: a tame animal without a chest takes the chest item from a player's hand (`AbstractChestedHorse.mobInteract`, `equipChest`): the
//! chest slots appear (`mount_inventory::chest_on`), the item is used up, the chest sound plays with a pitch of `1 + (a - b) * 0.2`. The
//! sound is a parameter (a llama's and a donkey's differ). Nothing here is about one species.
use crate::cow::*;
use crate::goals::*;
use crate::rules::{no_attrs, params, Rule};
use bevy_ecs::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct ChestParams {
    pub item: &'static str,
    pub sound: &'static str,
    /// The columns of the chest when the animal has no strength of its own (`mount_inventory::chest_columns`).
    pub columns: u8,
}
impl ChestParams {
    /// `Llama.playChestEquipsSound`: its columns are its strength.
    pub const LLAMA: Self = Self { item: "minecraft:chest", sound: "minecraft:entity.llama.chest", columns: 0 };
    /// `AbstractChestedHorse.playChestEquipsSound`, five columns.
    pub const DONKEY: Self = Self { item: "minecraft:chest", sound: "minecraft:entity.donkey.chest", columns: 5 };
}
pub static FIT_CHEST: Rule = Rule { name: "fit_chest", equip: |_, _, _| {}, attrs: no_attrs, interact };

fn interact(w: &mut World, e: Entity, p: &'static (dyn std::any::Any + Sync), i: &crate::behaviours::Interaction) -> bool {
    let p = *params::<ChestParams>(p);
    if !crate::items::is(p.item, i.item) || w.get::<crate::mount_inventory::MountInventory>(e).is_some() {
        return false;
    }
    if !w.get::<crate::tame::Owner>(e).is_some_and(|o| o.0.is_some()) {
        return false;
    }
    let columns = crate::mount_inventory::chest_columns(w, e, p.columns);
    crate::mount_inventory::chest_on(w, e, columns);
    let (a, b) = with_rng(w, e, |r| (r.next_int(1000), r.next_int(1000)));
    let at = w.get::<Pos>(e).map_or([0.0; 3], |q| [q.0.x, q.0.y, q.0.z]);
    w.resource_mut::<Events>().host.sound(p.sound, at, 1.0, 1.0 + (a - b) as f32 / 1000.0 * 0.2);
    crate::give_item::take_from_hand(w, i);
    true
}
