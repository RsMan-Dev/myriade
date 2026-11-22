//! The ECS side of the sculk, the pistons and the lightning: the listeners are the ECS entities within reach of a game event
//! (`Select{near}`, the spatial index), the vibration goes to P's ears once (`post_vibration`, P ranges the listeners itself), the
//! bolt's hits are P's. Each `ecs_*` method is the ECS
//! branch of one function of `blocks/sculk.rs` or `entities/lightning.rs`; the host branch stays in that function's `else`.
use crate::game::Game;
use crate::mobs::{MobWorld, Who};
use bevy_ecs::entity::Entity;
use horizon_client::ecs_bridge::EcsLevel;
use horizon_ecs_proto::{
    behaviours::vibration_listen::{Cfg, Vibration},
    bridge::view::EntityView,
    targeting,
};
use std::mem;

impl Game<'_> {
    /// Runs `f` on the level with the mob world lent for the call: the level leaves the game for the call, as `tick_ecs` does.
    pub(in crate::game) fn ecs_with<R>(&mut self, f: impl FnOnce(&mut EcsLevel, &MobWorld<'_>) -> R) -> R {
        let mut ecs = mem::replace(&mut self.ecs, EcsLevel::inert());
        let r = f(&mut ecs, &self.mob_world_base(&[]));
        self.ecs = ecs;
        r
    }
    /// The network id of the ECS entity `e`, linked here when it has none: a listener or a mover is named by the event it posts.
    pub(in crate::game) fn ecs_id_of(&mut self, e: Entity) -> u32 {
        let next = &mut self.next_entity;
        self.ecs.id_for(e, &mut || {
            *next += 1;
            *next
        })
    }
    /// The view of the ECS entity `id`, None when it is asleep or gone.
    pub(in crate::game) fn ecs_view(&self, id: u32) -> Option<EntityView> {
        horizon_ecs_proto::bridge::view::view(self.ecs.world()?, self.ecs.entity_of(id)?)
    }
    /// Whether the ECS entity `id` is a rider (`Mount.rider` of it is set): a mob that carries another.
    pub(in crate::game) fn ecs_has_rider(&self, id: u32) -> bool {
        self.ecs_view(id).is_some_and(|v| v.rider.is_some())
    }
    /// Whether the ECS entity `id` is a vibration listener (it has `vibration_listen::Cfg`).
    pub(in crate::game) fn ecs_is_listener(&self, id: u32) -> bool {
        self.ecs.entity_of(id).is_some_and(|e| self.ecs.world().is_some_and(|w| w.get::<Cfg>(e).is_some()))
    }
    /// The P side of a host `Who`: the local player, or the ECS entity of a mob's network id (None when it has none).
    pub(in crate::game) fn ecs_who(&self, who: Who) -> Option<targeting::Who> {
        match who {
            Who::Player => self.player.as_ref().map(|p| targeting::Who::Player(p.entity_id as u32)),
            Who::Mob(id) => self.ecs.entity_of(id).map(targeting::Who::Entity),
        }
    }
    /// The vibration of a game event, given to P's ears: P ranges the listeners by its index and delays each by its distance.
    pub(in crate::game) fn ecs_post_vibration(&mut self, pos: [f64; 3], source: Option<Who>, owner: Option<Who>, projectile: bool) {
        let v = Vibration {
            pos,
            source: source.and_then(|w| self.ecs_who(w)),
            owner: owner.and_then(|w| self.ecs_who(w)),
            projectile,
        };
        self.ecs.post_vibration(v);
    }
    /// The root vehicle of the ECS entity `id`: the last vehicle up its chain, or itself when it rides nothing.
    pub(in crate::game) fn ecs_root_vehicle(&self, id: u32) -> u32 {
        let mut root = id;
        while let Some(v) = self.ecs.vehicle_id(root) {
            root = v;
        }
        root
    }
    /// The trap horse of a bolt, spawned at `at` on the ECS level; its id, linked here.
    pub(in crate::game) fn ecs_spawn_trap_horse(&mut self, at: [f64; 3]) -> Option<u32> {
        let e = self.ecs_with(|ecs, mw| {
            let e = ecs.spawn_for(mw, "minecraft:skeleton_horse", at, None, horizon_ecs_proto::spawn_reason::SpawnReason::Triggered)?;
            ecs.set_trap(e, true);
            Some(e)
        })?;
        Some(self.ecs_id_of(e))
    }
    /// The host's view of the ECS entity `id` (the kind and the position a lightning or an advancement reads), None when it has none.
    pub(in crate::game) fn ecs_host_view(&self, id: u32) -> Option<horizon_client::advancements::EntityView> {
        let v = self.ecs_view(id)?;
        Some(horizon_client::advancements::EntityView { kind: v.kind.into(), position: v.position, ..Default::default() })
    }
    /// `Level.gameEvent` for the allays of P: each allay that hears `event` at `at` (its vibration listening or, for a jukebox
    /// event, its jukebox listening) is P's to apply.
    pub(in crate::game) fn ecs_allays_hear(&mut self, event: &'static str, at: [f64; 3]) {
        self.ecs_with(|ecs, mw| ecs.allays_hear(mw, event, at));
    }
}
