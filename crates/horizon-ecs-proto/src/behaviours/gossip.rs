//! Commérages du villageois (`GossipContainer`, `Villager.gossip`) : ce qu'il retient de chaque joueur, sa décroissance d'un
//! jour, l'échange à la rencontre. Indexé par identifiant de joueur (`u32`). Un villageois sans commérage n'a pas `Gossips` ;
//! la décroissance est planifiée par le souvenir `GOSSIP_DAY`, qui expire à l'échéance et réveille le cerveau.
use crate::brain::*;
use bevy_ecs::prelude::*;
use crate::cow::now_of;
use std::collections::BTreeMap;

pub use crate::rules::gossip::{DAY, HEARD_WAIT, Kind, PER_EVENT, PER_TRADE};
use crate::rules::gossip::{added_value, day_due, day_ttl, decayed_value, free_to_hear, reputation, transfer_picks, transferred_value};

pub static GOSSIP_DAY: MemKey = MemKey::new("horizon:gossip_day", Shape::Unit);

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
    pub fn players(&self) -> impl Iterator<Item = u32> + '_ {
        self.map.keys().copied()
    }
    pub fn get(&self, player: u32, kind: Kind) -> i32 {
        self.map.get(&player).map_or(0, |e| e[kind as usize])
    }
    /// `getReputation` : la somme pondérée de tous les types, pour ce joueur.
    pub fn reputation(&self, player: u32) -> i32 {
        self.map.get(&player).map_or(0, reputation)
    }
    /// `add` : additionne jusqu'au maximum du type, oublie sous 2.
    fn add(&mut self, player: u32, kind: Kind, amount: i32) {
        let e = self.map.entry(player).or_insert([0; 5]);
        let old = e[kind as usize];
        e[kind as usize] = added_value(old, amount, kind);
        if e.iter().all(|x| *x == 0) {
            self.map.remove(&player);
        }
    }
    /// `decay` : chaque type perd son montant quotidien.
    fn decay(&mut self) {
        self.map.retain(|_, e| {
            for k in Kind::ALL {
                e[k as usize] = decayed_value(e[k as usize], k);
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
        let values: Vec<(Kind, i32)> = entries.iter().map(|(_, k, v)| (*k, *v)).collect();
        transfer_picks(&values, draw).into_iter().map(|i| entries[i]).collect()
    }
}

/// Le souvenir de la décroissance : présent tant que la prochaine échéance n'est pas passée.
fn plan_day(w: &mut World, e: Entity, now: u64) {
    let Some(since) = w.get::<Gossips>(e).map(|g| g.since) else { return };
    if !present(w, e, &GOSSIP_DAY) {
        let ttl = day_ttl(since, now);
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
        super::defend_village_target::wake_golems(w, e);
    }
}

/// `Villager.gossip` : la rencontre de `a` et `b`. Rien si l'un des deux a entendu il y a moins de 1200 ticks. Sinon `a`
/// reçoit jusqu'à 10 commérages tirés chez `b`, affaiblis par le récit, et les deux notent le temps. Rend si elle a eu lieu
/// (l'appelant enchaîne `summon_defender::check`).
pub fn meet(w: &mut World, a: Entity, b: Entity) -> bool {
    let now = now_of(w, a);
    let free = |w: &World, e: Entity| w.get::<Heard>(e).is_none_or(|h| free_to_hear(h.0, now));
    if !free(w, a) || !free(w, b) {
        return false;
    }
    let picked = match w.get::<Gossips>(b).cloned() {
        Some(source) => crate::goals::with_rng(w, a, |r| source.pick(&mut |n| r.next_int(n))),
        None => Vec::new(),
    };
    for (player, kind, value) in picked {
        if let Some(decayed) = transferred_value(value, kind) {
            if w.get::<Gossips>(a).is_none() {
                w.entity_mut(a).insert(Gossips { map: BTreeMap::new(), since: now });
            }
            let mut g = w.get_mut::<Gossips>(a).unwrap();
            let slot = &mut g.map.entry(player).or_insert([0; 5])[kind as usize];
            *slot = (*slot).max(decayed);
        }
    }
    plan_day(w, a, now);
    super::defend_village_target::wake_golems(w, a);
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
        if day_due(g.since, now) {
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
