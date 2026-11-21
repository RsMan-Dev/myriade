//! What every declared identifier costs while no player is near (the zone on, `Herd::with_slab`: nothing is within reach): is it put
//! to sleep (`Dormant`), and which systems still run for it, per entity and tick, above the same herd without it. Written to
//! `DORMANCY_REPORT` when set; the test fails when an identifier that stays awake is visited by a system for more than `LIMIT` instr.
use horizon_core::nbt::Tag;
use horizon_ecs_proto::{
    cow::{spawn_species, species_table, Herd},
    nbt, profile,
};
use std::fmt::Write as _;

const N0: usize = 60;
fn count() -> usize {
    std::env::var("DORMANCY_N").ok().and_then(|v| v.parse().ok()).unwrap_or(N0)
}
const TICKS: u64 = 100;
fn ticks() -> u64 {
    std::env::var("DORMANCY_TICKS").ok().and_then(|v| v.parse().ok()).unwrap_or(TICKS)
}
/// Instructions per entity and tick above which an awake identifier fails.
const LIMIT: u64 = 40;

fn make(h: &mut Herd, id: &str, i: usize) -> Option<bevy_ecs::entity::Entity> {
    let (x, z) = (0.5 + (i % 20) as f64, 0.5 + (i / 20) as f64);
    match species_table().0.into_iter().find(|s| s.name == id) {
        Some(sp) => Some(spawn_species(&mut h.world, &sp, x, z, 1 + i as u64)),
        None => {
            let Tag::Compound(c) = Tag::compound([("id", Tag::String(id.into())), ("Pos", Tag::List(vec![Tag::Double(x.to_bits()), Tag::Double(1.0f64.to_bits()), Tag::Double(z.to_bits())]))]) else { unreachable!() };
            nbt::restore(&mut h.world, &c)
        }
    }
}

fn run(id: Option<&str>) -> (Vec<(String, u64)>, usize) {
    let mut h = Herd::with_slab(40, true, false);
    h.tick();
    let mut made = 0;
    if let Some(id) = id {
        let es: Vec<_> = (0..count()).filter_map(|i| make(&mut h, id, i)).collect();
        made = es.len();
        h.admit_spawned(&es);
    }
    let shared = profile::instrument(&mut h);
    for _ in 0..5 {
        h.tick();
    }
    profile::reset(&shared);
    for _ in 0..ticks() {
        h.tick();
    }
    let asleep = h.world.query_filtered::<(), (bevy_ecs::query::With<horizon_ecs_proto::zone::Dormant>, bevy_ecs::prelude::Allow<bevy_ecs::entity_disabling::Disabled>)>().iter(&h.world).count();
    let r = shared.lock().unwrap();
    (r.rows.iter().map(|x| (x.name.clone(), x.run + x.apply)).collect(), if id.is_some() { asleep } else { made })
}

#[test]
#[ignore = "measure: the instruction counter needs a quiet process; run it alone (DORMANCY_REPORT=file)"]
fn what_an_entity_costs_when_no_player_is_near() {
    if !super::common::have_data() {
        return;
    }
    let mut probe = Herd::with_slab(40, true, false);
    probe.tick();
    let ids: Vec<String> = nbt::recipes(&probe.world).into_iter().map(|(k, _)| k).collect();
    let (base, _) = run(None);
    let (mut out, mut bad) = (String::from("# Entities without a player near\n\nPer identifier: asleep (`Dormant`) of 60 made, then the systems that still cost per entity and tick (above the herd without it).\n\n"), Vec::new());
    for id in ids {
        let (rows, asleep) = run(Some(&id));
        let mut costly: Vec<(String, u64)> = rows
            .iter()
            .zip(&base)
            .filter_map(|((n, a), (_, b))| (a.saturating_sub(*b) / (count() as u64 * ticks()) > 0).then(|| (n.clone(), a.saturating_sub(*b) / (count() as u64 * ticks()))))
            .collect();
        costly.sort_by_key(|c| std::cmp::Reverse(c.1));
        let total: u64 = costly.iter().map(|c| c.1).sum();
        let _ = writeln!(out, "- `{id}`: asleep {asleep}/{}; {total} instr per entity-tick{}", count(), costly.iter().take(4).map(|(n, c)| format!(" [{n} {c}]")).collect::<String>());
        if asleep < count() && total > LIMIT {
            bad.push(format!("{id}: {total}"));
        }
    }
    if let Ok(p) = std::env::var("DORMANCY_REPORT") {
        std::fs::write(p, &out).expect("write the report");
    }
    assert!(bad.is_empty(), "awake and costly: {bad:?}");
}
