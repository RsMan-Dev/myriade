//! Le fermier récolte une culture mûre à portée (3x3x3, tirée comme le `Mob`) et la replante, ou sème dans la terre labourée libre
//! la graine taggée de son inventaire (`HarvestFarmland`, `villager_brain.rs:1685-1712`, `harvest_tick` `3385`). La maturité suit la
//! classe du bloc (`CropBlock`, mods compris). `replant` vient de la configuration (interface supposée, voir `villager-divers.md`).
use super::block_reach::{block_at, look_block, pick_in_reach, walk_to, Pred};
use crate::behaviours::{food_inventory, Cap};
use crate::blocks;
use crate::brain::*;
use crate::goals::Events;
use crate::rules::bonemeal::near_crop;
use crate::rules::farm::{crop_max_age, first_seed, is_ripe};
use bevy_ecs::prelude::*;
use horizon_client_lib::mobs::MobWorld;
use horizon_world::registry::BlockStateId;

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:villager_harvest_crop",
    mems: &[&WALK_TARGET, &LOOK_TARGET],
    requires: &[Cap::Body, Cap::Locomotion],
    can_start,
    start,
    tick,
    stop,
    ..Behaviour::DEFAULT
};

pub struct Params {
    pub ripe: Pred,
    pub replant: fn(&'static MobWorld<'static>, [i32; 3]) -> Option<BlockStateId>,
}

/// `CropBlock.isMaxAge` of the block at `p`, by its class (`PitcherCropBlock` is no crop; a mod's crop is by its template).
pub fn ripe(mw: &'static MobWorld<'static>, p: [i32; 3]) -> bool {
    let facts = mw.facts.get(blocks::state_id(mw, p));
    let max = crop_max_age(facts.has_class("CropBlock"), facts.has_class("BeetrootBlock"), facts.has_class("TorchflowerCropBlock"));
    is_ripe(max, blocks::int_property(mw, p, "age"))
}
/// `HarvestFarmland.validPos` for a sowing: air above farmland.
pub fn bare(mw: &'static MobWorld<'static>, p: [i32; 3]) -> bool {
    mw.facts.is_air(blocks::state_id(mw, p)) && mw.facts.get(blocks::state_id(mw, [p[0], p[1] - 1, p[2]])).has_class("FarmlandBlock")
}
/// The block a seed plants: a tagged BlockItem plants its block's default state (`seed_crop`).
fn seed(mw: &MobWorld<'static>, item: &str) -> Option<BlockStateId> {
    let items = mw.items?;
    if !items.item_in_tag("minecraft:villager_plantable_seeds", item) {
        return None;
    }
    items.get(item)?.block
}
/// The first seed of the villager's stock that plants, with the block it plants.
fn stock_seed(w: &World, e: Entity, mw: &MobWorld<'static>) -> Option<(String, BlockStateId)> {
    let stock = &w.get::<food_inventory::Stock>(e)?.0;
    let (item, _) = &stock[first_seed(stock, |item| seed(mw, item).is_some())?];
    Some((item.clone(), seed(mw, item)?))
}
fn holds_seed(w: &World, e: Entity) -> bool {
    stock_seed(w, e, blocks::world_of(w)).is_some()
}
/// The crop to harvest in reach, else a bare farmland to sow when the villager holds a seed.
fn target(w: &mut World, e: Entity, ripe: Pred) -> Option<[i32; 3]> {
    pick_in_reach(w, e, ripe).or_else(|| if holds_seed(w, e) { pick_in_reach(w, e, bare) } else { None })
}

fn can_start(w: &mut World, e: Entity, c: &Ctx) -> bool {
    let ripe = c.get::<Params>().ripe;
    target(w, e, ripe).is_some()
}

fn start(w: &mut World, e: Entity, c: &Ctx) {
    let ripe = c.get::<Params>().ripe;
    let Some(q) = target(w, e, ripe) else { return };
    walk_to(w, e, q, 0.5, 1);
    look_block(w, e, q);
}

/// Within reach (one block on each axis): replant a ripe crop or sow a bare farmland, then stop; otherwise keep walking.
fn tick(w: &mut World, e: Entity, c: &Ctx) -> bool {
    let p = c.get::<Params>();
    let Some(WalkTarget { target: Tracker::Block(q), .. }) = walk_target(w, e) else { return false };
    let mw = blocks::world_of(w);
    let ripe = (p.ripe)(mw, q);
    if !ripe && !bare(mw, q) {
        return false;
    }
    let Some(here) = block_at(w, e) else { return false };
    if near_crop(here, q) {
        if ripe {
            crate::level::break_block(w, q, true, Some(e), Some(e));
            if let Some(id) = (p.replant)(mw, q) {
                blocks::set_state_id(w, q, id);
            }
        } else {
            sow(w, e, mw, q);
        }
        return false;
    }
    true
}

/// Sows the first seed of the stock in the farmland at `q`: the seed is taken, the crop placed, `block_place` posted with the villager.
fn sow(w: &mut World, e: Entity, mw: &'static MobWorld<'static>, q: [i32; 3]) {
    let Some((item, state)) = stock_seed(w, e, mw) else { return };
    food_inventory::take(w, e, &item, 1);
    blocks::set_state_id(w, q, state);
    let host = &mut w.resource_mut::<Events>().host;
    host.block_place(e, q.map(|v| f64::from(v) + 0.5), state);
    host.sound("item.crop.plant", q.map(f64::from), 1.0, 1.0);
}

fn stop(w: &mut World, e: Entity, _: &Ctx) {
    erase(w, e, &WALK_TARGET);
    erase(w, e, &LOOK_TARGET);
}
