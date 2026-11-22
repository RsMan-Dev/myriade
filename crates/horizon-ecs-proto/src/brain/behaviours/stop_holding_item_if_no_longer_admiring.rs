//! `StopHoldingItemIfNoLongerAdmiring` (`PiglinAi` CORE): with no admiration under way, an off hand that holds something but no shield
//! (`BLOCKS_ATTACKS`) gives it up (`stopHoldingOffHandItem`: the stack leaves the off hand).
use crate::brain::*;
use crate::equipment;
use crate::goals::{self, Slot};

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:stop_holding_item_if_no_longer_admiring",
    mems: &[&ADMIRING_ITEM],
    requires: &[Cap::Brain],
    one_shot: true,
    can_start,
    start,
    ..Behaviour::DEFAULT
};

/// The off hand holds something, and it is not a shield.
pub fn holds_non_shield(held: u32, shield: bool) -> bool {
    held != 0 && !shield
}

fn can_start(w: &mut World, e: Entity, _: &Ctx) -> bool {
    let held = goals::equipped(w, e, Slot::Off);
    let shield = held == crate::items::item_id("minecraft:shield");
    !present(w, e, &ADMIRING_ITEM) && holds_non_shield(held, shield)
}

/// `stopHoldingOffHandItem(piglin, true)`: the stack is thrown toward a player or nearby (`PiglinAi.throwItems`, the sight of 16).
fn start(w: &mut World, e: Entity, _: &Ctx) {
    let id = equipment::take_slot(w, e, Slot::Off);
    let Some(name) = crate::items::item_name(id) else { return };
    let stack = horizon_client_lib::items::ItemStack::new(name, 1);
    crate::gift_drop::throw_toward_player_or_nearby(w, e, vec![stack], 16.0, 4, 2);
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_shield_stays_in_the_off_hand() {
        assert!(!super::holds_non_shield(7, true));
        assert!(super::holds_non_shield(7, false));
    }
}
