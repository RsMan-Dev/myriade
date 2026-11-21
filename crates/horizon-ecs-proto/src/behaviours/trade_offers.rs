//! État de commerce d'un villageois ou d'un marchand ambulant : les offres (`AbstractVillager.offers`), l'expérience et le niveau
//! (`Villager.villagerXp`), l'état de réapprovisionnement (`Trader`, du client), les offres tirées du registre avec les séquences
//! aléatoires du monde (`TradeData`), le nouveau métier (`Villager.setVillagerData`), et les champs de sauvegarde.
//! Passif : rien ne le fait tourner. `trade_session` (l'écran) et `trade_restock` (le réveil) l'appellent sur événement.
use super::schedule::Times;
use crate::cow::{now_of, Pos};
use bevy_ecs::prelude::*;
use horizon_client_lib::{
    entity_nbt::{offers_from_nbt, offers_to_nbt},
    items::Items,
    merchant::{self, MerchantOffer, Trader, TradeContext, Trades},
    mobs::render::VILLAGER_TYPES,
};
use crate::nbt::{owner, Field};
use horizon_core::{
    nbt::{Compound, Tag},
    random::XoroshiroRandom,
};
use horizon_items::enchant::Enchantments;
use std::{borrow::Cow, collections::BTreeMap};

/// Les registres de commerce du monde (chargés une fois, `'static`), sa graine et ses séquences aléatoires.
#[derive(Resource)]
pub struct TradeData {
    pub trades: Trades,
    pub items: &'static Items,
    pub enchantments: &'static Enchantments,
    pub seed: i64,
    /// Les séquences que le monde détient : une séquence qu'un écran ouvert a est absente (elle revient à la fermeture).
    pub sequences: BTreeMap<String, XoroshiroRandom>,
}

/// L'état de commerce d'un marchand. `wandering` : les trois ensembles du marchand ambulant, sans métier ; sinon `profession` est
/// un index des métiers actifs, `level` de 1 à 5, `variant` un index de `VILLAGER_TYPES`.
#[derive(Component, Clone, Debug)]
pub struct Merchant {
    pub trade: Trader,
    pub level: u8,
    pub profession: u8,
    pub variant: u8,
    pub wandering: bool,
}
impl Merchant {
    /// Offres pas encore faites (liste nulle de vanilla) : elles se font à la première demande.
    pub fn new(wandering: bool, profession: u8, variant: u8) -> Self {
        Self { trade: Trader { unset: true, ..Default::default() }, level: 1, profession, variant, wandering }
    }
}

const WANDERING_SETS: [&str; 3] = ["buying", "uncommon", "common"];

/// Identifiant `minecraft:` d'un type de villageois (`VILLAGER_TYPES`, par index).
pub fn variant_name(variant: u8) -> String {
    format!("minecraft:{}", VILLAGER_TYPES[usize::from(variant) % VILLAGER_TYPES.len()])
}

/// `Villager.getOffers` / `WanderingTrader.getOffers` : fait les offres d'un marchand qui ne les a pas encore, à partir de
/// l'ensemble de son métier et de son niveau (ou de ses trois ensembles ambulants). L'expérience est gardée.
pub fn ensure_offers(w: &mut World, e: Entity) {
    let Some(m) = w.get::<Merchant>(e) else { return };
    if !m.trade.unset {
        return;
    }
    let (wandering, level, profession, variant) = (m.wandering, i32::from(m.level), m.profession, m.variant);
    let origin = w.get::<Pos>(e).map_or([0.0; 3], |p| [p.0.x, p.0.y, p.0.z]);
    let variant = variant_name(variant);
    let sets: Vec<String> = if wandering {
        WANDERING_SETS.iter().map(|s| format!("minecraft:wandering_trader/{s}")).collect()
    } else {
        let name = profession_name(w, profession);
        vec![w.resource::<TradeData>().trades.set_for(&name, level)]
    };
    let offers: Vec<MerchantOffer> = sets.iter().flat_map(|set| offers_from_set(w, set, &variant, origin)).collect();
    if let Some(mut m) = w.get_mut::<Merchant>(e) {
        m.trade.offers = offers;
        m.trade.unset = false;
    }
}

/// `Villager.addOffersFromTradeSet` : les offres d'un ensemble, tirées de sa séquence, que le monde prête et reprend.
pub fn offers_from_set(w: &mut World, set: &str, variant: &str, origin: [f64; 3]) -> Vec<MerchantOffer> {
    let d = &mut *w.resource_mut::<TradeData>();
    let Some(sequence) = d.trades.sequence(set) else {
        return Vec::new();
    };
    let mut random = lend(&mut d.sequences, d.seed, &sequence);
    let ctx = TradeContext { items: d.items, enchantments: d.enchantments, variant, origin, maps: None };
    let out = d.trades.offers_from_set(&ctx, set, &mut random);
    d.sequences.insert(sequence, random);
    out
}

/// `WorldSequences.lend` : la séquence du monde pour cet identifiant, ou une nouvelle issue de la graine du monde.
pub fn lend(sequences: &mut BTreeMap<String, XoroshiroRandom>, seed: i64, id: &str) -> XoroshiroRandom {
    sequences.remove(id).unwrap_or_else(|| merchant::sequence_random(seed, id))
}

/// `Villager.setVillagerData` avec un nouveau métier : les offres sont refaites pour lui ; expérience et niveau sont gardés.
pub fn set_profession(w: &mut World, e: Entity, profession: u8) {
    let Some(mut m) = w.get_mut::<Merchant>(e) else { return };
    if m.profession != profession {
        m.profession = profession;
        m.trade.offers.clear();
        m.trade.unset = true;
    }
}

/// Identifiant `minecraft:` du métier pour le registre de commerce ; vide si l'index n'est pas un métier actif.
pub fn profession_name(w: &World, profession: u8) -> String {
    let d = w.resource::<TradeData>();
    d.trades.professions.state(usize::from(profession)).map(|s| s.id()).unwrap_or_default()
}

/// Les champs de commerce de la sauvegarde d'un marchand (`Villager.addAdditionalSaveData`) : `Xp`, `LastRestock`, `RestocksToday`, et
/// `Offers` quand elles sont faites. Le niveau est écrit dans `VillagerData` par la sauvegarde du métier. Déclarés au registre NBT.
pub const FIELDS: &[Field] = &[
    Field { key: Cow::Borrowed("Xp"), write: write_xp, read: read_xp, accepts: crate::nbt::accept::int, owner: Some(owner::<Merchant>) },
    Field { key: Cow::Borrowed("LastRestock"), write: write_last_restock, read: read_last_restock, accepts: crate::nbt::accept::long, owner: Some(owner::<Merchant>) },
    Field { key: Cow::Borrowed("RestocksToday"), write: write_restocks, read: read_restocks, accepts: crate::nbt::accept::int, owner: Some(owner::<Merchant>) },
    Field { key: Cow::Borrowed("Offers"), write: write_offers, read: read_offers, accepts: crate::nbt::accept::any, owner: Some(owner::<Merchant>) },
];

fn write_xp(w: &World, e: Entity, c: &mut Compound) {
    if let Some(m) = w.get::<Merchant>(e) {
        c.insert("Xp".into(), Tag::Int(m.trade.xp));
    }
}

fn write_last_restock(w: &World, e: Entity, c: &mut Compound) {
    if let Some(m) = w.get::<Merchant>(e) {
        c.insert("LastRestock".into(), Tag::Long(m.trade.last_restock));
    }
}

fn write_restocks(w: &World, e: Entity, c: &mut Compound) {
    if let Some(m) = w.get::<Merchant>(e) {
        c.insert("RestocksToday".into(), Tag::Int(m.trade.restocks_today));
    }
}

fn write_offers(w: &World, e: Entity, c: &mut Compound) {
    if let Some(m) = w.get::<Merchant>(e).filter(|m| !m.trade.unset) {
        c.insert("Offers".into(), offers_to_nbt(&m.trade.offers));
    }
}

/// Sets the trade state of a merchant; refused (false) when the entity has no `Merchant`.
fn set_trade(w: &mut World, e: Entity, f: impl FnOnce(&mut Trader)) -> bool {
    match w.get_mut::<Merchant>(e) {
        Some(mut m) => {
            f(&mut m.trade);
            true
        }
        None => false,
    }
}

fn read_xp(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Int(v) = t else { return false };
    set_trade(w, e, |tr| tr.xp = *v)
}

fn read_last_restock(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Long(v) = t else { return false };
    set_trade(w, e, |tr| tr.last_restock = *v)
}

fn read_restocks(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Int(v) = t else { return false };
    set_trade(w, e, |tr| tr.restocks_today = *v)
}

fn read_offers(w: &mut World, e: Entity, t: &Tag) -> bool {
    set_trade(w, e, |tr| {
        tr.offers = offers_from_nbt(t);
        tr.unset = false;
    })
}

/// Le temps de jeu (`now_of`, ASSUMED : temps de jeu du monde, à confirmer) et le jour (`Times.day`) que lit le réapprovisionnement.
pub fn clock(w: &World, e: Entity) -> (i64, i64) {
    (now_of(w, e) as i64, Times::absolute(w))
}
