//! `LivingRef` line of sight, random teleport and the box of a pose (arismoko/horizon#944):
//! the engine's own clip and `randomTeleport` paths, for players and mobs
//! alike.
use super::{Game, commands::Target};
use horizon_core::geometry::Vec3;
use mod_api::{EntityRef, LivingRef, Pose};

impl Game<'_> {
    /// `LivingEntity.hasLineOfSight(Entity)`: whether nothing that collides
    /// (`ClipContext.Block.COLLIDER`, fluids ignored) lies between the
    /// living entity's eyes and `target`'s eye height, within 128 blocks.
    /// False for a target in another level. None for a stale or unavailable
    /// living entity or target, one outside the level in place, or on a
    /// client.
    pub(super) fn living_has_line_of_sight(
        &self,
        living: LivingRef,
        target: EntityRef,
    ) -> Option<bool> {
        let viewer = self.native_living_target(living)?;
        let seen = self.native_entity_target(target)?;
        if self.target_level(viewer) != self.dim {
            return None;
        }
        if self.target_level(seen) != self.dim {
            return Some(false);
        }
        let feet = self.entity_position(living.entity())?;
        let from = self.entity_eye_position(living.entity())?;
        let to = self.entity_eye_position(target)?;
        let w = self.mob_world_base(&[]);
        Some(w.line_of_sight_for(
            Some(feet[1]),
            Vec3::new(from[0], from[1], from[2]),
            Vec3::new(to[0], to[1], to[2]),
        ))
    }

    /// `LivingEntity.getDimensions(Pose)`: the box the living entity has in
    /// `pose` — its width, height and eye height, `minecraft:scale`
    /// included, the fixed 0.2 box of a sleeping pose included. The species'
    /// own state still answers where its `getDefaultDimensions` reads it: a
    /// digging sniffer or an emerging warden is low in every pose. None for
    /// a stale or unavailable handle.
    pub(super) fn living_dimensions_for_pose(
        &self,
        living: LivingRef,
        pose: Pose,
    ) -> Option<[f64; 3]> {
        let target = self.native_living_read_target(living)?;
        // LivingEntity.getDimensions is final: sleeping wins over every
        // species' default dimensions, but never makes a stale handle valid.
        if pose == Pose::Sleeping {
            return Some([f64::from(0.2_f32); 3]);
        }
        match target {
            t @ (Target::Player | Target::Guest(_)) => {
                // `Avatar.getDefaultDimensions(pose).scale(getScale())`.
                let scale = self.native_target_attribute(
                    t,
                    "minecraft:scale",
                    horizon_world::attribute_map::ValueKind::Computed,
                )? as f32;
                let d = crate::pose::Dimensions::avatar(pose, scale);
                Some([
                    f64::from(d.width),
                    f64::from(d.height),
                    f64::from(d.eye_height),
                ])
            }
            Target::Mob(id) if self.ecs.is_active() => self
                .ecs
                .pose_dimensions(id, pose as u8)
                .map(|(w, h, e)| [f64::from(w), f64::from(h), f64::from(e)]),
            // ArmorStand.getDefaultDimensions ignores the pose: a marker's
            // box is empty, a small stand's BABY_DIMENSIONS, else the type's.
            Target::Stand(id) => {
                let a = self.armor_stand_saved(id)?;
                let dims = if a.marker {
                    (0.0, 0.0, 0.0)
                } else {
                    a.probe_dimensions()
                };
                let scale = self.native_target_attribute(
                    target,
                    "minecraft:scale",
                    horizon_world::attribute_map::ValueKind::Computed,
                )? as f32;
                Some([dims.0, dims.1, dims.2].map(|v| f64::from(v as f32 * scale)))
            }
            _ => None,
        }
    }

    /// `LivingEntity.randomTeleport(x, y, z, true, avoid)`: the point `at`
    /// is clamped into the world border and drops to the first block in
    /// `minecraft:entities_can_teleport_to` below it, losing a block of height
    /// per step down. The entity lands there, keeping its motion, unless that
    /// ground or a block in its box at the landing is in the `avoid` block
    /// tag, the box collides or holds a liquid, or the species refuses the
    /// spot (an enderman never lands on ground holding water). A landing
    /// stops a mob's navigation and broadcasts entity event 46 (the portal
    /// particles). Nothing else is played: a chorus fruit's sound and an
    /// enderman's own teleport effects belong to their callers.
    ///
    /// True when the entity moved. False when no spot was found, for a
    /// non-finite point, a fake player, a player in another level, an armor
    /// stand or the dragon, a stale handle, or on a client.
    pub(super) fn living_random_teleport(
        &mut self,
        living: LivingRef,
        at: [f64; 3],
        avoid: Option<&str>,
    ) -> bool {
        if !at.iter().all(|v| v.is_finite()) {
            return false;
        }
        let at = Vec3::new(at[0], at[1], at[2]);
        match self.native_living_target(living) {
            Some(target @ (Target::Player | Target::Guest(_))) => {
                if self.target_level(target) != self.dim {
                    return false;
                }
                self.native_player_sleep_write(target, |game| {
                    game.player.is_some() && game.random_teleport(at, avoid)
                })
            }
            Some(Target::Mob(id)) if self.ecs.is_active() => self.ecs_with(|ecs, mw| ecs.random_teleport(mw, id, at, avoid)),
            _ => false,
        }
    }
}

#[cfg(test)]
pub(in crate::game) mod tests {
    use super::super::native_entity_refs::tests::with_game as with_plain_game;
    use super::*;

    /// `with_game` with the level's ECS attached, so the mobs placed here exist (`ecs_game_with`).
    fn with_game(test: impl FnOnce(&mut crate::game::Game<'_>)) {
        with_plain_game(|game| {
            game.ecs = game.test_ecs_level();
            test(game)
        });
    }
    use mod_api::PathType;

    #[test]
    #[ignore = "daily"]
    fn pose_regression_dying_avatar_is_fixed() {
        with_game(|game| {
            crate::game::ecs::test_support::place_test_mob(game, 55, "minecraft:mannequin", [0.0; 3]);
            let living = game.living(game.entity(55).unwrap()).unwrap();
            assert!(game.living_set_attribute_base(living, "minecraft:scale", 2.0));
            assert_eq!(
                living.dimensions_for_pose(game, Pose::Dying),
                Some([f64::from(0.2_f32), f64::from(0.2_f32), f64::from(1.62_f32)])
            );
        });
    }

    #[test]
    #[ignore = "daily"]
    fn pose_regression_sleeping_stand() {
        with_game(|game| {
            crate::game::ecs::test_support::place_test_stand(game, 55, &horizon_client::entities::ArmorStand::new(
                    55,
                    Vec3::ZERO,
                    0.0,
                ));
            let living = game.living(game.entity(55).unwrap()).unwrap();
            assert_eq!(
                living.dimensions_for_pose(game, Pose::Sleeping),
                Some([f64::from(0.2_f32); 3])
            );
        });
    }

    #[test]
    #[ignore = "daily"]
    fn pose_regression_dragon_dimensions() {
        with_game(|game| {
            game.ecs_with(|ecs, mw| ecs.summon_dragon(mw, [0.0, 80.0, 0.0], 0.0, 0, horizon_ecs_proto::species::ender_dragon::START_PHASE, &mut || 55));
            let living = game.living(game.entity(55).unwrap()).unwrap();
            assert_eq!(
                living.dimensions_for_pose(game, Pose::Standing),
                Some([16.0, 8.0, f64::from(6.8_f32)])
            );
            assert!(game.living_set_attribute_base(living, "minecraft:scale", 2.0));
            assert_eq!(
                living.dimensions_for_pose(game, Pose::Standing),
                Some([32.0, 16.0, f64::from(13.6_f32)])
            );
            assert_eq!(
                living.dimensions_for_pose(game, Pose::Sleeping),
                Some([f64::from(0.2_f32); 3])
            );
        });
    }

    #[test]
    #[ignore = "daily"]
    fn pose_regression_scaled_stand() {
        with_game(|game| {
            crate::game::ecs::test_support::place_test_stand(game, 55, &horizon_client::entities::ArmorStand::new(
                    55,
                    Vec3::ZERO,
                    0.0,
                ));
            let living = game.living(game.entity(55).unwrap()).unwrap();
            assert!(game.living_set_attribute_base(living, "minecraft:scale", 2.0));
            assert_eq!(
                living.dimensions_for_pose(game, Pose::Standing),
                Some([1.0, f64::from(3.95_f32), f64::from(3.555_f32)])
            );
            game.armor_stand_edit(game.armor_stand_ids()[0], |a| a.small = true);
            assert_eq!(
                living.dimensions_for_pose(game, Pose::Standing),
                Some([0.5, f64::from(1.975_f32), f64::from(1.975_f32)])
            );
            game.armor_stand_edit(game.armor_stand_ids()[0], |a| a.marker = true);
            assert_eq!(
                living.dimensions_for_pose(game, Pose::Standing),
                Some([0.0; 3])
            );
            assert_eq!(
                living.dimensions_for_pose(game, Pose::Sleeping),
                Some([f64::from(0.2_f32); 3])
            );
        });
    }

    /// Lit empty chunks around the origin, a stone floor at y 63 over
    /// -8..8, and two zombies standing on it five blocks apart.
    pub(in crate::game) fn floor_and_zombies(game: &mut Game<'_>) {
        for (x, z) in (-1..=1).flat_map(|x| (-1..=1).map(move |z| (x, z))) {
            let mut chunk = horizon_client::chunk::Chunk::empty(x, z);
            chunk.lit = true;
            game.world.chunks.insert((x, z), std::sync::Arc::new(chunk));
        }
        let stone = game
            .pack
            .assets
            .registry
            .default_state("minecraft:stone")
            .unwrap();
        for x in -8..8 {
            for z in -8..8 {
                game.world.set(x, 63, z, stone).unwrap();
            }
        }
        game.player = Some(
            horizon_client::motion::PlayerMotion::new(game.motion, Vec3::new(0.5, 64.0, 0.5)).unwrap(),
        );
        crate::game::native_entity_refs::tests::bind_local_player(game, 20, 123);
        crate::game::ecs::test_support::place_test_mob(game, 12, "minecraft:zombie", [0.5, 64.0, 0.5]);
        crate::game::ecs::test_support::place_test_mob(game, 13, "minecraft:zombie", [5.5, 64.0, 0.5]);
    }

    #[test]
    #[ignore = "daily"]
    fn line_of_sight_follows_the_blocks_between() {
        with_game(|game| {
            floor_and_zombies(game);
            let a = game.living(game.entity(12).unwrap()).unwrap();
            let b = game.entity(13).unwrap();
            assert_eq!(a.has_line_of_sight(game, b), Some(true));
            let stone = game
                .pack
                .assets
                .registry
                .default_state("minecraft:stone")
                .unwrap();
            for y in 64..67 {
                game.world.set(3, y, 0, stone).unwrap();
            }
            assert_eq!(
                a.has_line_of_sight(game, b),
                Some(false),
                "a wall in between"
            );
        });
    }

    #[test]
    #[ignore = "daily"]
    fn dimensions_follow_the_pose_asked() {
        use super::super::lan::LanSettings;
        use horizon_client::lan::{Joined, NetConfig, PlayerSender, guest::Guest, hub::HubConfig};
        use horizon_protocol::packets::configuration::ClientInformation;
        use std::sync::{
            Arc,
            atomic::{AtomicI32, AtomicUsize},
        };

        with_game(|game| {
            let settings = LanSettings {
                port: 0,
                guest_commands: false,
                force_game_mode: false,
            };
            let net = NetConfig {
                listen: false,
                announce: false,
                lod_distance: 0,
                ..NetConfig::lan(0, String::new(), 2)
            };
            let config = HubConfig {
                default_game_mode: 0,
                guest_commands: false,
                force_game_mode: false,
                hardcore: false,
                view_distance: 2,
                simulation_distance: 2,
                dimension: game.dim.id().to_owned(),
                allow_flight: true,
                seed: 0,
                max_players: 2,
                integrated: true,
                levels: game.level_shapes,
                level_seeds: game.levels.runtime.seeds(),
            };
            game.lan_host(settings, net, config).unwrap();
            let (tx, _rx) = std::sync::mpsc::channel();
            let mut guest = Guest::new(
                Joined {
                    conn: 12,
                    uuid: 12,
                    name: "guest12".to_owned(),
                    properties: Vec::new(),
                    info: ClientInformation::new(8),
                    brand: String::new(),
                    sender: PlayerSender::new(tx, Arc::new(AtomicUsize::new(0))),
                    latency: Arc::new(AtomicI32::new(0)),
                    address: "127.0.0.1:1".parse().unwrap(),
                    expected: false,
                },
                12,
                [3.0, 64.0, 5.0],
                10,
            );
            guest.placed = true;
            let motion =
                horizon_client::motion::PlayerMotion::new(game.motion, Vec3::new(3.0, 64.0, 5.0))
                    .unwrap();
            guest.motion = Some(Box::new(motion));
            game.lan.as_mut().unwrap().hub.guests.push(guest);
            let living = game.living(game.entity(12).unwrap()).unwrap();
            // Avatar.POSES: the box of each stance a player takes.
            assert_eq!(
                living.dimensions_for_pose(game, Pose::Standing),
                Some([f64::from(0.6_f32), f64::from(1.8_f32), f64::from(1.62_f32)])
            );
            assert_eq!(
                living.dimensions_for_pose(game, Pose::Crouching),
                Some([f64::from(0.6_f32), f64::from(1.5_f32), f64::from(1.27_f32)])
            );
            assert_eq!(
                living.dimensions_for_pose(game, Pose::Swimming),
                Some([f64::from(0.6_f32), f64::from(0.6_f32), f64::from(0.4_f32)])
            );
            // LivingEntity.SLEEPING_DIMENSIONS: the fixed 0.2 box, scale off.
            assert_eq!(
                living.dimensions_for_pose(game, Pose::Sleeping),
                Some([f64::from(0.2_f32), f64::from(0.2_f32), f64::from(0.2_f32)])
            );
            // The stances a player never takes answer standing, as
            // Avatar.getDefaultDimensions' fallback does.
            assert_eq!(
                living.dimensions_for_pose(game, Pose::Roaring),
                Some([f64::from(0.6_f32), f64::from(1.8_f32), f64::from(1.62_f32)])
            );
            assert!(game.living_set_attribute_base(living, "minecraft:scale", 2.0));
            assert_eq!(
                living.dimensions_for_pose(game, Pose::Dying),
                Some([f64::from(0.2_f32), f64::from(0.2_f32), f64::from(1.62_f32)]),
                "Avatar.POSES dying dimensions remain fixed even at scale 2"
            );
            game.lan.as_mut().unwrap().hub.guests.clear();
            assert_eq!(
                living.dimensions_for_pose(game, Pose::Sleeping),
                None,
                "a gone player must not acquire sleeping dimensions"
            );
        });
    }

    #[test]
    #[ignore = "daily"]
    fn camel_sitting_is_lower_even_while_standing() {
        with_game(|game| {
            floor_and_zombies(game);
            crate::game::ecs::test_support::place_test_mob(game, 20, "minecraft:camel", [0.5, 64.0, 0.5]);
            let camel = game.living(game.entity(20).unwrap()).unwrap();
            let standing = camel.dimensions_for_pose(game, Pose::Standing).unwrap();
            // Camel.getDefaultDimensions: the sitting box 1.43 lower, its eyes
            // at 0.845 (`sitting_dims`), asked of a camel that stands.
            let sitting = camel.dimensions_for_pose(game, Pose::Sitting).unwrap();
            assert_eq!(
                sitting,
                [
                    f64::from(1.7_f32),
                    f64::from(horizon_client::mobs::camel::sitting_dims(false).1),
                    f64::from(horizon_client::mobs::camel::sitting_dims(false).2)
                ]
            );
            assert!(standing[1] > sitting[1]);
        });
    }

    #[test]
    #[ignore = "daily"]
    fn goat_long_jump_scales_the_box_by_seven_tenths() {
        with_game(|game| {
            floor_and_zombies(game);
            crate::game::ecs::test_support::place_test_mob(game, 21, "minecraft:goat", [0.5, 64.0, 0.5]);
            let goat = game.living(game.entity(21).unwrap()).unwrap();
            let standing = goat.dimensions_for_pose(game, Pose::Standing).unwrap();
            let long_jump = goat.dimensions_for_pose(game, Pose::LongJumping).unwrap();
            for (asked, is) in standing.iter().zip(&long_jump) {
                assert!(
                    (is - asked * f64::from(0.7_f32)).abs() < 1.0e-6,
                    "{is} is seven tenths of {asked}"
                );
            }
        });
    }

    #[test]
    #[ignore = "daily"]
    fn no_action_time_and_pathfinding_malus_reach_the_mob() {
        with_game(|game| {
            floor_and_zombies(game);
            let mob = game.mob(game.entity(12).unwrap()).unwrap();
            assert_eq!(mob.no_action_time(game), Some(0));
            assert!(mob.set_no_action_time(game, 650));
            assert_eq!(mob.no_action_time(game), Some(650));

            // `noActionTime` is a Java `int`: the tick past `i32::MAX` wraps
            // instead of panicking. Kept so it cannot despawn meanwhile.
            assert!(mob.set_persistence_required(game));
            assert!(mob.set_no_action_time(game, i32::MAX));
            game.tick_mobs().unwrap();
            let after = mob.no_action_time(game).unwrap();
            assert!(after < 0, "wrapped past i32::MAX, got {after}");
            assert!(mob.set_no_action_time(game, 650));

            assert_eq!(mob.pathfinding_malus(game, PathType::Water), Some(8.0));
            assert!(mob.set_pathfinding_malus(game, PathType::Water, -1.0));
            assert_eq!(mob.pathfinding_malus(game, PathType::Water), Some(-1.0));
            assert_eq!(mob.pathfinding_malus(game, PathType::Lava), Some(-1.0));
            assert!(!mob.set_pathfinding_malus(game, PathType::Water, f32::NAN));
        });
    }
}
