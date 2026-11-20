//! The seed hunt of the sniffer against the real one (`HORIZON_TEST_RESOURCES` set, process species `minecraft:sniffer`): 20 sniffers on
//! dirt, alone, for 2400 ticks: the ticks they spend in each state (idling, happy, scenting, sniffing, searching, digging, rising), the
//! digs they begin, the seeds that drop (`GiftLoot` of the `Mob`, an item on the ground here) and the places they remember as explored.
use crate::common;
use common::*;
use horizon_client_lib::mobs::{sniffer, MobEvent};
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{brain::behaviours::sniff, cow::*, goals::*};

fn skip() -> bool {
    !have_data() || wrong_species(&["minecraft:sniffer"])
}

#[test]
fn the_sniffer_hunts_seeds_as_the_mob_does() {
    if skip() {
        return;
    }
    let mut r = MobRef::new(60, false);
    let mut h = Herd::with_slab(60, true, false);
    for x in -45..=45 {
        for z in -45..=45 {
            r.set_block(x, 0, z, "minecraft:dirt");
            h.set_block(x, 0, z, "minecraft:dirt");
        }
    }
    h.tick();
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == species().name).unwrap();
    let mut ids = Vec::new();
    for k in 0..20usize {
        let (x, z) = (-40.0 + 16.0 * (k % 6) as f64 + 0.5, -30.0 + 16.0 * (k / 6) as f64 + 0.5);
        let i = r.add_id(Vec3::new(x, 1.0, z), (((k as u32 + 1) * 13).wrapping_mul(0x9E37_79B1) >> 4) | 1);
        let e = spawn_species(&mut h.world, &sp, x, z, (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        h.world.resource_mut::<Entered>().0.push(e);
        ids.push((i, e));
    }
    let n = ids.len();
    let mut spent = [[0usize; 7]; 2];
    let mut digs = [0usize; 2];
    let mut was = vec![[0u8; 2]; n];
    let mut first_seed: Vec<[Option<i64>; 2]> = vec![[None; 2]; n];
    let mut dig_start: Vec<[Option<i64>; 2]> = vec![[None; 2]; n];
    let mut seeds_mob = 0usize;
    let mut seen_events = 0usize;
    for t in 1..=2400i64 {
        r.step(None);
        h.tick();
        for ev in &r.events[seen_events..] {
            if let MobEvent::GiftLoot { mob, .. } = ev {
                seeds_mob += 1;
                if let Some(k) = ids.iter().position(|&(i, _)| r.mobs[i].id == *mob) {
                    first_seed[k][0].get_or_insert(t);
                }
            }
        }
        seen_events = r.events.len();
        for (k, &(i, e)) in ids.iter().enumerate() {
            let st = [sniffer::state(&r.mobs[i]), sniff::state(&h.world, e)];
            for s in 0..2 {
                spent[s][st[s] as usize] += 1;
                if st[s] == sniff::DIGGING && was[k][s] != sniff::DIGGING {
                    digs[s] += 1;
                    dig_start[k][s].get_or_insert(t);
                }
            }
            was[k] = st;
        }
    }
    let seeds_ecs = h.world.query::<&Pos>().iter(&h.world).count();
    let explored = [ids.iter().map(|&(i, _)| r.mobs[i].extra.positions.len()).sum::<usize>(), ids.iter().map(|&(_, e)| h.world.get::<sniff::State>(e).map_or(0, |s| s.explored.len())).sum::<usize>()];
    let _ = seeds_ecs;
    let items = h.world.query::<&horizon_ecs_proto::item::Item>().iter(&h.world).count();
    for (name, v) in [("mob", &spent[0]), ("ecs", &spent[1])] {
        eprintln!("{name} ticks per state [idle, happy, scent, sniff, search, dig, rise]: {v:?}");
    }
    let first = |s: usize| {
        let v: Vec<i64> = dig_start.iter().filter_map(|d| d[s]).collect();
        (v.iter().sum::<i64>() as f64 / v.len().max(1) as f64, v.len())
    };
    eprintln!("digs begun: mob {} ecs {}; first dig at mean tick: mob {:.0} ({} sniffers) ecs {:.0} ({}); seeds: mob {} ecs items {}; explored places: mob {} ecs {}", digs[0], digs[1], first(0).0, first(0).1, first(1).0, first(1).1, seeds_mob, items, explored[0], explored[1]);
    assert!(digs[0] > 5 && digs[1] > 5, "témoin : ils creusent : mob {} ecs {}", digs[0], digs[1]);
    let near = |a: f64, b: f64| (a - b).abs() <= 0.25 * a.max(b);
    assert!(near(digs[0] as f64, digs[1] as f64) && near(seeds_mob as f64, items as f64) && near(explored[0] as f64, explored[1] as f64), "digs {digs:?} seeds {seeds_mob}/{items} explored {explored:?}");
    for s in 0..7 {
        assert!(near(spent[0][s] as f64, spent[1][s] as f64), "state {s}: mob {} ecs {}", spent[0][s], spent[1][s]);
    }
}
