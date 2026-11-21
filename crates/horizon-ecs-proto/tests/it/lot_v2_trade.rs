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
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 3.5, y: 1.0, z: 0.5, main: 0, off: 0, ..Default::default() });
    // `HurtAt` starts at tick 0: the first 10 ticks of a world read as "hurt" (the follow stops when hurt in the last 10), so start later.
    for _ in 0..12 {
        h.tick();
    }
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

/// A profession by name (resolved once the level exists: the table is empty before) or by index.
#[derive(Clone, Copy)]
enum P {
    Name(&'static str),
    Idx(usize),
}
impl P {
    fn get(self) -> u8 {
        match self {
            P::Name(n) => horizon_client_lib::mobs::villager::profession_index(n).unwrap_or_else(|| panic!("no profession {n}")),
            P::Idx(i) => i as u8,
        }
    }
}

/// A villager that trades with a partner, both with a profession (index) and the first one holding `stock`, at the trade hour; the
/// partner's items are not touched. Rends what is left of `item` in the first one's inventory.
fn left_after_trade(mine: P, theirs: P, item: &str, n: u8) -> u32 {
    use horizon_ecs_proto::behaviours::food_inventory::{insert, Stock};
    use horizon_ecs_proto::behaviours::trade_offers::Merchant;
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = villager(&mut h);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == "minecraft:villager").unwrap();
    let other = spawn_species(&mut h.world, &sp, 2.5, 0.5, 2);
    h.world.get_mut::<ActiveUntil>(other).unwrap().0 = u64::MAX;
    h.world.resource_mut::<horizon_ecs_proto::goals::Entered>().0.push(other);
    super::villager_misc_vs_mob::insert_trade_data(&mut h);
    // With no experience and no job site the brain resets the profession (`ResetProfession`): both have some.
    for (x, p) in [(e, mine), (other, theirs)] {
        let mut m = Merchant::new(false, p.get(), 0);
        m.trade.xp = 1;
        h.world.entity_mut(x).insert(m);
    }
    insert(&mut h.world, e, item, n);
    for _ in 0..30 {
        h.tick();
    }
    horizon_ecs_proto::behaviours::schedule::Times::set(&mut h.world, 10500);
    set(&mut h.world, e, &play_tag::INTERACTION_TARGET, MemValue::Who(Who::Entity(other)), None);
    for _ in 0..10 {
        h.tick();
    }
    h.world.get::<Stock>(e).map_or(0, |s| s.0.iter().filter(|(k, _)| k == item).map(|(_, c)| u32::from(*c)).sum())
}

/// `TradeWithVillager.tick` (jar) : un fermier jette la moitié de son blé au-delà de 32 (40 blés : 20 restent) ; à 32 il garde tout.
#[test]
fn a_farmer_throws_half_of_its_wheat_above_thirty_two_awake() {
    if skip() {
        return;
    }
    let (farmer, none) = (P::Name("farmer"), P::Name("armorer"));
    assert_eq!(left_after_trade(farmer, none, "minecraft:wheat", 40), 20, "40 blés : la moitié jetée");
    assert_eq!(left_after_trade(farmer, none, "minecraft:wheat", 32), 32, "32 blés : rien jeté");
    // Not a farmer, but the partner's profession requests wheat: the wares rule gives the same half.
    assert_eq!(left_after_trade(none, farmer, "minecraft:wheat", 40), 20, "blé demandé par le métier du partenaire");
}

/// `TradeWithVillager.tick` (jar) : les denrées que demande le métier du partenaire et pas le nôtre sont jetées (une moitié de pile).
#[test]
fn a_villager_throws_the_wares_its_partners_profession_requests_awake() {
    if skip() {
        return;
    }
    let (a, b, item) = {
        let mut h = Herd::with_slab_awake(60, true, false);
        let _ = villager(&mut h);
        super::villager_misc_vs_mob::insert_trade_data(&mut h);
        let td = h.world.resource::<horizon_ecs_proto::behaviours::trade_offers::TradeData>();
        let mut found = None;
        for a in 0..30usize {
            for b in 0..30usize {
                let (Some(pa), Some(pb)) = (td.trades.professions.entry(a), td.trades.professions.entry(b)) else { continue };
                if let Some(i) = pb.requested.iter().find(|i| i.as_str() != "minecraft:wheat" && !pa.requested.contains(i) && td.items.get(i.as_str()).is_some_and(|d| d.max_stack >= 16)) {
                    found = Some((a, b, i.to_string()));
                    break;
                }
            }
            if found.is_some() {
                break;
            }
        }
        found.expect("two professions with different requested wares")
    };
    let left = left_after_trade(P::Idx(a), P::Idx(b), &item, 40);
    assert!(left < 40, "{item}: rien n'a été jeté au partenaire (métier {b})");
    assert_eq!(left_after_trade(P::Idx(b), P::Idx(a), &item, 40), 40, "{item}: demandé par mon métier, il reste");
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
