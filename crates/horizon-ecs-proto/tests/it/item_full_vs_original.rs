//! The item on the ground against Horizon's own in the same world, for the case the other files do not hold: the void. Vanilla removes an
//! item that falls below the world (`Entity.checkBelowWorld`, `minY - 64`) at the tick Horizon does. `HORIZON_TEST_RESOURCES` set.
//! Not written here (the lot's list, left to a later pass): fire, cactus, lightning, pickup by a player.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_client_lib::items::ItemStack;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::item::*;
use horizon_ecs_proto::item_reference::*;

/// An item that starts below the world is gone at the tick the original removes it, and not before.
#[test]
fn an_item_below_the_world_is_removed_when_the_original_is() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(16, true, false);
    let mw = h.world.resource::<Level>().mw.0;
    let at = Vec3::new(0.5, -120.0, 0.5);
    let mut o = Original { mw, items: vec![original(1, ItemStack::new("minecraft:stick", 1), at, Vec3::ZERO, 0)] };
    let e = drop_item(&mut h.world, ItemStack::new("minecraft:stick", 1), at, Vec3::ZERO, 0);
    let (mut when_o, mut when_e) = (None, None);
    for t in 1..=120 {
        o.tick();
        h.tick();
        if when_o.is_none() && o.items.is_empty() {
            when_o = Some(t);
        }
        if when_e.is_none() && h.world.get_entity(e).is_err() {
            when_e = Some(t);
        }
    }
    eprintln!("void: original removed at tick {when_o:?}, ecs at {when_e:?}");
    assert!(when_o.is_some() && when_o == when_e, "{when_o:?} vs {when_e:?}");
}
