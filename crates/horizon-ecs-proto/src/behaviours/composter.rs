//! Le villageois travaille à son poste quand il est à moins de 1,73 bloc du centre du bloc (`WorkAtComposter`,
//! `villager_brain.rs:1636-1660`). Un coup : `can_start` tire et décide (toutes les 300 ticks, une fois sur deux),
//! `start` fait le pain (`makeBread`, dans l'inventaire du villageois) et pose `HostEvent::ComposterWork` quand le bloc du poste est un
//! composteur ; l'hôte applique l'effet sur le bloc (`villager_compost_items_ecs`, `game/blocks/composter.rs`) puis réapprovisionne.
use bevy_ecs::prelude::Entity;
use bevy_ecs::prelude::World;
use super::block_reach::{centre_dist_sq, look_block};
use crate::brain::behaviours::trade_restock_work;
use crate::brain::*;
use crate::rules::composter::{at_site, check_due};
use crate::cow::{game_now, Pos};
use crate::goals::with_rng;
use crate::species::villager::JOB_SITE;
use bevy_ecs::prelude::Component;

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

/// `WorkAtComposter.useWorkstation`: the block of the job site is a composter (read in the level, not from the species).
fn is_composter(w: &World, p: [i32; 3]) -> bool {
    use myriade_engine_lib::mobs::spawn_rules::SpawnLevel;
    let l = w.resource::<crate::cow::Level>().mw.0;
    &*l.facts().get(l.state(p[0], p[1], p[2])).name == "minecraft:composter"
}

/// `WorkAtComposter.makeBread`: unless the inventory holds more than 36 bread, up to three loaves from nine wheat (three per loaf);
/// what does not fit drops (`spawnAtLocation(.., 0.5)`).
fn make_bread(w: &mut World, e: Entity) {
    use super::food_inventory::{insert, take, Stock};
    const BREAD: &str = "minecraft:bread";
    const WHEAT: &str = "minecraft:wheat";
    let Some(stock) = w.get::<Stock>(e) else { return };
    let count = |item: &str| stock.0.iter().filter(|(n, _)| n == item).map(|(_, c)| u32::from(*c)).sum::<u32>();
    if count(BREAD) > 36 {
        return;
    }
    let loaves = (count(WHEAT) / 3).min(3);
    if loaves == 0 {
        return;
    }
    take(w, e, WHEAT, (loaves * 3) as u8);
    let left = insert(w, e, BREAD, loaves as u8);
    if left > 0 {
        let Some(at) = w.get::<Pos>(e).map(|q| q.0) else { return };
        crate::item::drop_item(w, myriade_engine_lib::items::ItemStack::new(BREAD, u32::from(left)), horizon_core::geometry::Vec3::new(at.x, at.y + 0.5, at.z), horizon_core::geometry::Vec3::new(0.0, 0.2, 0.0), 10);
    }
}

fn site(w: &World, e: Entity) -> Option<[i32; 3]> {
    match get(w, e, &JOB_SITE) {
        Some(MemValue::Block(p)) => Some(*p),
        _ => None,
    }
}

fn can_start(w: &mut World, e: Entity, _: &Ctx) -> bool {
    let Some(p) = site(w, e) else { return false };
    let now = game_now(w) as u64;
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

fn start(w: &mut World, e: Entity, _: &Ctx) {
    let Some(p) = site(w, e) else { return };
    look_block(w, e, p);
    if is_composter(w, p) {
        make_bread(w, e);
        w.resource_mut::<crate::goals::Events>().host.composter_work(e, p);
    }
    trade_restock_work::restock_if_due(w, e);
}
