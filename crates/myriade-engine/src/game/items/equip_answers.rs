//! `IItemExtension.canEquip` on the game's side. The dispenser, a mob's pickup and the two
//! inventory menus ask it. No part of any item listens, so the component's answer stands.
use super::*;
use myriade_engine::mobs::equip::CanEquip;

/// A stack asked about for several slots and entities. Nothing is converted: the item has no part.
pub(in crate::game) struct EquipStack;

impl Game<'_> {
    pub(in crate::game) fn equip_stack(
        &self,
        _stack: &ItemStack,
        _kinds: &[&str],
        _targets: &[u32],
    ) -> EquipStack {
        EquipStack
    }

    pub(in crate::game) fn equip_can_apply(&self, _prepared: &EquipStack, _id: u32) -> bool {
        true
    }

    /// `LivingEntity.isEquippableInSlot`: the stack's `equippable` answer `data` stands.
    pub(in crate::game) fn can_equip(
        &self,
        _stack: &EquipStack,
        _slot: &str,
        _entity: u32,
        _kind: &str,
        data: bool,
    ) -> bool {
        data
    }

    /// `can_equip` lent to a menu click.
    pub(in crate::game) fn menu_can_equip(&self) -> MenuEquip {
        MenuEquip
    }
}

/// The game lent to a menu click as its `CanEquip`.
pub(in crate::game) struct MenuEquip;

impl CanEquip for MenuEquip {
    fn listens(&self, _stack: &ItemStack) -> bool {
        false
    }
    fn can_equip(&self, _stack: &ItemStack, _slot: &str, _entity: u32, _kind: &str, data: bool) -> bool {
        data
    }
}
