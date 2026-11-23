//! Client-side `Allay.mobInteract` state, before the authoritative server reply.
use super::super::{Species};
use crate::items::ItemStack;

#[derive(Debug)]
pub enum Interaction {
    Duplicate,
    Give,
    Take(ItemStack),
}

impl Interaction {
    pub fn sound(&self) -> &'static str {
        match self {
            Self::Duplicate => "minecraft:block.amethyst_block.chime",
            Self::Give => "minecraft:entity.allay.item_given",
            Self::Take(_) => "minecraft:entity.allay.item_taken",
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use horizon_core::geometry::Vec3;
    use std::sync::Arc;


    #[test]
    fn giving_and_taking_update_the_state_used_by_the_next_click() {
        let mut mob = allay();
        let offered = ItemStack::new("minecraft:stone", 3);
        assert!(matches!(
            predict(&mut mob, Some(&offered), false, false, [1; 4]),
            Some(Interaction::Give)
        ));
        assert_eq!(mob.extra.main_hand_stack().unwrap().count, 1);
        assert_eq!(mob.extra.owner, Some([1; 4]));
        assert!(predict(&mut mob, Some(&offered), true, false, [1; 4]).is_none());
        assert!(predict(&mut mob, None, false, false, [1; 4]).is_none());
        let Some(Interaction::Take(returned)) = predict(&mut mob, None, true, false, [1; 4]) else {
            panic!("empty main hand takes the item")
        };
        assert_eq!(&*returned.item, "minecraft:stone");
        assert_eq!(returned.count, 1);
        assert!(mob.extra.main_hand_stack().is_none());
        assert!(!mob.extra.tame);
        assert_eq!(mob.extra.owner, None);
        assert!(predict(&mut mob, None, true, false, [1; 4]).is_none());
    }

    #[test]
    fn duplication_requires_the_tag_and_ready_flag_and_is_not_repeated() {
        let mut mob = allay();
        mob.extra.flag = true;
        mob.extra
            .set_main_hand_stack(Some(ItemStack::new("minecraft:stone", 1)));
        let offered = ItemStack::new("fixture:tagged_item", 2);
        assert!(predict(&mut mob, Some(&offered), false, false, [0; 4]).is_none());
        assert!(matches!(
            predict(&mut mob, Some(&offered), false, true, [0; 4]),
            Some(Interaction::Duplicate)
        ));
        assert_eq!(mob.extra.egg_time, 6000);
        assert!(predict(&mut mob, Some(&offered), false, true, [0; 4]).is_none());
        assert_eq!(
            &*mob.extra.main_hand_stack().unwrap().item,
            "minecraft:stone"
        );
    }
}
