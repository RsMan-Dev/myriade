//! The ECS side of the sculk, the pistons and the lightning: the listeners are the ECS entities within reach of a game event
//! (`Select{near}`, the spatial index), the vibration goes to P's ears once (`post_vibration`, P ranges the listeners itself), the
//! bolt's hits are P's. Each `ecs_*` method is the ECS
//! branch of one function of `blocks/sculk.rs` or `entities/lightning.rs`; the host branch stays in that function's `else`.
use crate::game::Game;
use crate::mobs::{MobWorld, Who};
use bevy_ecs::entity::Entity;
use myriade_engine::ecs_bridge::EcsLevel;
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
    /// The entities that moved (`EcsLevel::portal_entries`); none moved: no mob world is built.
    pub(in crate::game) fn ecs_portal_entries(&mut self) -> Vec<(u32, bool, [f64; 3])> {
        // Besides the movers: the entities a portal holds (a stationary one is still inside it each tick) and those next to a portal
        // just lit (`Entity.applyEffectsFromBlocks` runs every tick in vanilla; here an entity enters a block by an event).
        let mut extra: Vec<u32> = self.portals.entities.keys().copied().collect();
        extra.extend_from_slice(&self.portals.lit_watch);
        if extra.is_empty() && !self.ecs.any_moving() {
            return Vec::new();
        }
        // No portal state in any loaded palette (nearly always) and no End gateway to look for: no mover can enter anything, so no
        // row is built (the flag is the portal sections' own, kept by the chunks touched).
        if extra.is_empty() && !self.in_end() && !self.level_may_hold_portal_now() {
            return Vec::new();
        }
        self.ecs_with(|ecs, mw| ecs.portal_entries(mw, &extra))
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
    pub(in crate::game) fn ecs_host_view(&self, id: u32) -> Option<myriade_engine::advancements::EntityView> {
        let v = self.ecs_view(id)?;
        Some(myriade_engine::advancements::EntityView { kind: v.kind.into(), position: v.position, ..Default::default() })
    }
    /// `ServerPlayer.awardKillScore` for the player `by` (a guest, else the acting player), on `victim` killed by no
    /// blow the host reads: the killer's stats, scores and `player_killed_entity`.
    pub(in crate::game) fn ecs_award_kill(&mut self, by: u32, victim: &myriade_engine::advancements::EntityView, blow: Option<&myriade_engine::advancements::DamageView>) {
        let guest = self.lan.as_ref().and_then(|h| h.hub.guests.iter().find(|g| g.entity_id as u32 == by).map(|g| g.conn));
        match guest {
            Some(conn) => {
                self.as_guest(conn, |g| g.award_kill_score(victim, blow));
            }
            None if self.player.as_ref().is_some_and(|me| me.entity_id as u32 == by) => self.award_kill_score(victim, blow),
            None => {}
        }
    }
    /// `Player.awardKillScore` then `ServerPlayer.awardKillScore` on the acting player (a mob: no player kill, no team).
    fn award_kill_score(&mut self, victim: &myriade_engine::advancements::EntityView, blow: Option<&myriade_engine::advancements::DamageView>) {
        self.award_stat("killed", &victim.kind, 1);
        self.score_player_killed_named(None, false);
        self.award_custom_stat("mob_kills", 1);
        self.trigger_killed_as("minecraft:player_killed_entity", victim, blow);
    }
    /// `Level.gameEvent` for the allays of P: each allay that hears `event` at `at` (its vibration listening or, for a jukebox
    /// event, its jukebox listening) is P's to apply.
    pub(in crate::game) fn ecs_allays_hear(&mut self, event: &'static str, at: [f64; 3]) {
        if !self.ecs.any_ear() {
            return;
        }
        self.ecs_with(|ecs, mw| ecs.allays_hear(mw, event, at));
    }
}
