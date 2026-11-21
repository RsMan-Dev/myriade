//! The evoker fangs against the vanilla counting of `EvokerFangs.tick` (warmup down to -8 for the bite, 22 ticks of life after the
//! start, the start's entity event sent once): the reference is a tick loop transcribed from the bytecode, not the ECS deadlines.
//! The start, the bite on a player in the box, and the removal fall on the same ticks. `HORIZON_TEST_RESOURCES` set.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::targeting::Who;
use horizon_ecs_proto::fangs::spawn_fang;
use horizon_ecs_proto::cow::Herd;
use horizon_ecs_proto::goals::*;
use horizon_ecs_proto::host_out::HostEvent;
use horizon_ecs_proto::hostile::*;

/// The vanilla ticks of one fang: (start, bite, removal), counted from its spawn.
fn vanilla_ticks(warmup: i32) -> (i64, i64, i64) {
    let (mut warm, mut life, mut sent) = (warmup, 22, false);
    let (mut start, mut bite) = (None, None);
    for t in 1..=100 {
        warm -= 1;
        if warm >= 0 {
            continue;
        }
        if warm == -8 {
            bite = Some(t);
        }
        if !sent {
            sent = true;
            start = Some(t);
        }
        life -= 1;
        if life < 0 {
            return (start.unwrap(), bite.unwrap(), t);
        }
    }
    unreachable!("a fang ends within 100 ticks")
}

#[test]
fn fangs_start_bite_and_end_on_the_vanilla_ticks() {
    if !have_data() {
        return;
    }
    for warmup in [0, 3, 17] {
        let at = Vec3::new(0.5, 1.0, 0.5);
        let mut h = Herd::with_slab_awake(16, true, false);
        let owner = h.world.spawn_empty().id();
        let fang = spawn_fang(&mut h.world, at, warmup, Some(Who::Entity(owner)));
        let (mut started, mut bitten, mut removed) = (None, None, None);
        let end = vanilla_ticks(warmup).2;
        for t in 1..=end + 2 {
            h.world.resource_mut::<Online>().set(Player { id: 1, x: at.x, y: at.y, z: at.z, main: 0, off: 0, ..Default::default() });
            h.tick();
            let out: Vec<HostEvent> = h.world.resource_mut::<Events>().host.drain().collect();
            if out.iter().any(|ev| matches!(ev, HostEvent::EntityEvent { entity, code: 4 } if *entity == fang)) {
                started.get_or_insert(t);
            }
            if h.world.resource::<PlayerState>().hits.iter().any(|x| x.tick == t as u64 && x.amount == 6.0) {
                bitten.get_or_insert(t);
            }
            if removed.is_none() && h.world.get_entity(fang).is_err() {
                removed = Some(t);
            }
        }
        let (start, bite, end) = vanilla_ticks(warmup);
        assert_eq!((started, bitten, removed), (Some(start), Some(bite), Some(end)), "warmup {warmup}");
    }
}
