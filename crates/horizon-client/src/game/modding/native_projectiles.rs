//! Projectile impacts for native mods (`native_projectile_impact`, NeoForge's
//! `ProjectileImpactEvent`). The ECS flies the projectiles; a mod's impact reaches
//! the host as `ArrowEvent::ModImpact` before it lands, and the host answers it
//! with `resolve_impact`.
use super::Game;
use horizon_client::{mobs::Who, projectiles::Hit};
use horizon_core::geometry::Vec3;

impl Game<'_> {
    /// Dispatch a hit for a projectile ticked outside the shared projectile list.
    /// The caller checks the server opt-in gate and exposes the live projectile
    /// before calling; misses never dispatch.
    pub(super) fn native_separate_projectile_impact(
        &mut self,
        projectile: u32,
        kind: &str,
        owner: Option<Who>,
        hit: Hit,
    ) -> bool {
        if matches!(hit, Hit::None) {
            return false;
        }
        let owner = match owner {
            Some(Who::Mob(id)) => self.native_impact_entity_id(id),
            Some(Who::Player) => self.own_entity_id() as u32,
            None => 0,
        };
        let (entity, block, face, at) = self.pearl_hit(hit);
        let entity_kind = entity
            .and_then(|id| self.entity(id))
            .and_then(|entity| self.entity_kind(entity))
            .map(|kind| kind.id(self))
            .unwrap_or_default();
        let mut canceled = false;
        self.native_projectile_impact(mod_api::__private::ProjectileImpactCall {
            dimension: self.dim.id(),
            projectile,
            kind,
            owner,
            entity,
            entity_kind: &entity_kind,
            block,
            face,
            at,
            canceled: &mut canceled,
        });
        canceled
    }

    /// Whether this game is a server (the server thread or a dedicated server,
    /// never a client) and a mod asked to hear the impacts
    /// (`native_hears_projectile_impacts`).
    pub(in crate::game) fn native_hears_impacts(&self) -> bool {
        let mut hears = false;
        self.native_hears_projectile_impacts(&mut hears);
        hears
    }

    /// The entity id of the player who threw something: the guest `guest`, or
    /// the host.
    fn native_thrower(&self, guest: Option<u32>) -> u32 {
        match guest {
            Some(conn) => self
                .lan_guest_entities()
                .into_iter()
                .find(|&(c, _)| c == conn)
                .map_or(0, |(_, id)| id),
            None => 0,
        }
    }

    /// `native_projectile_impact` for a player's thrown trident, which
    /// `tick_tridents` flies: it hit the mob
    /// `mob`, or with `mob` None the block at `block` on its `face`, at `at`.
    /// Whether a mod canceled it.
    pub(super) fn native_trident_impact(
        &mut self,
        trident: u32,
        guest: Option<u32>,
        mob: Option<u32>,
        block: [i32; 3],
        face: [i32; 3],
        at: Vec3,
    ) -> bool {
        if !self.native_hears_impacts() {
            return false;
        }
        let owner = self.native_thrower(guest);
        let (entity, entity_kind) = match mob {
            Some(id) => {
                let (entity, kind) = self.native_impact_entity(id);
                (Some(entity), kind)
            }
            None => (None, String::new()),
        };
        let dimension = self.dim.id();
        let mut canceled = false;
        self.native_projectile_impact(mod_api::__private::ProjectileImpactCall {
            dimension,
            projectile: trident,
            kind: "minecraft:trident",
            owner,
            entity,
            entity_kind: &entity_kind,
            block,
            face,
            at: [at.x, at.y, at.z],
            canceled: &mut canceled,
        });
        canceled
    }

    /// The entity id a projectile's hit on `id` stands for: a dragon part (the
    /// dragon's id + 1 + k) is the dragon, any other id its own. Allocates nothing.
    fn native_impact_entity_id(&self, id: u32) -> u32 {
        self.ecs.dragon_views()
            .into_iter()
            .map(|(d, _)| d)
            .find(|&d| (d + 1..=d + 8).contains(&id))
            .unwrap_or(id)
    }

    /// The entity id and type a projectile's hit on `id` stands for: a dragon
    /// part is the dragon, an End crystal its own id.
    fn native_impact_entity(&self, id: u32) -> (u32, String) {
        let entity = self.native_impact_entity_id(id);
        if let Some(kind) = self.ecs.kind(id) {
            return (entity, kind.into());
        }
        if let Some(k) = self.cart_genre(id) {
            return (id, k.id().into());
        }
        if self.lan_guest_entities().iter().any(|&(_, e)| e == id) {
            return (id, "minecraft:player".into());
        }
        (id, String::new())
    }

    /// `EntityTeleportEvent.EnderPearl` for `owner`'s pearl that hit `hit`:
    /// the listeners' `(target, damage, cancel)`, starting from `to` and `damage`.
    pub(in crate::game) fn native_pearl_teleport(
        &mut self,
        owner: mod_api::EntityRef,
        pearl: u32,
        from: Vec3,
        to: Vec3,
        mut damage: f32,
        hit: Hit,
    ) -> ([f64; 3], f32, bool) {
        let mut target = [to.x, to.y, to.z];
        let mut cancel = false;
        if !mod_api::__private::listened(mod_api::HookId::EnderPearlTeleport) {
            return (target, damage, cancel);
        }
        let (hit_entity, hit_block, hit_face, hit_at) = self.pearl_hit(hit);
        self.native_ender_pearl_teleport(mod_api::__private::EnderPearlTeleportCall {
            owner,
            pearl,
            from: [from.x, from.y, from.z],
            target: &mut target,
            damage: &mut damage,
            cancel: &mut cancel,
            hit_entity,
            hit_block,
            hit_face,
            hit_at,
        });
        (target, damage, cancel)
    }

    /// What a pearl hit (`getHitResult`), as `ender_pearl_teleport` tells it:
    /// `(hit_entity, hit_block, hit_face, hit_at)`. An entity's block is the
    /// cell of the hit point and its face `[0, 0, 0]`; a dragon part is the dragon.
    fn pearl_hit(&self, hit: Hit) -> (Option<u32>, [i32; 3], [i32; 3], [f64; 3]) {
        match hit {
            Hit::Block { pos, at, face } => (None, pos, face, [at.x, at.y, at.z]),
            Hit::Entity { who, at } => {
                let entity = match who {
                    Who::Mob(id) => self.native_impact_entity_id(id),
                    Who::Player => self.own_entity_id() as u32,
                };
                let cell = [
                    at.x.floor() as i32,
                    at.y.floor() as i32,
                    at.z.floor() as i32,
                ];
                (Some(entity), cell, [0; 3], [at.x, at.y, at.z])
            }
            Hit::None => (None, [0; 3], [0; 3], [0.0; 3]),
        }
    }
}

#[cfg(test)]
mod pearl_hit_tests {
    use super::*;

    /// `ender_pearl_teleport`'s hit fields: a block and its face normal, or an
    /// entity with the cell of its hit point and no face.
    #[test]
    #[ignore = "daily"]
    fn a_pearl_tells_the_block_face_or_the_entity_it_hit() {
        crate::game::native_entity_refs::tests::with_game(|game| {
            let wall = Hit::Block {
                pos: [4, 70, -2],
                at: Vec3::new(4.0, 70.5, -1.5),
                face: [-1, 0, 0],
            };
            assert_eq!(
                game.pearl_hit(wall),
                (None, [4, 70, -2], [-1, 0, 0], [4.0, 70.5, -1.5])
            );
            let cow = Hit::Entity {
                who: Who::Mob(42),
                at: Vec3::new(1.25, 64.5, -0.5),
            };
            assert_eq!(
                game.pearl_hit(cow),
                (Some(42), [1, 64, -1], [0; 3], [1.25, 64.5, -0.5])
            );
        });
    }

    /// A dragon part is the dragon and the local player is its own entity.
    #[test]
    #[ignore = "daily"]
    fn a_pearl_names_the_dragon_for_a_part_and_the_local_player_by_its_own_id() {
        crate::game::native_entity_refs::tests::with_game(|game| {
            game.ecs_with(|ecs, mw| {
                ecs.summon_dragon(
                    mw,
                    [0.0, 80.0, 0.0],
                    0.0,
                    0,
                    horizon_ecs_proto::species::ender_dragon::START_PHASE,
                    &horizon_ecs_proto::spawn_sheet::SpawnSheet::default(),
                    &mut || 225009,
                )
            });
            let at = Vec3::new(1.5, 64.5, -0.5);
            let part = Hit::Entity {
                who: Who::Mob(225009 + 1 + 2),
                at,
            };
            assert_eq!(
                game.pearl_hit(part),
                (Some(225009), [1, 64, -1], [0; 3], [1.5, 64.5, -0.5])
            );
            let player = Hit::Entity {
                who: Who::Player,
                at,
            };
            // The local player is the entity the game gives it, here as the game reports it.
            let own = game.own_entity_id() as u32;
            assert_eq!(game.pearl_hit(player).0, Some(own));
        });
    }

    /// A listener hears the hit through `native_pearl_teleport` and its write comes back.
    #[test]
    #[ignore = "daily"]
    fn a_listening_mod_hears_the_pearls_hit_and_its_target_comes_back() {
        use mod_api::{EnderPearlTeleportEvent, Flow, Hooks, ModEntry, ServerHost};
        use std::sync::{Arc, Mutex};
        type Heard = Arc<Mutex<Vec<(Option<u32>, [i32; 3])>>>;
        struct Hear(Heard);
        #[mod_api::__subscriptions]
        impl Hooks for Hear {
            fn ender_pearl_teleport(
                &self,
                _: &mut dyn ServerHost,
                event: &mut EnderPearlTeleportEvent<'_>,
            ) -> Flow {
                self.0
                    .lock()
                    .unwrap()
                    .push((event.hit_entity, event.hit_block));
                *event.target = [1.0, 2.0, 3.0];
                Flow::Continue
            }
        }
        let heard = Heard::default();
        let mods: mod_api::Mods = vec![ModEntry::new("hear", Box::new(Hear(heard.clone())))].into();
        mod_platform::testing::with_mods(mods, || {
            crate::game::native_entity_refs::tests::with_game(|game| {
                let _phase =
                    mod_api::__private::PhaseOwner::scope(mod_api::__private::ModPhase::World);
                game.ecs_with(|ecs, mw| {
                    ecs.summon_dragon(
                        mw,
                        [0.0, 80.0, 0.0],
                        0.0,
                        0,
                        horizon_ecs_proto::species::ender_dragon::START_PHASE,
                        &horizon_ecs_proto::spawn_sheet::SpawnSheet::default(),
                        &mut || 225009,
                    )
                });
                let owner = mod_api::EntityRef::new(mod_api::WorldEpoch::new(1), 7);
                let hit = Hit::Entity {
                    who: Who::Mob(225009 + 1 + 2),
                    at: Vec3::new(1.5, 64.5, -0.5),
                };
                let (target, damage, cancel) = game.native_pearl_teleport(
                    owner,
                    9,
                    Vec3::ZERO,
                    Vec3::new(8.0, 70.0, 8.0),
                    0.0,
                    hit,
                );
                assert_eq!((target, damage, cancel), ([1.0, 2.0, 3.0], 0.0, false));
                assert_eq!(*heard.lock().unwrap(), [(Some(225009), [1, 64, -1])]);
            });
        });
    }
}
