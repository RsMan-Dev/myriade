//! The cost at rest of the dropped items and of the orbs: 100 000 of each on a floor with no player settle, and then nothing wakes
//! them. One player beside 1 000 orbs draws exactly those within 8 blocks. No reference here: the expected values are the counts
//! the scene gives, not a run of the code under test.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_client_lib::items::ItemStack;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::goals::*;
use horizon_ecs_proto::item::*;
use horizon_ecs_proto::orb::*;

const N: usize = 100_000;

fn awake(h: &mut Herd) -> (usize, usize, usize) {
    let items = h.world.query_filtered::<(), With<ItemAwake>>().iter(&h.world).count();
    let orbs = h.world.query_filtered::<(), With<OrbAwake>>().iter(&h.world).count();
    let moving = h.world.query_filtered::<(), With<Moving>>().iter(&h.world).count();
    (items, orbs, moving)
}

/// Stacks and orbs on a grid one block apart, all over the slab (an entity outside it would fall for ever): nothing merges, everything falls and lands.
fn floor(h: &mut Herd) {
    for i in 0..N {
        let at = Vec3::new((i % 316) as f64 - 158.0 + 0.5, 1.0, (i / 316) as f64 - 158.0 + 0.5);
        drop_item(&mut h.world, ItemStack::new("minecraft:sand", 1), at, Vec3::ZERO, 0);
        spawn_orb(&mut h.world, 1, 1, at, Vec3::ZERO);
    }
}

/// The 100 000 of each settle within a few seconds of game time, and then no item and no orb is awake or moving.
#[test]
fn a_floor_of_items_and_orbs_wakes_nothing_once_settled() {
    let mut h = Herd::with_slab(170, true, false);
    floor(&mut h);
    for _ in 0..60 {
        h.tick();
    }
    assert_eq!(awake(&mut h), (0, 0, 0));
    for _ in 0..200 {
        h.tick();
    }
    assert_eq!(awake(&mut h), (0, 0, 0), "a settled entity was woken again");
}

/// A player within 8 blocks of an orb attracts it; the others of a grid of 1 000 are not drawn, and they stay asleep.
#[test]
fn one_player_draws_the_orbs_within_eight_blocks_only() {
    let mut h = Herd::with_slab(64, true, false);
    let p = Vec3::new(0.5, 1.0, 0.5);
    h.world.resource_mut::<Online>().set(Player { id: 1, x: p.x, y: p.y, z: p.z, main: 0, off: 0, ..Default::default() });
    let mut within = 0;
    for i in 0..1000 {
        // Two blocks apart (orbs one block apart merge at their first look) and off the 8-block circle.
        let at = Vec3::new((i % 40) as f64 * 2.0 - 40.0 + 1.0, 1.0, (i / 40) as f64 * 2.0 - 24.0 + 1.0);
        let d2 = (at.x - p.x).powi(2) + (at.z - p.z).powi(2);
        if d2 < 64.0 {
            within += 1;
        }
        spawn_orb(&mut h.world, 1, 1, at, Vec3::ZERO);
    }
    for _ in 0..2 {
        h.tick();
    }
    let attracted = h.world.query_filtered::<(), With<Attracted>>().iter(&h.world).count();
    assert_eq!(attracted, within);
    let (_, orbs, _) = awake(&mut h);
    assert_eq!(orbs, within, "only the drawn orbs are awake");
}
