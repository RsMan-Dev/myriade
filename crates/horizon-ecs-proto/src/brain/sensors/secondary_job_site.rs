//! The secondary job site (`sense_secondary` of the `Mob`'s villager): the blocks of the profession's entry (`secondary_blocks`: farmland
//! for the farmer, the members of its `horizon:villager_secondary_poi/<profession>` tag, resolved once at load), within 4 blocks along x
//! and z and 2 along y of the villager, every 40 ticks. No block in the entry: no scan. Nothing found: the memory is erased.
use crate::behaviours::block_reach::block_at;
use crate::behaviours::trade_offers::{Merchant, TradeData};
use crate::blocks;
use crate::brain::*;

pub static SECONDARY_JOB_SITE: MemKey = MemKey::new("horizon:secondary_job_site", Shape::Blocks);
pub static SENSOR: Sensor = Sensor { name: "horizon:secondary_job_site", mems: &[&SECONDARY_JOB_SITE], scan, ..Sensor::DEFAULT };

fn scan(w: &mut World, e: Entity, _: &Ctx) {
    let index = usize::from(w.get::<Merchant>(e).map_or(0, |m| m.profession));
    let Some(blocks_of) = w.get_resource::<TradeData>().and_then(|d| d.trades.professions.entry(index)).map(|p| &p.secondary_blocks) else {
        erase(w, e, &SECONDARY_JOB_SITE);
        return;
    };
    if blocks_of.is_empty() {
        erase(w, e, &SECONDARY_JOB_SITE);
        return;
    }
    let Some(c) = block_at(w, e) else { return };
    let mw = blocks::world_of(w);
    let mut sites = Vec::new();
    for x in -4..=4 {
        for y in -2..=2 {
            for z in -4..=4 {
                let p = [c[0] + x, c[1] + y, c[2] + z];
                if blocks_of.contains(mw.block_name(p[0], p[1], p[2])) {
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
