//! `StartAdmiringItemIfSeen` (`PiglinAi` CORE, `create(119)`): the wanted item in view is a loved item (the `piglin_loved` tag, gold),
//! no admiration runs, admiring is not disabled and no walk to admire is disabled: `admireGoldItem` sets `ADMIRING_ITEM` for 119 ticks.
use crate::brain::sensors::nearest_wanted_item::NEAREST_WANTED_ITEM;
use crate::brain::*;
use crate::{item, items, targeting::Who};

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:start_admiring_item_if_seen",
    mems: &[&ADMIRING_ITEM, &ADMIRING_DISABLED, &DISABLE_WALK_TO_ADMIRE_ITEM, &NEAREST_WANTED_ITEM],
    requires: &[Cap::Brain],
    one_shot: true,
    can_start,
    start,
    ..Behaviour::DEFAULT
};

/// `Params`-free: the admiration lasts this many ticks (`admireGoldItem`).
pub const ADMIRE_TICKS: u64 = 119;

/// `PiglinAi.isLovedItem`: the item kind is in the `minecraft:piglin_loved` tag.
pub fn is_loved(w: &World, it: Entity) -> bool {
    item::kind_of(w, it).is_some_and(|name| items::has_tag("minecraft:piglin_loved", items::item_id(name)))
}

fn can_start(w: &mut World, e: Entity, _: &Ctx) -> bool {
    let seen = matches!(get(w, e, &NEAREST_WANTED_ITEM), Some(MemValue::Who(Who::Entity(it))) if is_loved(w, *it));
    seen && !present(w, e, &ADMIRING_ITEM) && !present(w, e, &ADMIRING_DISABLED) && !present(w, e, &DISABLE_WALK_TO_ADMIRE_ITEM)
}

fn start(w: &mut World, e: Entity, _: &Ctx) {
    set(w, e, &ADMIRING_ITEM, MemValue::Bool(true), Some(ADMIRE_TICKS));
}

#[cfg(test)]
mod tests {
    #[test]
    fn admiration_lasts_119_ticks() {
        assert_eq!(super::ADMIRE_TICKS, 119);
    }
}
