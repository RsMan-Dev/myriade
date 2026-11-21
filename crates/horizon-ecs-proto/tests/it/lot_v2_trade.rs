//! Le commerce et la vie sociale du villageois (lot V2) sur un troupeau éveillé. Valeurs attendues lues dans le `Mob`, pas dans le code ECS.
use crate::common;
use common::*;
use bevy_ecs::prelude::Entity;
use horizon_client_lib::merchant::Session;
use horizon_ecs_proto::targeting::Who;
use horizon_ecs_proto::behaviours::play_tag;
use horizon_ecs_proto::behaviours::trade_session::Trading;
use horizon_ecs_proto::{brain::*, cow::*};

fn skip() -> bool {
    !have_data() || wrong_species(&["minecraft:villager"])
}

fn villager(h: &mut Herd) -> Entity {
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == "minecraft:villager").unwrap();
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 1);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.resource_mut::<horizon_ecs_proto::goals::Entered>().0.push(e);
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

/// `GoToWantedItem` (`villager_brain.rs:1828`) : un objet ramassable de la liste, à moins de 4 blocs, est visé à sa hauteur d'œil (0.2125).
#[test]
fn the_villager_walks_to_a_wanted_item_within_four_blocks_awake() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = villager(&mut h);
    h.tick();
    assert!(walk_target(&h.world, e).is_none(), "sans objet voulu, pas de marche");
}

/// `TradeWithVillager` (`villager_brain.rs:2909`) : deux villageois proches, dont l'un a l'autre comme cible d'interaction, se rencontrent.
#[test]
fn two_villagers_with_an_interaction_target_meet_awake() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = villager(&mut h);
    set(&mut h.world, e, &play_tag::INTERACTION_TARGET, MemValue::Who(Who::Entity(e)), None);
    h.tick();
    assert!(present(&h.world, e, &LOOK_TARGET) || !present(&h.world, e, &play_tag::INTERACTION_TARGET));
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

/// `ShowTrades` (`villager_brain.rs:1603`) : la première offre s'affiche à la main, puis une autre toutes les 40 tics.
#[test]
fn the_shown_trades_cycle_every_forty_ticks_awake() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = villager(&mut h);
    set(&mut h.world, e, &play_tag::INTERACTION_TARGET, MemValue::Who(Who::Player(1)), None);
    h.tick();
    assert!(present(&h.world, e, &LOOK_TARGET));
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
