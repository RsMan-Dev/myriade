//! Point 3: the durations of the effects of a blow follow the level's difficulty (jar: `CaveSpider` 0/7/15 s, `Husk` 140 ticks per whole
//! point of the effective difficulty, only with an empty hand). Without a level the effective difficulty is its floor, `base * 0.75`.
use bevy_ecs::prelude::*;
use horizon_ecs_proto::effects::{Difficulty, CAVE_SPIDER, HUSK};

#[test]
fn cave_spider_poison_by_difficulty() {
    let mut w = World::new();
    let from = w.spawn_empty().id();
    let span = CAVE_SPIDER.0[0].2;
    let got: Vec<i32> = (0..4u8)
        .map(|d| {
            w.insert_resource(Difficulty(d));
            span.ticks(&w, from)
        })
        .collect();
    assert_eq!(got, vec![0, 0, 140, 300]);
}

#[test]
fn husk_hunger_follows_the_effective_difficulty() {
    let mut w = World::new();
    let from = w.spawn_empty().id();
    w.insert_resource(Difficulty(3));
    // 3 * 0.75 = 2.25: two whole points.
    assert_eq!(HUSK.0[0].2.ticks(&w, from), 280);
}
