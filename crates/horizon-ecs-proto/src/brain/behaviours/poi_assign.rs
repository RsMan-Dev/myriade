//! `AssignProfession` (`villager_jobs.rs:206`, `assign_profession`) and `ResetProfession` (`villager_jobs.rs:189`, `reset_profession`).
//! The potential site within 2 blocks becomes the job site: its claim moves to the job memory, the profession of the site is given
//! when the villager has none, and the villager gets the happy particles. The job site lost keeps the profession (the `Mob` does not
//! clear it): `ResetProfession` does, without a job site, with no experience and at most level 1, offers forgotten
//! (`villager.rs:415`, `set_profession`), NITWIT (11) and NONE (0) excluded (`villager.rs:19-20`).
use crate::behaviours::poi_claim::Claims;
use crate::behaviours::profession::{self, Profession};
use crate::behaviours::trade_offers::{self, Merchant};
use crate::brain::behaviours::poi_acquire::happy_particles;
use crate::brain::*;
use crate::cow::Pos;
use crate::rules::poi_assign as rules;
use horizon_world::poi::PoiType;

const NONE: u8 = 0;
const NITWIT: u8 = 11;

pub struct Params {
    pub job: &'static MemKey,
    pub potential: &'static MemKey,
    /// Métier donné par un point (`villager_professions.rs`, `job`) : nom du métier, résolu en index à l'attribution.
    pub professions: &'static [(PoiType, &'static str)],
}

fn site(w: &World, e: Entity, k: &MemKey) -> Option<[i32; 3]> {
    match get(w, e, k) {
        Some(MemValue::Block(p)) => Some(*p),
        _ => None,
    }
}

use crate::brain::refresh as refresh_brain;
fn keep(_: &mut World, _: Entity) {}

fn can_assign(w: &mut World, e: Entity, c: &Ctx) -> bool {
    let p = c.get::<Params>();
    let Some(s) = site(w, e, p.potential) else { return false };
    w.get::<Pos>(e).is_some_and(|at| rules::near(s, at.0))
}

fn assign(w: &mut World, e: Entity, c: &Ctx) {
    let p = c.get::<Params>();
    let Some(s) = site(w, e, p.potential) else { return };
    erase(w, e, p.potential);
    set(w, e, p.job, MemValue::Block(s), None);
    let mut refresh = false;
    if w.get::<Profession>(e).is_none() {
        profession::on_acquired(w, e, s);
        let kind = w.get::<Profession>(e).map(|j| j.0);
        let name = p.professions.iter().find(|(k, _)| Some(*k) == kind).map(|&(_, n)| n);
        if let Some(i) = name.and_then(|n| (0..=u8::MAX).find(|&i| trade_offers::profession_name(w, i).trim_start_matches("minecraft:") == n)) {
            trade_offers::set_profession(w, e, i);
            refresh = true;
        }
    }
    if let Some(mut cs) = w.get_mut::<Claims>(e) {
        for claim in cs.0.iter_mut().filter(|k| k.pos == s && std::ptr::eq(k.memory, p.potential)) {
            claim.memory = p.job;
            claim.on_lost = keep;
        }
    }
    happy_particles(w, e);
    // `AssignProfessionFromJobSite`: `setVillagerData` then `Villager.refreshBrain`.
    if refresh {
        refresh_brain(w, e);
    }
}

fn can_reset(w: &mut World, e: Entity, c: &Ctx) -> bool {
    let p = c.get::<Params>();
    !present(w, e, p.job) && trade_offers::profession_known(w, e) && w.get::<Merchant>(e).is_some_and(|m| m.profession != NONE && m.profession != NITWIT && m.trade.xp == 0 && m.level <= 1)
}

fn reset(w: &mut World, e: Entity, _: &Ctx) {
    trade_offers::set_profession(w, e, NONE);
    w.entity_mut(e).remove::<Profession>();
    // `ResetProfession`: `setVillagerData` then `Villager.refreshBrain`.
    refresh_brain(w, e);
}

pub static ASSIGN: Behaviour = Behaviour { name: "minecraft:assign_profession", one_shot: true, can_start: can_assign, start: assign, ..Behaviour::DEFAULT };
pub static RESET: Behaviour = Behaviour { name: "minecraft:reset_profession", one_shot: true, can_start: can_reset, start: reset, ..Behaviour::DEFAULT };
