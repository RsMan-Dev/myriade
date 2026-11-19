//! Commérages du villageois (`GossipContainer`, `Villager.gossip`) : ce qu'il retient de chaque joueur, sa décroissance d'un
//! jour, l'échange à la rencontre. Indexé par identifiant de joueur (`u32`). Un villageois sans commérage n'a pas `Gossips` ;
//! la décroissance est planifiée par le souvenir `GOSSIP_DAY`, qui expire à l'échéance et réveille le cerveau.
use crate::brain::*;
use bevy_ecs::prelude::*;
use crate::cow::now_of;
use std::collections::BTreeMap;

pub const DAY: u64 = 24000;
/// `Villager.gossip` : entendre à nouveau attend 1200 ticks, pour les deux.
pub const HEARD_WAIT: u64 = 1200;
pub const PER_EVENT: i32 = 25;
pub const PER_TRADE: i32 = 2;
const DISCARD: i32 = 2;
const TRANSFERRED: usize = 10;

pub static GOSSIP_DAY: MemKey = MemKey::new("horizon:gossip_day", Shape::Unit);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    MajorNegative,
    MinorNegative,
    MinorPositive,
    MajorPositive,
    Trading,
}
impl Kind {
    pub const ALL: [Kind; 5] = [Kind::MajorNegative, Kind::MinorNegative, Kind::MinorPositive, Kind::MajorPositive, Kind::Trading];
    pub fn weight(self) -> i32 {
        match self {
            Kind::MajorNegative => -5,
            Kind::MinorNegative => -1,
            Kind::MinorPositive | Kind::Trading => 1,
            Kind::MajorPositive => 5,
        }
    }
    pub fn max(self) -> i32 {
        match self {
            Kind::MajorNegative => 100,
            Kind::MinorNegative => 200,
            Kind::MinorPositive | Kind::Trading => 25,
            Kind::MajorPositive => 20,
        }
    }
    pub fn decay_per_day(self) -> i32 {
        match self {
            Kind::MajorNegative => 10,
            Kind::MinorNegative => 20,
            Kind::MinorPositive => 1,
            Kind::MajorPositive => 0,
            Kind::Trading => 2,
        }
    }
    pub fn decay_per_transfer(self) -> i32 {
        match self {
            Kind::MinorPositive => 5,
            _ => 20,
        }
    }
}

/// Par joueur, la valeur de chaque type (0 : aucun), et le temps du dernier décompte (`since`).
#[derive(Component, Clone, Debug, Default)]
#[component(storage = "SparseSet")]
pub struct Gossips {
    map: BTreeMap<u32, [i32; 5]>,
    since: u64,
}
/// Le temps de la dernière rencontre qui a donné un commérage (`last_gossip_time`).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Heard(pub u64);

impl Gossips {
    pub fn get(&self, player: u32, kind: Kind) -> i32 {
        self.map.get(&player).map_or(0, |e| e[kind as usize])
    }
    /// `getReputation` : la somme pondérée de tous les types, pour ce joueur.
    pub fn reputation(&self, player: u32) -> i32 {
        self.map.get(&player).map_or(0, |e| Kind::ALL.iter().map(|k| e[*k as usize] * k.weight()).sum())
    }
    /// `add` : additionne jusqu'au maximum du type, oublie sous 2.
    fn add(&mut self, player: u32, kind: Kind, amount: i32) {
        let e = self.map.entry(player).or_insert([0; 5]);
        let old = e[kind as usize];
        let sum = old + amount;
        let v = if old == 0 { amount } else if sum > kind.max() { kind.max().max(old) } else { sum };
        let v = v.min(kind.max());
        e[kind as usize] = if v < DISCARD { 0 } else { v };
        if e.iter().all(|x| *x == 0) {
            self.map.remove(&player);
        }
    }
    /// `decay` : chaque type perd son montant quotidien.
    fn decay(&mut self) {
        self.map.retain(|_, e| {
            for k in Kind::ALL {
                let v = e[k as usize];
                if v != 0 {
                    let next = v - k.decay_per_day();
                    e[k as usize] = if next < DISCARD { 0 } else { next };
                }
            }
            e.iter().any(|x| *x != 0)
        });
    }
    /// `selectGossipsForTransfer` : jusqu'à 10 tirages pondérés par la grandeur, un doublon compté une fois. `draw(n)` rend
    /// un entier dans `0..n`.
    fn pick(&self, draw: &mut dyn FnMut(i32) -> i32) -> Vec<(u32, Kind, i32)> {
        let entries: Vec<(u32, Kind, i32)> = self
            .map
            .iter()
            .flat_map(|(p, e)| Kind::ALL.into_iter().filter(|k| e[*k as usize] != 0).map(move |k| (*p, k, e[k as usize])))
            .collect();
        let mut ends = Vec::with_capacity(entries.len());
        let mut total = 0;
        for (_, k, v) in &entries {
            total += (v * k.weight()).abs();
            ends.push(total - 1);
        }
        let mut picked: Vec<usize> = Vec::new();
        if total > 0 {
            for _ in 0..TRANSFERRED {
                let index = match ends.binary_search(&draw(total)) {
                    Ok(i) | Err(i) => i,
                };
                if !picked.contains(&index) {
                    picked.push(index);
                }
            }
        }
        picked.into_iter().map(|i| entries[i]).collect()
    }
}

/// Le souvenir de la décroissance : présent tant que la prochaine échéance n'est pas passée.
fn plan_day(w: &mut World, e: Entity, now: u64) {
    let Some(since) = w.get::<Gossips>(e).map(|g| g.since) else { return };
    if !present(w, e, &GOSSIP_DAY) {
        let ttl = (since + DAY).saturating_sub(now).max(1);
        set(w, e, &GOSSIP_DAY, MemValue::Unit, Some(ttl));
    }
}

/// Un commérage du joueur `player` entre chez le villageois `e` : témoin d'un coup, échange, commerce. Le composant
/// apparaît au premier commérage et disparaît quand il est vide.
pub fn add(w: &mut World, e: Entity, player: u32, kind: Kind, amount: i32) {
    let now = now_of(w, e);
    if w.get::<Gossips>(e).is_none() {
        w.entity_mut(e).insert(Gossips { map: BTreeMap::new(), since: now });
    }
    let empty = {
        let mut g = w.get_mut::<Gossips>(e).unwrap();
        g.add(player, kind, amount);
        g.map.is_empty()
    };
    if empty {
        w.entity_mut(e).remove::<Gossips>();
    } else {
        plan_day(w, e, now);
    }
}

/// `Villager.gossip` : la rencontre de `a` et `b`. Rien si l'un des deux a entendu il y a moins de 1200 ticks. Sinon `a`
/// reçoit jusqu'à 10 commérages tirés chez `b`, affaiblis par le récit, et les deux notent le temps. Rend si elle a eu lieu
/// (l'appelant enchaîne `golem_summon::check`).
pub fn meet(w: &mut World, a: Entity, b: Entity) -> bool {
    let now = now_of(w, a);
    let free = |w: &World, e: Entity| w.get::<Heard>(e).is_none_or(|h| now < h.0 || now >= h.0 + HEARD_WAIT);
    if !free(w, a) || !free(w, b) {
        return false;
    }
    let picked = match w.get::<Gossips>(b).cloned() {
        Some(source) => crate::goals::with_rng(w, a, |r| source.pick(&mut |n| r.next_int(n))),
        None => Vec::new(),
    };
    for (player, kind, value) in picked {
        let decayed = value - kind.decay_per_transfer();
        if decayed >= DISCARD {
            if w.get::<Gossips>(a).is_none() {
                w.entity_mut(a).insert(Gossips { map: BTreeMap::new(), since: now });
            }
            let mut g = w.get_mut::<Gossips>(a).unwrap();
            let slot = &mut g.map.entry(player).or_insert([0; 5])[kind as usize];
            *slot = (*slot).max(decayed);
        }
    }
    plan_day(w, a, now);
    w.entity_mut(a).insert(Heard(now));
    w.entity_mut(b).insert(Heard(now));
    true
}

/// Décroissance planifiée : le souvenir `GOSSIP_DAY` absent (il a expiré, ou le villageois vient d'en recevoir un) et
/// des commérages présents. Le comportement tient en un coup : il décroît si le jour est écoulé et replanifie.
fn due(w: &mut World, e: Entity, _: &Ctx) -> bool {
    w.get::<Gossips>(e).is_some() && !present(w, e, &GOSSIP_DAY)
}
fn decay(w: &mut World, e: Entity, _: &Ctx) {
    let now = now_of(w, e);
    let empty = {
        let mut g = w.get_mut::<Gossips>(e).unwrap();
        if now >= g.since + DAY {
            g.decay();
            g.since = now;
        }
        g.map.is_empty()
    };
    if empty {
        w.entity_mut(e).remove::<Gossips>();
    } else {
        plan_day(w, e, now);
    }
}

pub static DECAY: Behaviour = Behaviour {
    name: "gossip:decay",
    mems: &[&GOSSIP_DAY],
    one_shot: true,
    can_start: due,
    start: decay,
    ..Behaviour::DEFAULT
};
