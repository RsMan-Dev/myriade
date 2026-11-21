//! The golden of a free fall: Horizon's own falling block, tick by tick, from 70 onto a stone floor (top at 61). The trace (`tick y vy`, the last
//! row the tick it lands) is the file the ECS prototype's `falling_block_vs_original` compares its entity with.
//! `FALLING_GOLDEN_OUT=<file>` writes the trace; otherwise it is checked against the committed one.
use crate::game::{FallingBlock, native_entity_refs::tests::with_game};
use horizon_client::chunk::Chunk;
use horizon_core::{geometry::Vec3, position::BlockPos};
use std::sync::Arc;

const GOLDEN: &str = "../horizon-ecs-proto/tests/golden/falling_free_fall.txt";

#[test]
fn a_falling_block_falls_as_the_golden_says() {
    if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
        return;
    }
    with_game(|game| {
        game.world
            .chunks
            .insert((0, 0), Arc::new(Chunk::empty(0, 0)));
        let registry = game.pack.assets.registry;
        game.world
            .set(0, 60, 0, registry.default_state("minecraft:stone").unwrap())
            .unwrap();
        let sand = registry.default_state("minecraft:sand").unwrap();
        let at = Vec3::new(0.5, 70.0, 0.5);
        game.falling.push(FallingBlock {
            on_ground: false,
            attachment_lifetime: Default::default(),
            rotation: [0.0; 2],
            join: Default::default(),
            id: 71,
            state: sand,
            start_pos: BlockPos::new(0, 70, 0),
            position: at,
            previous: at,
            velocity: Vec3::ZERO,
            age: 0,
            no_gravity: false,
            portal: Default::default(),
            drop_item: false,
            fall_distance: 0.0,
            stuck: Vec3::ZERO,
            damage: Default::default(),
            saved: None,
        });
        let mut trace = String::new();
        for t in 1..=400 {
            game.tick_one_falling(0, &[]).unwrap();
            if game.falling.is_empty() {
                trace += &format!("{t} landed\n");
                break;
            }
            let f = &game.falling[0];
            trace += &format!("{t} {:?} {:?}\n", f.position.y, f.velocity.y);
        }
        assert!(trace.ends_with("landed\n"), "it never landed");
        assert_eq!(game.world.block(0, 61, 0), sand, "it is a block again");
        match std::env::var_os("FALLING_GOLDEN_OUT") {
            Some(out) => std::fs::write(out, &trace).unwrap(),
            None => {
                let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(GOLDEN);
                assert_eq!(trace, std::fs::read_to_string(path).unwrap());
            }
        }
    });
}
