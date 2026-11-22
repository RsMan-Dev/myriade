//! `LivingEntity.baseTick`'s drowning: the air (300) runs down by 1 a tick with the eyes under water; at -20 it is set back to 0, the creature
//! takes 2 drowning damage and the host is told with `MiscEvent::Drowned` (8 bubbles), then again every 20 ticks. A creature that breathes
//! under water (the type tag `can_breathe_under_water`, a species that does not drown) never drowns.
use crate::common::*;
use horizon_ecs_proto::{colony_register::MiscOut, cow::*, effects::CanBreatheUnderWater, host_out_misc::MiscEvent};

/// A 25 x 25 pool two blocks deep under a stone lid: whatever the creature does, its eyes stay under water.
fn lidded_pool(h: &mut Herd) {
    for x in -12..=12 {
        for z in -12..=12 {
            h.set_block(x, 1, z, "minecraft:water");
            h.set_block(x, 2, z, "minecraft:water");
            h.set_block(x, 3, z, "minecraft:stone");
        }
    }
    h.tick();
}

fn drownings(h: &Herd) -> Vec<(bevy_ecs::entity::Entity, i32)> {
    h.world.get_resource::<MiscOut>().map_or_else(Vec::new, |m| m.0.iter().filter_map(|e| if let MiscEvent::Drowned { entity, bubbles, .. } = e { Some((*entity, *bubbles)) } else { None }).collect())
}

fn spawn_named(h: &mut Herd, name: &str) -> bevy_ecs::entity::Entity {
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == name).unwrap_or_else(|| panic!("{name}"));
    spawn_species(&mut h.world, &sp, 0.5, 0.5, 61)
}

#[test]
fn a_cow_under_water_drowns_after_320_ticks_then_every_20_for_2_damage() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    lidded_pool(&mut h);
    let e = spawn_named(&mut h, "minecraft:cow");
    let max = h.world.get::<Health>(e).unwrap().0;
    let mut at = Vec::new();
    for t in 1..=420 {
        h.tick();
        // The drowning's 2 damage is queued by the breath and taken by the next tick's hit system.
        if let Some(&d) = at.last().filter(|&&d| d + 1 == t && at.len() <= 2) {
            assert_eq!(h.world.get::<Health>(e).unwrap().0, max - 2.0 * at.len() as f32, "2 damage a drowning (at {d})");
        }
        let n = drownings(&h).len();
        if n > at.len() {
            at.push(t);
            assert_eq!(drownings(&h)[n - 1], (e, 8), "eight bubbles");
        }
    }
    assert!(at.len() >= 3, "drowned at {at:?}");
    assert!((318..=324).contains(&at[0]), "300 air plus 20 below zero: the first drowning near tick 320, got {at:?}");
    assert_eq!(at[1] - at[0], 20, "{at:?}");
    assert_eq!(at[2] - at[1], 20, "{at:?}");
}

#[test]
fn a_creature_with_the_breathe_under_water_tag_never_drowns() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    lidded_pool(&mut h);
    let e = spawn_named(&mut h, "minecraft:cow");
    h.world.entity_mut(e).insert(CanBreatheUnderWater);
    for _ in 0..400 {
        h.tick();
    }
    assert!(drownings(&h).is_empty(), "{:?}", drownings(&h));
}

#[test]
fn a_drowned_zombie_does_not_drown() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    lidded_pool(&mut h);
    let _ = spawn_named(&mut h, "minecraft:drowned");
    for _ in 0..400 {
        h.tick();
    }
    assert!(drownings(&h).is_empty(), "{:?}", drownings(&h));
}
