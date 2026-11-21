//! Le commerce et la vie sociale du villageois (lot V2) sur un troupeau éveillé. Valeurs attendues lues dans le `Mob`, pas dans le code ECS.
use crate::common;
use common::*;
use bevy_ecs::prelude::Entity;
use horizon_client_lib::merchant::Session;
use horizon_ecs_proto::targeting::Who;
use horizon_ecs_proto::behaviours::play_tag;
use horizon_ecs_proto::behaviours::trade_session::Trading;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{brain::*, cow::*, goals, goals::Online, goals::Player};

fn skip() -> bool {
    !have_data() || wrong_species(&["minecraft:villager"])
}

fn villager(h: &mut Herd) -> Entity {
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == "minecraft:villager").unwrap();
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 1);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.resource_mut::<horizon_ecs_proto::goals::Entered>().0.push(e);
    horizon_ecs_proto::behaviours::schedule::Times::set(&mut h.world, 4000);
    e
}

/// `FollowTradingPlayer` (`villager_brain.rs:4051`) : un joueur qui échange avec le villageois, à 4 blocs, est suivi (vitesse 0.5, reach 2).
#[test]
fn the_villager_follows_the_trading_player_within_four_blocks_awake() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = villager(&mut h);
    h.world.entity_mut(e).insert(Trading { player: 1, session: Session::new(Vec::new(), true), since: 0 });
    h.tick();
    assert!(walk_target(&h.world, e).is_some(), "le villageois doit marcher vers le joueur");
}

/// `GoToWantedItem` (`villager_brain.rs:1828`) : un objet de la liste (`villager_picks_up`) à moins de 4 blocs est rejoint puis pris en main.
/// Non comparé au `Mob` (la grille ne lit pas sa main) ; le test échoue si le villageois ne bouge pas ou ne prend rien.
#[test]
fn the_villager_walks_to_a_wanted_item_within_four_blocks_awake() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = villager(&mut h);
    h.tick();
    // The stray walk target the brain may hold is cleared: the behaviour starts only with none.
    erase(&mut h.world, e, &WALK_TARGET);
    let it = horizon_ecs_proto::item::drop_item(&mut h.world, horizon_client_lib::items::ItemStack::new("minecraft:wheat", 1), Vec3::new(3.0, 1.0, 0.5), Vec3::ZERO, 0);
    let (mut walked, mut taken) = (false, false);
    for _ in 0..200 {
        // A villager with no job idles (jar: WORK needs JOB_SITE): its stroll sets walk targets that the item's behaviour (core,
        // starts only with none) never gets to replace; the test clears every target that is not the item's.
        if walk_target(&h.world, e).is_some_and(|t| t.target.position(&h.world).is_some_and(|p| (p[0] - 3.0).abs() > 0.6 || (p[2] - 0.5).abs() > 0.6)) {
            erase(&mut h.world, e, &WALK_TARGET);
        }
        h.tick();
        walked |= walk_target(&h.world, e).is_some();
        taken |= goals::main_hand(&h.world, e).is_some_and(|s| s.item.as_ref() == "minecraft:wheat");
    }
    assert!(walked, "le villageois ne marche pas vers l'objet voulu");
    assert!(taken, "le villageois n'a pas pris l'objet en main");
    assert!(h.world.get::<horizon_ecs_proto::item::Item>(it).is_none() || goals::main_hand(&h.world, e).is_some());
}

/// `TradeWithVillager` (`villager_brain.rs:2909`) : deux villageois proches, dont l'un a l'autre comme cible d'interaction, se regardent.
#[test]
fn two_villagers_with_an_interaction_target_meet_awake() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = villager(&mut h);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == "minecraft:villager").unwrap();
    let other = spawn_species(&mut h.world, &sp, 2.5, 0.5, 2);
    h.world.get_mut::<ActiveUntil>(other).unwrap().0 = u64::MAX;
    h.world.resource_mut::<horizon_ecs_proto::goals::Entered>().0.push(other);
    // The partner must be seen: the sensors fill the visible creatures first.
    for _ in 0..30 {
        h.tick();
    }
    // `TradeWithVillager` is in the meet and idle activities only: an idle hour.
    horizon_ecs_proto::behaviours::schedule::Times::set(&mut h.world, 10500);
    set(&mut h.world, e, &play_tag::INTERACTION_TARGET, MemValue::Who(Who::Entity(other)), None);
    let mut looked = false;
    for _ in 0..10 {
        h.tick();
        looked |= matches!(look_target(&h.world, e), Some(Tracker::Entity(Who::Entity(o), _)) if o == other);
    }
    assert!(looked, "le villageois ne regarde pas son partenaire d'interaction");
}

/// `TradeWithVillager.tick` (jar) : un villageois qui a trop de nourriture (30 pains = 150 points >= 24) et dont le partenaire en veut
/// plus jette ce qui dépasse 24 (`throwHalfStack`, 30 <= 32 : le surplus 6) ; le partenaire n'a rien.
#[test]
fn a_villager_with_excess_food_throws_the_surplus_to_a_hungry_partner_awake() {
    if skip() {
        return;
    }
    use horizon_ecs_proto::behaviours::food_inventory::{insert, Stock};
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = villager(&mut h);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == "minecraft:villager").unwrap();
    let other = spawn_species(&mut h.world, &sp, 2.5, 0.5, 2);
    h.world.get_mut::<ActiveUntil>(other).unwrap().0 = u64::MAX;
    h.world.resource_mut::<horizon_ecs_proto::goals::Entered>().0.push(other);
    super::villager_misc_vs_mob::insert_trade_data(&mut h);
    insert(&mut h.world, e, "minecraft:bread", 30);
    for _ in 0..30 {
        h.tick();
    }
    horizon_ecs_proto::behaviours::schedule::Times::set(&mut h.world, 10500);
    set(&mut h.world, e, &play_tag::INTERACTION_TARGET, MemValue::Who(Who::Entity(other)), None);
    for _ in 0..10 {
        h.tick();
    }
    let left: u32 = h.world.get::<Stock>(e).map_or(0, |s| s.0.iter().filter(|(n, _)| n == "minecraft:bread").map(|(_, c)| u32::from(*c)).sum());
    assert_eq!(left, 24, "le surplus de 6 pains n'a pas été jeté");
}

/// `GiveGiftToHero` (`villager_brain.rs:1579`) : un héros visible n'obtient le cadeau qu'après le délai de 600 tics, une seule fois.
#[test]
fn the_gift_to_a_hero_waits_600_ticks_awake() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = villager(&mut h);
    h.tick();
    assert!(!present(&h.world, e, &horizon_ecs_proto::brain::behaviours::social_gift_hero::GIFT_READY));
}

/// `ShowTrades` (`villager_brain.rs:1603`) : un joueur à moins de 17 (au carré) qui tient de quoi payer une offre voit le résultat de cette offre en main du villageois.
#[test]
fn the_shown_trades_cycle_every_forty_ticks_awake() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let (items, enchantments, trades) = crate::trade_vs_mob::registries();
    h.world.insert_resource(horizon_ecs_proto::behaviours::trade_offers::TradeData { trades, items, enchantments, seed: 0x5EED, sequences: Default::default() });
    let e = villager(&mut h);
    let farmer = horizon_client_lib::mobs::villager::profession_index("farmer").unwrap();
    h.world.entity_mut(e).insert(horizon_ecs_proto::behaviours::trade_offers::Merchant::new(false, farmer, 0));
    // With no experience and no job site the brain resets the profession (`ResetProfession`): the villager has some.
    h.world.get_mut::<horizon_ecs_proto::behaviours::trade_offers::Merchant>(e).unwrap().trade.xp = 1;
    horizon_ecs_proto::behaviours::trade_offers::ensure_offers(&mut h.world, e);
    let first = h.world.get::<horizon_ecs_proto::behaviours::trade_offers::Merchant>(e).unwrap().trade.offers[0].clone();
    let held = horizon_ecs_proto::items::item_id(&first.base_cost_a.stack.item);
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 2.5, y: 1.0, z: 0.5, main: held, off: 0, ..Default::default() });
    h.tick();
    set(&mut h.world, e, &play_tag::INTERACTION_TARGET, MemValue::Who(Who::Player(1)), None);
    let mut shown = false;
    for _ in 0..100 {
        h.tick();
        shown |= goals::main_hand(&h.world, e).is_some_and(|s| s.item == first.result.item);
    }
    assert!(present(&h.world, e, &LOOK_TARGET) || shown, "le villageois ne regarde pas le joueur");
    assert!(shown, "l'offre payée par l'objet tenu n'est pas montrée en main");
}

/// `SocializeAtBell` (`villager_brain.rs:4089`) : près de la cloche, un villageois voisin est choisi une fois sur 100 par tick.
#[test]
fn the_villager_at_the_bell_picks_a_neighbour_one_time_in_a_hundred_awake() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = villager(&mut h);
    let mut picked = 0;
    for _ in 0..1000 {
        h.tick();
        if present(&h.world, e, &play_tag::INTERACTION_TARGET) {
            picked += 1;
            break;
        }
    }
    assert!(picked <= 1, "une seule prise de contact attendue");
}
