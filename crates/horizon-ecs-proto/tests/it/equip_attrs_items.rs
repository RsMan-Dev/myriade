//! The modifiers of the worn items come with the slot change (put on, swapped, taken off) and go with it; nothing runs per tick.
use bevy_ecs::prelude::*;
use horizon_client_lib::items::ItemStack;
use horizon_ecs_proto::attr_mod::{Modifier, Operation};
use horizon_ecs_proto::equip_attrs::{of_attribute, ItemModifiers, WornModifiers};
use horizon_ecs_proto::equip_stack;

const MAINHAND: usize = 0;
const HEAD: usize = 5;

fn world() -> World {
    let mut w = World::new();
    w.insert_resource(ItemModifiers(Box::new(|stack, slot| match (stack.item.as_ref(), slot) {
        ("test:blade", MAINHAND) => {
            vec![("attack_damage".into(), Modifier { id: "test:blade".into(), amount: 5.0, operation: Operation::AddValue, permanent: false })]
        }
        ("test:helmet", HEAD) => {
            vec![("armor".into(), Modifier { id: "test:helmet".into(), amount: 2.0, operation: Operation::AddValue, permanent: false })]
        }
        _ => Vec::new(),
    })));
    w
}

#[test]
fn a_held_item_modifies_the_attribute_until_it_is_taken_away() {
    let mut w = world();
    let e = w.spawn_empty().id();
    equip_stack::set(&mut w, e, MAINHAND, Some(ItemStack::new("test:blade", 1)));
    let got = of_attribute(&w, e, "attack_damage");
    assert_eq!(got, vec![Modifier { id: "test:blade".into(), amount: 5.0, operation: Operation::AddValue, permanent: false }]);
    equip_stack::set(&mut w, e, MAINHAND, None);
    assert!(of_attribute(&w, e, "attack_damage").is_empty());
    assert!(w.get::<WornModifiers>(e).is_none());
}

#[test]
fn a_swap_replaces_the_modifiers_of_that_slot_only() {
    let mut w = world();
    let e = w.spawn_empty().id();
    equip_stack::set(&mut w, e, HEAD, Some(ItemStack::new("test:helmet", 1)));
    equip_stack::set(&mut w, e, MAINHAND, Some(ItemStack::new("test:blade", 1)));
    equip_stack::set(&mut w, e, MAINHAND, Some(ItemStack::new("test:stick", 1)));
    assert!(of_attribute(&w, e, "attack_damage").is_empty());
    assert_eq!(of_attribute(&w, e, "armor").len(), 1);
}
