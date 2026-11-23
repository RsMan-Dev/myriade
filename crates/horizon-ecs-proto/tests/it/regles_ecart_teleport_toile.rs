//! Comportement ECS actuel, écart à soumettre au propriétaire (journal regles-ecrites-4.txt, rangs 5 et 7). Vanille : après une
//! téléportation due à un dégât (magma, feu), `Entity.applyEffectsFromBlocks` garde le chemin de l'ancienne position (la toile
//! quittée ralentit encore) et ne réallume pas au point d'arrivée. Ici : le scan se fait au point courant ; une toile quittée ne
//! ralentit plus, rien d'autre n'est lu. Ce test fige le comportement ECS, pas la vanille.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::cow::*;

fn body(h: &mut Herd, p: Vec3) -> Entity {
    let e = h
        .world
        .spawn((
            Pos(p),
            Vel(Vec3::ZERO),
            Body { width: 0.6, height: 1.8, step_height: 0.6, speed: 0.0, fall_factor: 1.0, safe_fall: 3.0, fall_mult: 1.0, movement_efficiency: 0.0, object: false, orb: false, tnt: false, lava_walker: false },
            Gravity(0.08),
            Bounds(box_at(p, 0.6, 1.8)),
            Cell(u32::MAX, [i32::MIN; 8], u32::MAX),
            Contact { on_ground: false, horizontal: false, fall_distance: 0.0, stuck: 0 },
            FluidState::default(),
            Moving,
            Health(20.0),
            HurtAt { at: 0, last: 0.0 },
        ))
        .id();
    enter_index(&mut h.world, e);
    e
}

#[test]
fn a_creature_moved_out_of_a_web_is_no_longer_slowed_by_it() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    for y in 1..=5 {
        h.set_block(0, y, 0, "minecraft:cobweb");
    }
    h.tick();
    let e = body(&mut h, Vec3::new(0.5, 4.0, 0.5));
    // The move a teleport makes: the body is now above a clear column, the web left behind.
    h.world.get_mut::<Pos>(e).unwrap().0 = Vec3::new(4.5, 4.0, 0.5);
    for _ in 0..15 {
        h.tick();
    }
    let y = h.world.get::<Pos>(e).unwrap().0.y;
    assert!(y < 2.0, "the free fall at the landing spot, not the web's slow fall (y = {y})");
}
