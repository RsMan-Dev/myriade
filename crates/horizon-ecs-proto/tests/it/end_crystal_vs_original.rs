//! The End crystal against the vanilla `EndCrystal`, transcribed from its bytecode: `tick` sets fire at its feet when the block there
//! is air (in the End), `hurtServer` refuses fire and a shielded crystal's blow unless creative, and removes it on the same tick.
//! The reference is a tick model, not the ECS deadlines. Fire comes on at its first tick, and again on the tick after the block under
//! it goes back to air. `HORIZON_TEST_RESOURCES` set.
use crate::common;
use common::*;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::blocks::world_of;
use horizon_ecs_proto::end_crystal::{block_changed, hurt, spawn, Blow, CrystalEvents, EndFire, Shielded};
use horizon_ecs_proto::goals::*;
use horizon_ecs_proto::targeting::Who;

const CELL: [i32; 3] = [0, 10, 0];

/// The ticks (counted from the spawn) on which the fire appears under a crystal, when the block there is air at the start and burns
/// out during tick `burnt`: the crystal's tick sees it at its first tick and at the next one.
fn vanilla_fire_ticks(burnt: i64) -> Vec<i64> {
    vec![1, burnt + 1]
}

fn fire_on(h: &Herd) -> bool {
    let mw = world_of(&h.world);
    let fire = mw.registry.default_state("minecraft:fire");
    fire.is_some() && Some(mw.world.block(CELL[0], CELL[1], CELL[2])) == fire
}

#[test]
fn fire_under_a_crystal_comes_on_the_vanilla_ticks() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab(16, true, false);
    h.world.insert_resource(EndFire(true));
    h.set_block(CELL[0], CELL[1], CELL[2], "minecraft:air");
    h.tick();
    spawn(&mut h.world, Vec3::new(0.5, 10.0, 0.5), true);
    let (mut placed, mut was) = (Vec::new(), false);
    for t in 1..=10 {
        if t == 5 {
            h.set_block(CELL[0], CELL[1], CELL[2], "minecraft:air");
        }
        if t == 6 {
            block_changed(&mut h.world, CELL);
        }
        h.tick();
        let now = fire_on(&h);
        if now && !was {
            placed.push(t);
        }
        was = now;
    }
    assert_eq!(placed, vanilla_fire_ticks(5));
}

#[test]
fn blows_are_refused_or_remove_the_crystal_on_their_tick() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab(16, true, false);
    h.world.insert_resource(EndFire(true));
    let at = Vec3::new(0.5, 10.0, 0.5);
    let e = spawn(&mut h.world, at, true);
    h.tick();
    let blow = Blow { by: Some(Who::Player(1)), fire: false, explosion: false, creative: false, dragon: false };
    assert!(!hurt(&mut h.world, e, Blow { fire: true, ..blow }), "fire is refused");
    assert!(!hurt(&mut h.world, e, Blow { dragon: true, ..blow }), "a dragon's blow is refused");
    h.world.entity_mut(e).insert(Shielded);
    assert!(!hurt(&mut h.world, e, blow), "a shielded crystal takes no survival blow");
    assert!(h.world.get_entity(e).is_ok(), "still standing after the refusals");
    h.tick();
    assert!(hurt(&mut h.world, e, Blow { creative: true, ..blow }), "a creative blow goes through");
    assert!(h.world.get_entity(e).is_err(), "gone on the tick of the blow");
    let gone = &h.world.resource::<CrystalEvents>().0;
    assert_eq!(gone.len(), 1);
    assert_eq!(gone[0].at, at);
}
