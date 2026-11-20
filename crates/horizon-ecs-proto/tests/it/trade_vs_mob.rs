//! Commerce d'un villageois : la même scène côté client (`Session` et `Trader`, code que le `Mob` exécute) et côté ECS
//! (`trade_offers`, `trade_session`, `trade_restock`). Ouverture, achat, offre épuisée, réapprovisionnement, montée de niveau,
//! sauvegarde. Garde comme `leash_vs_mob.rs` (`HORIZON_SPECIES=minecraft:villager`, `REGISTER_ALL`, `HORIZON_TEST_RESOURCES`).
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_client_lib::{
    items::{ItemStack, Items},
    merchant::{self, MerchantOffer, Session, TradeEnv, Trades},
};
use horizon_ecs_proto::behaviours::{
    schedule::Times,
    trade_offers::{self, Merchant, TradeData},
    trade_restock, trade_session,
};
use horizon_items::enchant::Enchantments;
use std::collections::BTreeMap;

const SEED: i64 = 0x5EED;

fn skip() -> bool {
    !have_data()
        || wrong_species(&["minecraft:villager"])
        || std::env::var_os("REGISTER_ALL").is_none()
}

/// Les registres du monde de test (items, enchantements) et le registre de commerce chargé des ressources.
pub(crate) fn registries() -> (&'static Items, &'static Enchantments, Trades) {
    let r = MobRef::new(1, false);
    let items = r.w.items.expect("items");
    let root = std::path::PathBuf::from(std::env::var("HORIZON_TEST_RESOURCES").expect("resources"));
    let defaults = horizon_world::item_defaults::read(&root).expect("item defaults");
    let enchantments: &'static Enchantments = Box::leak(Box::new(Enchantments::load(&root, &defaults).expect("enchantments")));
    let trades = Trades::load(&root, &horizon_world::loot::PackOverlay::NONE);
    (items, enchantments, trades)
}

/// Les offres telles qu'un joueur les voit : coût A, coût B, résultat.
type Shape = Vec<(String, u32, Option<String>, String, u32)>;
fn shape(offers: &[MerchantOffer]) -> Shape {
    offers
        .iter()
        .map(|o| {
            (
                o.base_cost_a.stack.item.to_string(),
                o.base_cost_a.stack.count,
                o.cost_b.as_ref().map(|c| c.stack.item.to_string()),
                o.result.item.to_string(),
                o.result.count,
            )
        })
        .collect()
}

/// Le paiement d'un usage de l'offre, du joueur qui a exactement ce qu'il faut.
fn payment(items: &Items, offer: &MerchantOffer) -> (Option<ItemStack>, Option<ItemStack>) {
    let mut a = offer.base_cost_a.stack.clone();
    a.count = offer.cost_a_count(items);
    (Some(a), offer.cost_b.as_ref().map(|c| c.stack.clone()))
}

/// Un monde ECS avec un villageois de ce métier, à ce niveau et cette expérience.
fn ecs_world(profession: u8, level: u8, xp: i32) -> (World, Entity, &'static Items) {
    let (items, enchantments, trades) = registries();
    let mut w = World::new();
    w.insert_resource(TradeData { trades, items, enchantments, seed: SEED, sequences: BTreeMap::new() });
    Times::set(&mut w, 0);
    let e = w.spawn(Merchant::new(false, profession, 0)).id();
    let mut m = w.get_mut::<Merchant>(e).unwrap();
    m.level = level;
    m.trade.xp = xp;
    drop(m);
    (w, e, items)
}

/// Le côté client : une `Session` sur les mêmes offres, avec son propre registre (chargé une seconde fois).
fn mob_session(offers: Vec<MerchantOffer>, xp: i32, level: i32) -> (Session, Trades, &'static Enchantments) {
    let (_, enchantments, trades) = registries();
    let mut s = Session::new(offers, true);
    s.xp = xp;
    s.level = level;
    (s, trades, enchantments)
}

fn mob_buy(s: &mut Session, trades: &Trades, enchantments: &Enchantments, items: &Items, index: usize) -> bool {
    let (mut a, mut b) = payment(items, &s.offers[index].clone());
    if !s.offers[index].take(items, &mut a, &mut b) {
        return false;
    }
    let env = TradeEnv { trades, enchantments, maps: None };
    s.notify_trade(index, Some(&env), items);
    true
}

fn ecs_buy(w: &mut World, e: Entity, items: &Items, index: usize) -> bool {
    let offer = w.get::<trade_session::Trading>(e).unwrap().session.offers[index].clone();
    let (mut a, mut b) = payment(items, &offer);
    trade_session::buy(w, e, index, &mut a, &mut b)
}

/// Les offres d'un métier et d'un niveau : celles du client, sur la même graine.
#[test]
fn the_offers_are_the_clients_for_each_level() {
    if skip() {
        return;
    }
    for level in 1..=5u8 {
        let (mut w, e, _) = ecs_world(0, level, 0);
        trade_offers::ensure_offers(&mut w, e);
        let ported = shape(&w.get::<Merchant>(e).unwrap().trade.offers);
        let (_, enchantments, trades) = registries();
        let profession = trades.professions.state(0).unwrap().id();
        let set = trades.set_for(&profession, i32::from(level));
        let seq = trades.sequence(&set).unwrap();
        let mut random = merchant::sequence_random(SEED, &seq);
        let ctx = merchant::TradeContext { items: registries().0, enchantments, variant: "minecraft:plains", origin: [0.0; 3], maps: None };
        let client = trades.offers_from_set(&ctx, &set, &mut random);
        assert_eq!(ported, shape(&client), "level {level}");
    }
}

/// L'écran : ouverture, achats jusqu'à épuisement de l'offre, refus de l'achat suivant, fermeture qui rend les offres. Le client
/// fait les mêmes achats sur sa `Session` et doit arriver au même état.
#[test]
fn open_buy_run_out_and_close_as_the_mob() {
    if skip() {
        return;
    }
    let (mut w, e, items) = ecs_world(0, 1, 0);
    trade_offers::ensure_offers(&mut w, e);
    let offers = w.get::<Merchant>(e).unwrap().trade.offers.clone();
    let (mut mob, trades, enchantments) = mob_session(offers, 0, 1);
    trade_session::open(&mut w, e, 1);
    let max = w.get::<Merchant>(e).unwrap().trade.offers[0].max_uses;
    for _ in 0..max {
        assert!(ecs_buy(&mut w, e, items, 0));
        assert!(mob_buy(&mut mob, &trades, enchantments, items, 0));
    }
    assert!(!ecs_buy(&mut w, e, items, 0), "une offre épuisée ne se vend pas");
    assert!(!mob_buy(&mut mob, &trades, enchantments, items, 0));
    trade_session::close(&mut w, e);
    let m = w.get::<Merchant>(e).unwrap();
    assert_eq!(m.trade.offers[0].uses, max);
    assert_eq!(m.trade.xp, mob.xp, "même expérience après les échanges");
    assert_eq!(shape(&m.trade.offers), shape(&mob.offers));
}

/// Une expérience qui atteint le seuil : le niveau monte, côté ECS comme côté client.
#[test]
fn a_trade_that_fills_the_experience_levels_up() {
    if skip() {
        return;
    }
    let threshold = merchant::max_xp(1);
    let (mut w, e, items) = ecs_world(0, 1, threshold);
    trade_offers::ensure_offers(&mut w, e);
    let offers = w.get::<Merchant>(e).unwrap().trade.offers.clone();
    let (mut mob, trades, enchantments) = mob_session(offers, threshold, 1);
    trade_session::open(&mut w, e, 1);
    assert!(ecs_buy(&mut w, e, items, 0));
    assert!(mob_buy(&mut mob, &trades, enchantments, items, 0));
    trade_session::close(&mut w, e);
    let m = w.get::<Merchant>(e).unwrap();
    assert_eq!(i32::from(m.level), mob.level, "même niveau après l'échange");
    assert_eq!(m.level, 2);
}

/// Le réapprovisionnement : la première fois il est permis tout de suite ; la seconde fois, il faut attendre plus que l'intervalle
/// du client. On compare la décision et l'état à ceux du `Trader` du client, pas à une constante recopiée.
#[test]
fn the_restock_follows_the_clients_trader() {
    if skip() {
        return;
    }
    let (mut w, e, items) = ecs_world(0, 1, 0);
    trade_offers::ensure_offers(&mut w, e);
    trade_session::open(&mut w, e, 1);
    assert!(ecs_buy(&mut w, e, items, 0));
    trade_session::close(&mut w, e);
    let mut mob = w.get::<Merchant>(e).unwrap().trade.clone();
    for game in [0_i64, 1, 1_000, 5_000, 30_000] {
        let ported = trade_restock::due_at(&mut w, e, game);
        assert_eq!(ported, mob.should_restock(game, 0), "due at {game}");
        if ported {
            trade_restock::restock_at(&mut w, e, game);
            mob.restock(game);
        }
        assert_eq!(w.get::<Merchant>(e).unwrap().trade.offers[0].uses, mob.offers[0].uses, "uses at {game}");
    }
}

/// La sauvegarde garde expérience, réapprovisionnement et offres, et les relit à l'identique.
#[test]
fn the_trade_fields_survive_a_save() {
    if skip() {
        return;
    }
    let (mut w, e, items) = ecs_world(0, 1, 0);
    trade_offers::ensure_offers(&mut w, e);
    trade_session::open(&mut w, e, 1);
    assert!(ecs_buy(&mut w, e, items, 0));
    trade_session::close(&mut w, e);
    let saved = trade_offers::save(&w, e);
    let read = trade_offers::read(|k| saved.iter().find(|(n, _)| *n == k).map(|(_, t)| t)).expect("champs de commerce");
    let m = w.get::<Merchant>(e).unwrap();
    assert_eq!(read.xp, m.trade.xp);
    assert_eq!(read.restocks_today, m.trade.restocks_today);
    assert_eq!(read.last_restock, m.trade.last_restock);
    assert_eq!(shape(&read.offers), shape(&m.trade.offers));
}
