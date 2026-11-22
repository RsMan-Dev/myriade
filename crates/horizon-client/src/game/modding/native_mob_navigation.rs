//! Navigation helpers for native mod mob handles.
use super::{Game, commands::Target};
use mod_api::{MobRef, MoveToPathError, PathType};

/// The numbers a mod hands a mob's navigation (`moveTo(x, y, z, speed)`): a
/// finite speed, and finite coordinates small enough for the pathfinder's
/// block arithmetic. The mob handles' boundary, shared with a goal tick's own
/// `move_to` (arismoko/horizon#1077) — one check, not a copy per caller.
pub(super) fn navigation_move_accepts(at: [f64; 3], speed: f64) -> bool {
    speed.is_finite()
        && at
            .iter()
            .all(|v| v.is_finite() && v.abs() <= 2_000_000_000.0)
}

/// The coordinates a mod hands a mob's look control (`setLookAt(x, y, z)`):
/// finite ones — the same boundary the navigation takes, for the control
/// whose input it shares.
pub(super) fn navigation_look_accepts(at: [f64; 3]) -> bool {
    at.iter().all(|v| v.is_finite())
}

fn navigation_flags_supported(mob: &horizon_client::mobs::Mob) -> bool {
    use horizon_client::mobs::nav::NavKind;
    matches!(
        mob.ai.nav.kind,
        NavKind::Ground | NavKind::WallClimber | NavKind::Flying
    )
}

/// The nodes a mod hands `PathNavigation.moveTo(Path, speed)`: finite
/// coordinates and a finite speed. Nothing is refused for being unloaded or
/// outside the world, and the node count is unbounded (vanilla's `moveTo`
/// carries no cap); a node outside the block-coordinate range is floored to
/// the saturating integer and reads as void air, as vanilla's `BlockPos` cast
/// does.
pub(super) fn supplied_path_accepts(nodes: &[mod_api::PathNode], speed: f64) -> bool {
    speed.is_finite()
        && nodes
            .iter()
            .all(|node| node.x.is_finite() && node.y.is_finite() && node.z.is_finite())
}

/// The refusal a `false` from `nav_move_to_supplied` stands for once the
/// numbers and the mob are known to be fine: `PathNavigation.moveTo(Path)`
/// refuses a path with no node left, an empty one or one the trim emptied.
/// One conversion, shared by the handle and by a goal tick's view (arismoko/horizon#4410).
pub(super) fn supplied_path_refusal(followed: bool) -> Result<(), MoveToPathError> {
    if followed {
        Ok(())
    } else {
        Err(MoveToPathError::Empty)
    }
}

impl Game<'_> {
    /// `PathNavigation.moveTo(x, y, z, speed)`: true if a path was started.
    /// Refuses a stale or unavailable mob, a client, non-finite arguments,
    /// and coordinates too large for safe block arithmetic. As in vanilla,
    /// the mob's own goals may replace the path next tick. False for the
    /// dragon, which flies by its phase and keeps no navigation.
    pub(super) fn mob_move_to(&mut self, mob: MobRef, at: [f64; 3], speed: f64) -> bool {
        if !navigation_move_accepts(at, speed) {
            return false;
        }
        let Some(Target::Mob(id)) = self.native_entity_target(mob.entity()) else {
            return false;
        };
        if self.ecs.is_active() {
            // The pathfinder runs in the ECS with the mob world lent; the mob's own navigation is its state there.
            return self.ecs_mod_live(id).is_some()
                && self.ecs_with_mw(|ecs, mw| ecs.nav_move_to(mw, id, at, speed));
        }
        let Some(index) = self.remote_entities.iter().position(|m| m.id == id) else {
            return false;
        };
        // The pathfinder reads terrain only. Take the mob list so its selected
        // mob can be mutably borrowed alongside the shared MobWorld, just as
        // the mob tick does. No mod hook fires during pathfinding.
        let mut mobs = self.remote_entities.lift();
        #[cfg(test)]
        super::entity::taken_pass_tests::after_take(self);
        let moved = mobs[index].nav_move_to(&self.mob_world_base(&[]), at, speed);
        self.remote_entities
            .restore(mobs, horizon_client::join_seq::RestoreOrder::SurvivorsFirst);
        moved
    }

    /// `PathNavigation.moveTo(Path, speed)`: follow a path the mod computed.
    /// Each node is floored to its block, classified by the engine on submit
    /// (the classification never filters the path), trimmed as every `moveTo`
    /// trims, and followed as given. A supplied path keeps no recompute
    /// target, so a block change on it or the stuck check drops it without a
    /// search. Only non-finite coordinates and speeds are refused; the node
    /// count is unbounded, unloaded or out-of-world nodes are neither refused
    /// nor clamped, and an extreme finite node is classified and followed with
    /// saturating block arithmetic. `Unavailable` for a stale or unavailable
    /// mob, a client, and the dragon; `Empty` when no node is left to follow.
    pub(super) fn mob_move_to_path(
        &mut self,
        mob: MobRef,
        nodes: Vec<mod_api::PathNode>,
        speed: f64,
    ) -> Result<(), MoveToPathError> {
        if !supplied_path_accepts(&nodes, speed) {
            return Err(MoveToPathError::NotFinite);
        }
        let Some(Target::Mob(id)) = self.native_entity_target(mob.entity()) else {
            return Err(MoveToPathError::Unavailable);
        };
        if self.ecs.is_active() {
            if self.ecs_mod_live(id).is_none() {
                return Err(MoveToPathError::Unavailable);
            }
            let nodes: Vec<[f64; 3]> = nodes.iter().map(|n| [n.x, n.y, n.z]).collect();
            let followed = self.ecs_with_mw(|ecs, mw| ecs.nav_move_to_supplied(mw, id, &nodes, speed));
            return supplied_path_refusal(followed);
        }
        let Some(index) = self.remote_entities.iter().position(|m| m.id == id) else {
            return Err(MoveToPathError::Unavailable);
        };
        let nodes: Vec<[f64; 3]> = nodes.iter().map(|n| [n.x, n.y, n.z]).collect();
        // The pathfinder reads terrain only. Take the mob list so its selected
        // mob can be mutably borrowed alongside the shared MobWorld, as the
        // mob tick does. No mod hook fires during classification.
        let mut mobs = self.remote_entities.lift();
        #[cfg(test)]
        super::entity::taken_pass_tests::after_take(self);
        let followed = mobs[index].nav_move_to_supplied(&self.mob_world_base(&[]), &nodes, speed);
        self.remote_entities
            .restore(mobs, horizon_client::join_seq::RestoreOrder::SurvivorsFirst);
        supplied_path_refusal(followed)
    }

    /// `PathNavigation.stop`. False for a stale or unavailable mob, or on a
    /// client, and for the dragon (no navigation).
    pub(super) fn mob_stop(&mut self, mob: MobRef) -> bool {
        let Some(Target::Mob(id)) = self.native_entity_target(mob.entity()) else {
            return false;
        };
        if self.ecs.is_active() {
            return self.ecs_mod_live(id).is_some() && self.ecs.nav_stop(id);
        }
        let Some(mob) = self.remote_entities.iter_mut().find(|m| m.id == id) else {
            return false;
        };
        mob.nav_stop();
        true
    }

    /// `PathNavigation.isDone`; None for a stale or unavailable mob, or on a
    /// client, and for the dragon (no navigation).
    pub(super) fn mob_is_done(&self, mob: MobRef) -> Option<bool> {
        let Target::Mob(id) = self.native_entity_target(mob.entity())? else {
            return None;
        };
        if self.ecs.is_active() {
            return self.ecs_mod_live(id).and(self.ecs.nav_is_done(id));
        }
        Some(self.remote_entities.iter().find(|m| m.id == id)?.ai.nav.is_done())
    }

    /// `PathNavigation.getSpeedModifier`: the multiplier the current path
    /// is followed at (1.0 is the mob's normal speed; 0 until a path sets
    /// one). This is not the mob's movement-speed attribute. None for a
    /// stale or unavailable mob, or on a client, and for the dragon.
    pub(super) fn mob_navigation_speed(&self, mob: MobRef) -> Option<f64> {
        let Target::Mob(id) = self.native_entity_target(mob.entity())? else {
            return None;
        };
        if self.ecs.is_active() {
            return self.ecs_mod_live(id).and(self.ecs.nav_speed_modifier(id));
        }
        Some(self.remote_entities.iter().find(|m| m.id == id)?.ai.nav.speed_modifier)
    }

    /// The live supported navigation's `can_open_doors` flag; unavailable mobs refuse.
    pub(super) fn mob_can_open_doors(&self, mob: MobRef) -> Option<bool> {
        let Target::Mob(id) = self.native_entity_target(mob.entity())? else {
            return None;
        };
        if self.ecs.is_active() {
            return self.ecs_mod_live(id).and(self.ecs.nav_flag(id, "can_open_doors"));
        }
        let mob = self.remote_entities.iter().find(|m| m.id == id && !m.removed)?;
        navigation_flags_supported(mob).then_some(mob.ai.nav.can_open_doors)
    }

    /// `PathNavigation.setCanOpenDoors`; leaves the current path intact.
    pub(super) fn mob_set_can_open_doors(&mut self, mob: MobRef, value: bool) -> bool {
        let Some(Target::Mob(id)) = self.native_entity_target(mob.entity()) else {
            return false;
        };
        if self.ecs.is_active() {
            return self.ecs_mod_live(id).is_some() && self.ecs.set_nav_flag(id, "can_open_doors", value);
        }
        let Some(mob) = self.remote_entities.iter_mut().find(|m| m.id == id && !m.removed) else {
            return false;
        };
        if !navigation_flags_supported(mob) {
            return false;
        }
        mob.ai.nav.can_open_doors = value;
        true
    }

    /// The live supported navigation's `can_pass_doors` flag; unavailable mobs refuse.
    pub(super) fn mob_can_pass_doors(&self, mob: MobRef) -> Option<bool> {
        let Target::Mob(id) = self.native_entity_target(mob.entity())? else {
            return None;
        };
        if self.ecs.is_active() {
            return self.ecs_mod_live(id).and(self.ecs.nav_flag(id, "can_pass_doors"));
        }
        let mob = self.remote_entities.iter().find(|m| m.id == id && !m.removed)?;
        navigation_flags_supported(mob).then_some(mob.ai.nav.can_pass_doors)
    }

    /// Sets the pass-door navigation wish; leaves the current path intact.
    pub(super) fn mob_set_can_pass_doors(&mut self, mob: MobRef, value: bool) -> bool {
        let Some(Target::Mob(id)) = self.native_entity_target(mob.entity()) else {
            return false;
        };
        if self.ecs.is_active() {
            return self.ecs_mod_live(id).is_some() && self.ecs.set_nav_flag(id, "can_pass_doors", value);
        }
        let Some(mob) = self.remote_entities.iter_mut().find(|m| m.id == id && !m.removed) else {
            return false;
        };
        if !navigation_flags_supported(mob) {
            return false;
        }
        mob.ai.nav.can_pass_doors = value;
        true
    }

    /// The live supported navigation's `can_float` flag; unavailable mobs refuse.
    pub(super) fn mob_can_float(&self, mob: MobRef) -> Option<bool> {
        let Target::Mob(id) = self.native_entity_target(mob.entity())? else {
            return None;
        };
        if self.ecs.is_active() {
            return self.ecs_mod_live(id).and(self.ecs.nav_flag(id, "can_float"));
        }
        let mob = self.remote_entities.iter().find(|m| m.id == id && !m.removed)?;
        navigation_flags_supported(mob).then_some(mob.ai.nav.can_float)
    }

    /// `PathNavigation.setCanFloat`; leaves the current path intact.
    pub(super) fn mob_set_can_float(&mut self, mob: MobRef, value: bool) -> bool {
        let Some(Target::Mob(id)) = self.native_entity_target(mob.entity()) else {
            return false;
        };
        if self.ecs.is_active() {
            return self.ecs_mod_live(id).is_some() && self.ecs.set_nav_flag(id, "can_float", value);
        }
        let Some(mob) = self.remote_entities.iter_mut().find(|m| m.id == id && !m.removed) else {
            return false;
        };
        if !navigation_flags_supported(mob) {
            return false;
        }
        mob.ai.nav.can_float = value;
        true
    }

    /// `Mob.getPathfindingMalus`: what a node of `kind` costs this mob's
    /// pathfinder — its own override, else the kind's default (a negative
    /// malus closes the kind). None for a stale or unavailable mob, the
    /// dragon, or on a client.
    pub(super) fn mob_pathfinding_malus(&self, mob: MobRef, kind: PathType) -> Option<f32> {
        let Target::Mob(id) = self.native_entity_target(mob.entity())? else {
            return None;
        };
        if self.ecs.is_active() {
            return self.ecs_mod_live(id).and(self.ecs.malus(id, kind));
        }
        Some(
            self.remote_entities
                .iter()
                .find(|m| m.id == id && !m.removed)?
                .malus(kind),
        )
    }

    /// `Mob.setPathfindingMalus`: what the mob's next paths cost through
    /// `kind` (negative closes it, as the species' own overrides do); not
    /// saved, as in vanilla, so a reload or conversion keeps the species'
    /// own. False for a malus that is not finite, a stale or unavailable mob,
    /// the dragon, or on a client.
    pub(super) fn mob_set_pathfinding_malus(
        &mut self,
        mob: MobRef,
        kind: PathType,
        malus: f32,
    ) -> bool {
        if !malus.is_finite() {
            return false;
        }
        let Some(Target::Mob(id)) = self.native_entity_target(mob.entity()) else {
            return false;
        };
        if self.ecs.is_active() {
            return self.ecs_mod_live(id).is_some() && self.ecs.set_malus(id, kind, malus);
        }
        self.remote_entities
            .iter_mut()
            .find(|m| m.id == id && !m.removed)
            .map(|m| m.set_malus(kind, malus))
            .is_some()
    }

    /// `PathNavigation.setSpeedModifier`: sets the multiplier the current
    /// path is followed at, as `move_to`'s speed argument does. The mob's
    /// own AI — a goal's or a brain species' next `moveTo` — sets its own
    /// again, as in vanilla. False for a non-finite speed, a stale or
    /// unavailable mob, or on a client, and for the dragon.
    pub(super) fn mob_set_navigation_speed(&mut self, mob: MobRef, speed: f64) -> bool {
        if !speed.is_finite() {
            return false;
        }
        let Some(Target::Mob(id)) = self.native_entity_target(mob.entity()) else {
            return false;
        };
        if self.ecs.is_active() {
            return self.ecs_mod_live(id).is_some() && self.ecs.set_nav_speed_modifier(id, speed);
        }
        self.remote_entities
            .iter_mut()
            .find(|m| m.id == id)
            .map(|m| m.ai.nav.speed_modifier = speed)
            .is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::super::native_entity_refs::tests::with_game;
    use super::supplied_path_accepts;
    use horizon_client::{chunk::Chunk, mobs::Mob};
    use horizon_core::geometry::Vec3;
    use mod_api::{EntityRef, MobRef, MoveToPathError, PathNode, WorldEpoch};
    use std::sync::Arc;

    /// `moveTo(Path, speed)`'s boundary never gets stricter than vanilla's:
    /// finite coordinates and speed pass, a node count far above any vanilla
    /// search passes, and coordinates outside the block range are not refused
    /// (they are floored with the saturating cast and read as void air).
    #[test]
    fn a_supplied_path_only_refuses_non_finite_numbers() {
        let node = |x: f64, y: f64, z: f64| PathNode { x, y, z };
        assert!(supplied_path_accepts(&[], 1.0), "an empty path is taken");
        assert!(supplied_path_accepts(
            &[node(1.5, 64.0, -2.5), node(2.0, 64.0, -2.0)],
            0.3
        ));
        assert!(
            supplied_path_accepts(&[node(3.0e7, -3.0e7, 30_000_000.5)], 1.0),
            "past 30,000,000 is not refused"
        );
        assert!(
            supplied_path_accepts(&[node(1.0e300, -1.0e300, 0.0)], 1.0),
            "any finite coordinate is taken, floored by the saturating cast"
        );
        assert!(!supplied_path_accepts(&[node(f64::NAN, 64.0, 0.0)], 1.0));
        assert!(!supplied_path_accepts(
            &[node(0.0, f64::INFINITY, 0.0)],
            1.0
        ));
        assert!(!supplied_path_accepts(
            &[node(0.0, 64.0, f64::NEG_INFINITY)],
            1.0
        ));
        assert!(!supplied_path_accepts(&[node(0.0, 64.0, 0.0)], f64::NAN));
        assert!(
            !supplied_path_accepts(&[node(0.0, 64.0, 0.0)], f64::INFINITY),
            "a non-finite speed is refused as `move_to` refuses it"
        );
    }

    /// A refusal names its cause, so a mod tells an invalid number from a mob
    /// that is not there and from a path with nothing left to follow (arismoko/horizon#4443).
    #[test]
    #[ignore = "daily"]
    fn a_supplied_path_refusal_names_its_cause() {
        with_game(|game| {
            let kind = game
                .res
                .mob_types()
                .get("minecraft:zombie")
                .unwrap()
                .clone();
            game.remote_entities
                .push(Mob::new(99, kind, Vec3::new(3.5, 65.0, 3.5), 0.0));
            let entity = game.entity(99).unwrap();
            let mob = game.mob(entity).unwrap();
            let node = |x: f64, y: f64, z: f64| PathNode { x, y, z };
            assert_eq!(
                mob.move_to_path(game, vec![node(f64::NAN, 65.0, 3.5)], 1.0),
                Err(MoveToPathError::NotFinite)
            );
            assert_eq!(
                mob.move_to_path(game, Vec::new(), 1.0),
                Err(MoveToPathError::Empty),
                "an empty path has no node left to follow"
            );
            assert_eq!(
                mob.move_to_path(game, vec![node(3.5, 65.0, 3.5), node(4.5, 65.0, 3.5)], 1.0),
                Ok(()),
                "a path the navigation took answers Ok"
            );
            assert_eq!(mob.is_done(game), Some(false));
            let stale = MobRef::new(EntityRef::new(
                WorldEpoch::new(entity.epoch().get() + 1),
                99,
            ));
            assert_eq!(
                stale.move_to_path(game, vec![node(3.5, 65.0, 3.5)], 1.0),
                Err(MoveToPathError::Unavailable)
            );
        });
    }

    #[test]
    #[ignore = "daily"]
    fn navigation_speed_is_the_paths_speed_modifier() {
        with_game(|game| {
            let kind = game
                .res
                .mob_types()
                .get("minecraft:zombie")
                .unwrap()
                .clone();
            game.remote_entities
                .push(Mob::new(99, kind, Vec3::new(3.5, 65.0, 3.5), 0.0));
            let entity = game.entity(99).unwrap();
            let mob = game.mob(entity).unwrap();
            // A fresh navigation has no path, so no speed was set for one.
            assert_eq!(mob.navigation_speed(game), Some(0.0));
            assert!(mob.set_navigation_speed(game, 1.25));
            assert_eq!(mob.navigation_speed(game), Some(1.25));
            assert_eq!(game.remote_entities[0].ai.nav.speed_modifier, 1.25);
            assert!(!mob.set_navigation_speed(game, f64::NAN));
            assert_eq!(game.remote_entities[0].ai.nav.speed_modifier, 1.25);
            // Not the movement-speed attribute.
            let attribute = game.remote_entities[0].movement_speed();
            assert_ne!(attribute, 1.25);
            let stale = MobRef::new(EntityRef::new(
                WorldEpoch::new(entity.epoch().get() + 1),
                99,
            ));
            assert_eq!(stale.navigation_speed(game), None);
            assert!(!stale.set_navigation_speed(game, 1.0));
            assert_eq!(game.remote_entities[0].ai.nav.speed_modifier, 1.25);
        });
    }
}
