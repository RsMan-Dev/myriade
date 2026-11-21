//! `PoiCompetitorScan` (`villager_jobs.rs:111`, `scan_job_competitors`): among the villagers of the same profession that hold the same job
//! site, the most experienced keeps it (`selectWinner`: the second wins a tie). A villager that loses drops its job site memory. Runs when
//! the brain wakes; the neighbours come from the `MOBS` memory of the living-entities sensor.
use crate::behaviours::profession::Profession;
use crate::behaviours::trade_offers::Merchant;
use crate::brain::behaviours::poi_validate::forget;
use crate::brain::*;
use crate::poi::PoiRegistry;
use crate::rules::poi_competitor_scan::loses_to;
use crate::targeting::Who;
use bevy_ecs::prelude::*;

pub struct Params {
    pub job: &'static MemKey,
}

fn xp(w: &World, e: Entity) -> i32 {
    w.get::<Merchant>(e).map_or(0, |m| m.trade.xp)
}

fn site(w: &World, e: Entity, k: &MemKey) -> Option<[i32; 3]> {
    match get(w, e, k) {
        Some(MemValue::Block(p)) => Some(*p),
        _ => None,
    }
}

/// True when a competitor at least as experienced as the villager (in the order of the list) holds the same site.
fn loses(w: &World, e: Entity, job: &MemKey) -> bool {
    let (Some(&prof), Some(p)) = (w.get::<Profession>(e), site(w, e, job)) else { return false };
    if w.resource::<PoiRegistry>().get(p).is_none() {
        return false;
    }
    let Some(MemValue::Whos(list)) = get(w, e, &MOBS) else { return false };
    let rivals = list.iter().filter_map(|&who| {
        let Who::Entity(s) = who else { return None };
        (s != e && w.get::<Profession>(s) == Some(&prof) && site(w, s, job) == Some(p)).then(|| xp(w, s))
    });
    loses_to(xp(w, e), rivals)
}

fn can_start(w: &mut World, e: Entity, c: &Ctx) -> bool {
    loses(w, e, c.get::<Params>().job)
}

fn start(w: &mut World, e: Entity, c: &Ctx) {
    forget(w, e, c.get::<Params>().job);
}

pub static BEHAVIOUR: Behaviour = Behaviour { name: "minecraft:poi_competitor_scan", one_shot: true, can_start, start, ..Behaviour::DEFAULT };
