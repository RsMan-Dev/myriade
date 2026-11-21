//! The normal case of a server: nearly every entity sleeps (`Dormant`, `Disabled`). `save_all` must write each of them as it writes it
//! awake, and `restore` must bring them back: no entity may be lost to sleep (a herd saved asleep has as many compounds, per identifier, as
//! the same herd saved just after it was made).
use bevy_ecs::prelude::*;
use horizon_core::nbt::{Compound, Tag};
use horizon_ecs_proto::{cow::Herd, nbt, zone::Dormant};
use std::collections::BTreeMap;

const PER_ID: usize = 3;

fn compound(id: &str, i: usize) -> Compound {
    let pos = Tag::List(vec![Tag::Double((0.5 + i as f64).to_bits()), Tag::Double(1.0f64.to_bits()), Tag::Double(0.5f64.to_bits())]);
    let Tag::Compound(c) = Tag::compound([("id", Tag::String(id.into())), ("Pos", pos)]) else { unreachable!() };
    c
}

fn saved_by_id(h: &mut Herd) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    nbt::save_all(&mut h.world, |_, c| {
        if let Some(Tag::String(id)) = c.get(&"id".into()) {
            *out.entry(id.to_string_lossy()).or_insert(0) += 1;
        }
    });
    out
}

#[test]
fn a_sleeping_herd_is_saved_and_restored_whole() {
    if !super::common::have_data() {
        return;
    }
    let mut h = Herd::with_slab(40, true, false);
    h.tick();
    let ids: Vec<String> = nbt::recipes(&h.world).into_iter().map(|(k, _)| k).collect();
    let mut made = Vec::new();
    for id in &ids {
        for i in 0..PER_ID {
            if let Some(e) = nbt::restore(&mut h.world, &compound(id, i)) {
                made.push(e);
            }
        }
    }
    let awake = saved_by_id(&mut h);
    h.admit_spawned(&made);
    for _ in 0..5 {
        h.tick();
    }
    let asleep_n = h.world.query_filtered::<(), (With<Dormant>, Allow<bevy_ecs::entity_disabling::Disabled>)>().iter(&h.world).count();
    assert!(asleep_n > 20, "the test is vacuous: only {asleep_n} entities sleep");
    let asleep = saved_by_id(&mut h);
    let lost: Vec<_> = awake.iter().filter(|(k, n)| asleep.get(*k).copied().unwrap_or(0) < **n).map(|(k, n)| format!("{k}: {n} -> {}", asleep.get(k).copied().unwrap_or(0))).collect();
    assert!(lost.is_empty(), "lost to sleep: {lost:?}");
    // The compounds, restored in a fresh world, give the same counts.
    let mut all = Vec::new();
    nbt::save_all(&mut h.world, |_, c| all.push(c));
    let mut fresh = Herd::with_slab(40, true, false);
    fresh.tick();
    for c in &all {
        nbt::restore(&mut fresh.world, c);
    }
    let back = saved_by_id(&mut fresh);
    let short: Vec<_> = asleep.iter().filter(|(k, n)| back.get(*k).copied().unwrap_or(0) < **n).map(|(k, n)| format!("{k}: {n} -> {}", back.get(k).copied().unwrap_or(0))).collect();
    assert!(short.is_empty(), "lost by restore: {short:?}");
    // A stand comes back as a stand (its `Stand` component), not as a bare entity.
    let stands = fresh.world.query_filtered::<(), (With<horizon_ecs_proto::stand::Stand>, Allow<bevy_ecs::entity_disabling::Disabled>)>().iter(&fresh.world).count();
    assert_eq!(stands, asleep.get("minecraft:armor_stand").copied().unwrap_or(0), "stands restored without `Stand`");
}
