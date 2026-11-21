//! Le villageois travaille à son poste quand il est à moins de 1,73 bloc du centre du bloc (`WorkAtComposter`,
//! `villager_brain.rs:1636-1660`). Un coup : `can_start` tire et décide (toutes les 300 ticks, une fois sur deux),
//! `start` pose l'événement (`ComposterWork`, pour un villageois qui travaille au composteur) et réapprovisionne.
//! La conversion blé vers compost ou pain se fait côté hôte (`game/blocks/composter.rs`).
use bevy_ecs::prelude::Entity;
use bevy_ecs::prelude::World;
use super::block_reach::{centre_dist_sq, look_block};
use super::trade_restock;
use crate::brain::*;
use crate::rules::composter::{at_site, check_due};
use crate::cow::{now_of, Pos};
use crate::goals::with_rng;
use crate::species::villager::JOB_SITE;
use bevy_ecs::prelude::{Component, Resource};

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:villager_work_at_poi",
    mems: &[&JOB_SITE],
    one_shot: true,
    can_start,
    start,
    ..Behaviour::DEFAULT
};

pub struct Params {
    /// The job is the composter's (`composter` of the `Mob`): the work is an event for the host.
    pub composter: bool,
}

/// The last check of the work (`lastCheck`, ticks), absent until the first.
#[derive(Component, Default, Clone, Copy)]
pub struct LastCheck(pub u64);

/// The work events of this tick (the job site of each villager that worked at a composter), drained by the host.
#[derive(Resource, Default)]
pub struct ComposterWork(pub Vec<[i32; 3]>);

fn site(w: &World, e: Entity) -> Option<[i32; 3]> {
    match get(w, e, &JOB_SITE) {
        Some(MemValue::Block(p)) => Some(*p),
        _ => None,
    }
}

fn can_start(w: &mut World, e: Entity, _: &Ctx) -> bool {
    let Some(p) = site(w, e) else { return false };
    let now = now_of(w, e);
    let last = w.get::<LastCheck>(e).map_or(0, |l| l.0);
    if !check_due(now, last) {
        return false;
    }
    if with_rng(w, e, |r| r.next_int(2)) != 0 {
        return false;
    }
    w.entity_mut(e).insert(LastCheck(now));
    let Some(at) = w.get::<Pos>(e).map(|q| q.0) else { return false };
    at_site(centre_dist_sq(p, at))
}

fn start(w: &mut World, e: Entity, c: &Ctx) {
    let Some(p) = site(w, e) else { return };
    look_block(w, e, p);
    if c.get::<Params>().composter {
        if let Some(mut work) = w.get_resource_mut::<ComposterWork>() {
            work.0.push(p);
        }
    }
    trade_restock::on_work(w, e);
}
