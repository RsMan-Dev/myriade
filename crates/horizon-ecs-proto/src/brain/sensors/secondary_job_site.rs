//! The farmer's secondary job site (`sense_secondary` of the `Mob`'s villager): the farmland within 4 blocks along x and z and 2 along y of
//! the villager, every 40 ticks. Nothing found: the memory is erased. Only the farmer has it (the crops of the species are its configuration).
use crate::behaviours::block_reach::block_at;
use crate::behaviours::trade_offers::{self, Merchant};
use crate::blocks;
use crate::brain::*;
use bevy_ecs::prelude::*;

pub static SECONDARY_JOB_SITE: MemKey = MemKey::new("horizon:secondary_job_site", Shape::Blocks);
pub static SENSOR: Sensor = Sensor { name: "horizon:secondary_job_site", mems: &[&SECONDARY_JOB_SITE], scan, ..Sensor::DEFAULT };

fn scan(w: &mut World, e: Entity, _: &Ctx) {
    let Some(c) = block_at(w, e) else { return };
    // `sense_secondary`: the farmland is a secondary site of a farmer only (`farmer && farmland`), not of the other professions.
    let profession = w.get::<Merchant>(e).map_or(0, |m| m.profession);
    if trade_offers::profession_name(w, profession).strip_prefix("minecraft:") != Some("farmer") {
        erase(w, e, &SECONDARY_JOB_SITE);
        return;
    }
    let mw = blocks::world_of(w);
    let mut sites = Vec::new();
    for x in -4..=4 {
        for y in -2..=2 {
            for z in -4..=4 {
                let p = [c[0] + x, c[1] + y, c[2] + z];
                if mw.block_name(p[0], p[1], p[2]) == "minecraft:farmland" {
                    sites.push(p);
                }
            }
        }
    }
    if sites.is_empty() {
        erase(w, e, &SECONDARY_JOB_SITE);
    } else {
        set(w, e, &SECONDARY_JOB_SITE, MemValue::Blocks(sites), None);
    }
}
