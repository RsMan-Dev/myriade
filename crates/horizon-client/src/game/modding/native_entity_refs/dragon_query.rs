//! The ECS dragon participates in Level.getEntities; parts follow the walk.
use super::*;

impl Game<'_> {
    pub(super) fn append_dragon_query_ids(&self, bounds: Aabb, parts: bool, ids: &mut Vec<u32>) {
        for (id, dragon) in self.ecs.dragon_views().into_iter().filter(|(id, _)| *id != 0) {
            if parts {
                ids.extend(
                    (0..8u32)
                        .filter(|&k| bounds.intersects(dragon.boxes[k as usize]))
                        .map(|k| id + 1 + k),
                );
            } else {
                // EntityType.ENDER_DRAGON dimensions: 16 wide, 8 high.
                let p = dragon.position;
                let body = Aabb::new(
                    Vec3::new(p[0] - 8.0, p[1], p[2] - 8.0),
                    Vec3::new(p[0] + 8.0, p[1] + 8.0, p[2] + 8.0),
                );
                if bounds.intersects(body) {
                    ids.push(id);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::native_entity_refs::tests::with_game;
    use horizon_client::mobs::Mob;

    fn summon_dragon_20(game: &mut Game<'_>) {
        game.ecs_with(|ecs, mw| {
            ecs.summon_dragon(
                mw,
                [0.0, 80.0, 0.0],
                0.0,
                0,
                horizon_ecs_proto::species::ender_dragon::START_PHASE,
                &mut || 20,
            )
        });
    }

    #[test]
    #[ignore = "daily"]
    fn entity_uuid_lookup_includes_the_dragon_and_excludes_its_parts() {
        use crate::game::creatures::SpawnSite;
        use horizon_client::lan::playerdata::uuid_ints;
        use mod_api::{EntityUuid, ServerHost};

        with_game(|game| {
            let _phase = mod_api::__private::PhaseOwner::scope(mod_api::__private::ModPhase::World);
            let id = game
                .spawn_dragon_spawned([0.0, 80.0, 0.0], SpawnSite::Plain)
                .unwrap();
            game.persist.uuids.insert(id, [19, 23, 29, 31]);
            let dragon = game.entity(id).unwrap();
            let part = game.entity(id + 1).unwrap();
            let part_uuid = part.uuid(game).expect("dragon parts have UUIDs");
            // A part can also have saved UUID metadata when it holds attachments.
            game.persist.uuids.insert(part.id(), uuid_ints(part_uuid));

            let host: &dyn ServerHost = game;
            let dragon_uuid = EntityUuid(0x0000_0013_0000_0017_0000_001d_0000_001f);
            let part_uuid = EntityUuid(part_uuid);
            assert_eq!(host.entity_by_uuid(dragon_uuid), Some(dragon));
            assert_eq!(host.entity_by_uuid(part_uuid), None);

            #[cfg(not(any(feature = "system-allocator", feature = "alloc-census")))]
            {
                let (found, allocations) = horizon_threads::allocation_count::measure(|| {
                    (
                        host.entity_by_uuid(dragon_uuid),
                        host.entity_by_uuid(part_uuid),
                    )
                });
                assert_eq!(found, (Some(dragon), None));
                assert_eq!(allocations, 0);
            }
        });
    }



}
