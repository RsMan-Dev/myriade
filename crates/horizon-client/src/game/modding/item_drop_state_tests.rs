//! `ItemDrop`'s item-entity state (arismoko/horizon#3423): `thrower`, `target`, `no_gravity` and `age` set by a
//! drop hook reach the item entity that joins the level and behave as vanilla's do; a drop left
//! as it was joins as before.
use horizon_client::{entities::ItemEntity, items::ItemStack, lan::playerdata::uuid_ints};
use horizon_core::geometry::Vec3;
use mod_api::ItemDrop;

use super::spill::{BlockItemDrop, DropState, prepare_item_random};

const KILLER: u128 = 0x1234;
const THROWER: u128 = 0x5678;

fn drop_with(state: DropState, random: bool) -> BlockItemDrop {
    BlockItemDrop {
        random: random.then(|| prepare_item_random(None)),
        stack: ItemStack::new("minecraft:rotten_flesh", 1),
        position: [0.5, 64.0, 0.5],
        velocity: [0.1, 0.2, 0.0],
        pickup_delay: 10,
        state,
    }
}

fn set() -> DropState {
    DropState {
        thrower: Some(THROWER),
        target: Some(KILLER),
        no_gravity: true,
        age: ItemDrop::UNLIMITED_LIFETIME,
    }
}

#[test]
fn an_unedited_drop_keeps_its_random_as_it_was() {
    let drawn = drop_with(DropState::default(), true);
    assert_eq!(drawn.random_with_state(), drawn.random);
    assert_eq!(
        drop_with(DropState::default(), false).random_with_state(),
        None
    );
}

#[test]
fn a_drop_with_state_carries_it_into_its_random() {
    let drawn = drop_with(set(), true);
    let random = drawn.random_with_state().unwrap();
    assert_eq!(random.state, set());
    assert_eq!(
        random.uuid,
        drawn.random.unwrap().uuid,
        "the drawn random is kept"
    );
    // An edited drop has no random yet: one is drawn with the drop's own motion.
    let random = drop_with(set(), false).random_with_state().unwrap();
    assert_eq!((random.state, random.velocity), (set(), [0.1, 0.2, 0.0]));
}

#[test]
fn the_state_reaches_the_item_entity() {
    let fresh = || {
        ItemEntity::new(
            1,
            ItemStack::new("minecraft:stone", 1),
            Vec3::ZERO,
            Vec3::ZERO,
            0.0,
        )
    };
    let item = set().apply(fresh());
    assert_eq!(item.thrower, Some(uuid_ints(THROWER)));
    assert_eq!(item.target, Some(uuid_ints(KILLER)));
    assert!(item.no_gravity);
    assert_eq!(item.age, -32768);
    // The default state is a fresh item entity's.
    let plain = DropState::default().apply(fresh());
    let new = fresh();
    assert_eq!(
        (plain.thrower, plain.target, plain.no_gravity, plain.age),
        (new.thrower, new.target, new.no_gravity, new.age)
    );
}
