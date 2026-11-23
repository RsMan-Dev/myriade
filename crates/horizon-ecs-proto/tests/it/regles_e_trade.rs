//! `Villager` trade display: the offer shown in the main hand has no drop chance, and clearing the display gives the default chance back
//! (`equipment_drop::DEFAULT`, 0.085). The player holds the item an offer pays with and opens the trade screen, as the host does
//! (`trade_session::open`, the host's `trade_open`); the origin test gives 0 while shown, 0.085 after.
use crate::common;
use common::*;
use horizon_ecs_proto::behaviours::trade_offers::{ensure_offers, Merchant};
use horizon_ecs_proto::behaviours::trade_session::open;
use horizon_ecs_proto::cow::{spawn_species, ActiveUntil, Herd, SpeciesTable};
use horizon_ecs_proto::equipment_drop::{chance_of, DEFAULT};
use horizon_ecs_proto::goals::{main_hand, Online, Player, Slot};

#[test]
fn a_shown_trade_has_no_drop_chance_and_clearing_it_restores_the_default() {
    if !have_data() || wrong_species(&["minecraft:villager"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    insert_trade_data(&mut h);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == "minecraft:villager").unwrap();
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 1);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    let farmer = myriade_engine_lib::mobs::villager::profession_index("farmer").unwrap();
    h.world.entity_mut(e).insert(Merchant::new(false, farmer, 0));
    h.world.get_mut::<Merchant>(e).unwrap().trade.xp = 1;
    ensure_offers(&mut h.world, e);
    let first = h.world.get::<Merchant>(e).unwrap().trade.offers[0].clone();
    let held = horizon_ecs_proto::items::item_id(&first.base_cost_a.stack.item);
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 2.5, y: 1.0, z: 0.5, main: held, off: 0, ..Default::default() });
    open(&mut h.world, e, 1);
    h.tick();
    assert!(main_hand(&h.world, e).is_some_and(|s| s.item == first.result.item), "the paid offer is shown in the main hand");
    assert!(chance_of(&h.world, e, Slot::Main).abs() < 1e-9, "a shown offer has no drop chance");
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 2.5, y: 1.0, z: 0.5, main: 0, off: 0, ..Default::default() });
    for _ in 0..3 {
        h.tick();
    }
    assert!((chance_of(&h.world, e, Slot::Main) - DEFAULT).abs() < 1e-6, "the default chance is back once the display clears");
}
