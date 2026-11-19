//! L'écran de commerce d'un marchand (`Villager.mobInteract` vers `MerchantMenu`, et les buts `TradeWithPlayer` et
//! `LookAtTradingPlayer`). Ouvert par l'interaction du joueur, fermé par sa fermeture ou par la fin du marchand. Le composant
//! `Trading` n'existe que pendant l'écran (épars : aucun parcours ne le trouve). Le `Session` du client garde les règles du
//! commerce ; ses événements partent vers le monde.
use crate::cow::now_of;
use super::gossip::{self, Gossips, Kind, PER_TRADE};
use super::trade_offers::{ensure_offers, lend, profession_name, variant_name, Merchant, TradeData};
use super::*;
use crate::cow::{Contact, FluidState, GrowAt, Health, HurtAt, Pos};
use crate::goals::*;
use bevy_ecs::prelude::*;
use horizon_client_lib::{
    items::ItemStack,
    merchant::{Event, Session, TradeEnv},
};
use horizon_core::random::XoroshiroRandom;
use std::collections::BTreeMap;

/// ASSUMED : `invulnerableTime` de vanilla après un coup, 10 ticks ; pas de constante nommée côté ECS (`cow.rs` le documente).
const HURT_INVULNERABLE_TICKS: u64 = 10;
/// `TradeWithPlayer` : le joueur à 16 blocs au plus (au carré).
const TRADE_RANGE2: f64 = 256.0;

/// Le joueur qui commerce et le côté marchand de l'écran ; `since` est le tick d'ouverture.
#[derive(Component)]
#[component(storage = "SparseSet")]
pub struct Trading {
    pub player: u32,
    pub session: Session,
    pub since: u64,
}

pub static MODULE: Module = Module {
    name: "trade",
    flags: MOVE | LOOK,
    can_use,
    start,
    stop,
    interact,
    requires: &[Cap::Body],
    ..Module::DEFAULT
};

/// `TradeWithPlayer.canUse` : le joueur est défini et présent, le marchand vit, n'a pas été touché, n'est pas dans l'eau, est au
/// sol, et le joueur est à portée.
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let Some(player) = w.get::<Trading>(e).map(|t| t.player) else { return false };
    let Some(p) = w.resource::<Online>().get(player) else { return false };
    let now = now_of(w, e);
    let hurt = w.get::<HurtAt>(e).is_some_and(|h| h.at + HURT_INVULNERABLE_TICKS > now);
    let alive = w.get::<Health>(e).is_some_and(|h| h.0 > 0.0);
    let on_ground = w.get::<Contact>(e).is_some_and(|c| c.on_ground);
    let in_water = w.get::<FluidState>(e).is_some_and(|f| f.in_water);
    let pos = w.get::<Pos>(e).map_or([0.0; 3], |q| [q.0.x, q.0.y, q.0.z]);
    let d2 = (p.x - pos[0]).powi(2) + (p.y - pos[1]).powi(2) + (p.z - pos[2]).powi(2);
    alive && !hurt && !in_water && on_ground && d2 <= TRADE_RANGE2
}

fn start(w: &mut World, e: Entity, _: Inst) {
    stop_nav(w, e);
}

fn stop(w: &mut World, e: Entity, _: Inst) {
    stop_nav(w, e);
}

/// `Villager.mobInteract` : un marchand qui n'est pas un bébé et n'a pas d'écran ouvert pour un autre joueur ouvre le sien.
fn interact(w: &mut World, e: Entity, _: Inst, i: &Interaction) -> bool {
    if w.get::<Merchant>(e).is_none() || w.get::<GrowAt>(e).is_some() {
        return false;
    }
    if w.get::<Trading>(e).is_some_and(|t| t.player != i.player) {
        return true;
    }
    open(w, e, i.player);
    true
}

/// Ouvre l'écran : fait les offres si elles ne le sont pas, copie offres, expérience, niveau, réputation et amplificateur de
/// Héros du village du joueur dans la session, prête les séquences des niveaux à venir, et réveille le but de commerce.
pub fn open(w: &mut World, e: Entity, player: u32) {
    ensure_offers(w, e);
    let Some(m) = w.get::<Merchant>(e) else { return };
    let (offers, xp, level, wandering, profession, variant) =
        (m.trade.offers.clone(), m.trade.xp, i32::from(m.level), m.wandering, m.profession, m.variant);
    let origin = w.get::<Pos>(e).map_or([0.0; 3], |p| [p.0.x, p.0.y, p.0.z]);
    let profession = profession_name(w, profession);
    let reputation = reputation_of(w, e, player);
    let hero = player_hero(w, player);
    let sequences = lend_for_levels(w, &profession, level);
    let mut session = Session::new(offers, !wandering);
    session.xp = xp;
    session.level = level;
    session.profession = profession;
    session.variant = variant_name(variant);
    session.origin = origin;
    session.sequences = sequences;
    session.reputation = reputation;
    session.hero = hero;
    session.update_special_prices();
    let since = now_of(w, e);
    w.entity_mut(e).insert(Trading { player, session, since });
    wake(w, e, &MODULE);
}

/// `Session::sequences_needed` : les séquences des niveaux au-dessus de celui-ci, prêtées par le monde (`WorldSequences.lend`).
fn lend_for_levels(w: &mut World, profession: &str, level: i32) -> BTreeMap<String, XoroshiroRandom> {
    let needed = Session::sequences_needed(&w.resource::<TradeData>().trades, profession, level);
    let mut d = w.resource_mut::<TradeData>();
    let seed = d.seed;
    needed
        .into_iter()
        .map(|id| {
            let r = lend(&mut d.sequences, seed, &id);
            (id, r)
        })
        .collect()
}

/// Le joueur achète l'offre `index` (`MerchantMenu.clickedOffer`) : l'offre prend le paiement dans ses deux emplacements (`a`,
/// `b`), puis la session fait l'échange. Un échange donne un commérage de commerce au marchand, et des cœurs.
pub fn buy(w: &mut World, e: Entity, index: usize, a: &mut Option<ItemStack>, b: &mut Option<ItemStack>) -> bool {
    let Some(mut t) = w.entity_mut(e).take::<Trading>() else { return false };
    let (done, events) = {
        let d = w.resource::<TradeData>();
        let paid = t.session.offers.get(index).is_some_and(|o| o.take(d.items, a, b));
        if paid {
            let env = TradeEnv { trades: &d.trades, enchantments: d.enchantments, maps: None };
            t.session.notify_trade(index, Some(&env), d.items);
        }
        (paid, std::mem::take(&mut t.session.events))
    };
    let traded = events.iter().any(|ev| matches!(ev, Event::Traded(..)));
    if traded {
        gossip::add(w, e, t.player, Kind::Trading, PER_TRADE);
        t.session.reputation = reputation_of(w, e, t.player);
        t.session.update_special_prices();
    }
    w.entity_mut(e).insert(t);
    emit(w, e, events);
    if traded {
        hearts(w, e);
    }
    done
}

/// Une réputation a changé pendant l'écran (`onReputationEventFrom`) : les prix du joueur sont recalculés.
pub fn refresh_prices(w: &mut World, e: Entity) {
    let Some(player) = w.get::<Trading>(e).map(|t| t.player) else { return };
    let reputation = reputation_of(w, e, player);
    let hero = player_hero(w, player);
    let Some(mut t) = w.get_mut::<Trading>(e) else { return };
    t.session.reputation = reputation;
    t.session.hero = hero;
    t.session.update_special_prices();
}

/// Le joueur ferme l'écran : offres, expérience et niveau reviennent au marchand, les séquences reviennent au monde, l'écran et son
/// but se terminent.
pub fn close(w: &mut World, e: Entity) {
    let Some(t) = w.entity_mut(e).take::<Trading>() else { return };
    if let Some(mut m) = w.get_mut::<Merchant>(e) {
        m.trade.offers = t.session.offers;
        m.trade.xp = t.session.xp;
        m.level = u8::try_from(t.session.level).unwrap_or(m.level);
    }
    let mut d = w.resource_mut::<TradeData>();
    for (id, r) in t.session.sequences {
        d.sequences.insert(id, r);
    }
    end(w, e, &MODULE);
}

/// `Villager.getReputation` du joueur `player` : la somme pondérée de ses commérages chez ce villageois (zéro sans commérage).
pub fn reputation_of(w: &World, e: Entity, player: u32) -> i32 {
    w.get::<Gossips>(e).map_or(0, |g| g.reputation(player))
}

/// L'amplificateur de Héros du village du joueur. ASSUMED : `effects.rs` n'a pas de lecture d'amplificateur (`effects::add` seul) ;
/// None tant qu'elle n'existe pas, donc pas de remise héros.
pub fn player_hero(_w: &World, _player: u32) -> Option<i32> {
    None
}

/// Envoie les événements de la session vers le monde : orbes d'expérience, régénération à la montée de niveau, sons du commerce.
fn emit(w: &mut World, e: Entity, events: Vec<Event>) {
    let at = w.get::<Pos>(e).map_or([0.0; 3], |p| [p.0.x, p.0.y, p.0.z]);
    for ev in events {
        match ev {
            Event::Orb(v) => {
                let p = w.get::<Pos>(e).map_or_else(Default::default, |p| p.0);
                crate::orb::spawn_orb(w, v, 1, p, Default::default());
            }
            Event::LevelUp(_) => {
                crate::effects::add(w, e, crate::effects::Kind::Regeneration, 0, 200);
            }
            Event::TradeUpdated(ok, pitch) => {
                let name = if ok { "entity.villager.yes" } else { "entity.villager.no" };
                w.resource_mut::<Events>().host.sound(name, at, 1.0, pitch);
            }
            Event::TradeSound => w.resource_mut::<Events>().host.sound("entity.villager.trade", at, 1.0, 1.0),
            Event::OffersRefreshed | Event::Traded(..) => {}
        }
    }
}

/// Le statut 14 d'un villageois après un échange : les cœurs (`particles::entity_event_burst`, table du client de rendu).
fn hearts(w: &mut World, e: Entity) {
    let Some((kind, burst)) = horizon_client_render::particles::entity_event_burst("minecraft:villager", 14) else { return };
    let at = w.get::<Pos>(e).map_or([0.0; 3], |p| [p.0.x, p.0.y, p.0.z]);
    let speed = burst.speed.unwrap_or(0.0);
    w.resource_mut::<Events>().host.particles(kind, [at[0], at[1] + burst.y, at[2]], [0.5; 3], speed, burst.count);
}
