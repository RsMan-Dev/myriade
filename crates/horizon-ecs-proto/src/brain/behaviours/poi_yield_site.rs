//! `YieldJobSite` (`villager_jobs.rs:150`, `yield_job_site`): a villager without a profession and without a job site gives up its potential
//! site when a nearby villager of the site's profession holds no site or this one. Its walk, its look and the site go. Runs when the brain
//! wakes; the neighbours come from the `MOBS` memory.
use crate::behaviours::profession::Profession;
use crate::brain::behaviours::poi_validate::forget;
use crate::brain::*;
use crate::goals::stop_nav;
use crate::poi::PoiRegistry;
use crate::targeting::Who;

pub struct Params {
    pub job: &'static MemKey,
    pub potential: &'static MemKey,
    pub baby: fn(&World, Entity) -> bool,
}

fn site(w: &World, e: Entity, k: &MemKey) -> Option<[i32; 3]> {
    match get(w, e, k) {
        Some(MemValue::Block(p)) => Some(*p),
        _ => None,
    }
}

fn yields(w: &World, e: Entity, c: &Params) -> bool {
    let Some(p) = site(w, e, c.potential) else { return false };
    if present(w, e, c.job) || w.get::<Profession>(e).is_some() || (c.baby)(w, e) {
        return false;
    }
    let Some(poi) = w.resource::<PoiRegistry>().get(p) else { return false };
    let Some(MemValue::Whos(list)) = get(w, e, &MOBS) else { return false };
    list.iter().any(|&who| {
        let Who::Entity(s) = who else { return false };
        // A nearby villager of that profession with no site, or with this one, takes it over.
        !present(w, s, c.potential)
            && w.get::<Profession>(s) == Some(&Profession(poi.kind))
            && site(w, s, c.job).is_none_or(|q| q == p)
    })
}

fn can_start(w: &mut World, e: Entity, c: &Ctx) -> bool {
    yields(w, e, c.get::<Params>())
}

fn start(w: &mut World, e: Entity, c: &Ctx) {
    erase(w, e, &WALK_TARGET);
    erase(w, e, &LOOK_TARGET);
    forget(w, e, c.get::<Params>().potential);
    stop_nav(w, e);
}

pub static BEHAVIOUR: Behaviour = Behaviour { name: "minecraft:yield_job_site", one_shot: true, can_start, start, ..Behaviour::DEFAULT };
