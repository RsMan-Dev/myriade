//! Powder snow for a creature (`LivingEntity` on `PowderSnowBlock`): a zombie is not in `#powder_snow_walkable_mobs` (rabbit, endermite,
//! silverfish, fox), so it sinks into the snow; a rabbit stands on it. Expected values come from the jar's tag, not from the code under test.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_ecs_proto::cow::{spawn_species, Herd, Pos, SpeciesTable};

fn y_of(h: &Herd, e: Entity) -> f64 {
    h.world.get::<Pos>(e).unwrap().0.y
}

fn spawn_on_snow(h: &mut Herd, name: &str, x: f64, y: f64) -> Entity {
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == name).unwrap();
    let e = spawn_species(&mut h.world, &sp, x, 0.5, 5);
    h.world.get_mut::<Pos>(e).unwrap().0.y = y;
    e
}

/// A zombie (not walkable on powder snow) sinks into the column; a rabbit (walkable) stays on its top, the snow's top at y = 6.
#[test]
fn a_zombie_sinks_into_powder_snow_and_a_rabbit_stands_on_it() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    for y in 1..=5 {
        h.set_block(0, y, 0, "minecraft:powder_snow");
        h.set_block(3, y, 0, "minecraft:powder_snow");
    }
    let zombie = spawn_on_snow(&mut h, "minecraft:zombie", 0.0, 7.0);
    let rabbit = spawn_on_snow(&mut h, "minecraft:rabbit", 3.0, 7.0);
    for _ in 0..60 {
        h.tick();
    }
    assert!(y_of(&h, zombie) < 5.5, "a zombie sinks into powder snow (y = {})", y_of(&h, zombie));
    assert!(y_of(&h, rabbit) > 5.5, "a rabbit stands on powder snow (y = {})", y_of(&h, rabbit));
}

/// The jar's walk rule (`PowderSnowBlock.canEntityWalkOnPowderSnow`) reads the tag and the boots, never the hand: a zombie holding
/// scaffolding stands on the scaffolding as one that holds nothing does (the origin test's expectation, 65.0, is the same).
#[test]
fn holding_scaffolding_changes_nothing_on_scaffolding() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    h.set_block(0, 5, 0, "minecraft:scaffolding");
    h.set_block(3, 5, 0, "minecraft:scaffolding");
    let bare = spawn_on_snow(&mut h, "minecraft:zombie", 0.0, 7.0);
    let held = spawn_on_snow(&mut h, "minecraft:zombie", 3.0, 7.0);
    let item = myriade_engine_lib::items::ItemStack::new("minecraft:scaffolding", 1);
    horizon_ecs_proto::goals::set_main_hand(&mut h.world, held, Some(item));
    for _ in 0..20 {
        h.tick();
    }
    assert!(y_of(&h, held) > 5.9, "a zombie holding scaffolding stands on it (y = {})", y_of(&h, held));
    assert!((y_of(&h, held) - y_of(&h, bare)).abs() < 1e-6, "holding scaffolding changes no height");
}

/// A zombie standing on scaffolding is held at the block's top (the stable shape above scaffolding, `ScaffoldingBlock` has no `fallOn`).
#[test]
fn a_zombie_stands_on_scaffolding() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    h.set_block(0, 5, 0, "minecraft:scaffolding");
    let zombie = spawn_on_snow(&mut h, "minecraft:zombie", 0.0, 7.0);
    for _ in 0..40 {
        h.tick();
    }
    assert!(y_of(&h, zombie) > 5.9, "a zombie stands on scaffolding (y = {})", y_of(&h, zombie));
}
