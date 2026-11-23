//! Projectiles, fireworks, tridents and hooks on the ECS level: the launch of a projectile kind, the owner as an id, the
//! targets read by id around a segment or a blast, the hurt and the pull of an entity by id. Written, not run.
use crate::game::Game;
use crate::mobs::Who;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{bridge::host::HurtSource, targeting::Who as ShotBy};

impl Game<'_> {
    /// `ecs_launch` with the item a thrown projectile carries (`stack`, None for the kind's own item).
    pub(in crate::game) fn ecs_launch_item(&mut self, kind: &str, from: [f64; 3], velocity: [f64; 3], owner: Option<ShotBy>, stack: Option<crate::items::ItemStack>) -> Option<u32> {
        let mut next = self.next_entity;
        let id = self.ecs_with(|ecs, mw| {
            ecs.launch_item(mw, kind, from, velocity, owner, stack, &mut || {
                next += 1;
                next
            })
        });
        self.next_entity = next;
        (id != 0).then_some(id)
    }
    /// The owner a projectile records: the acting player (as `Who::Player`), or the ECS entity of a mob; None when unknown.
    pub(in crate::game) fn ecs_owner(&self, who: Option<Who>) -> Option<ShotBy> {
        match who? {
            Who::Player => self.player.as_ref().map(|p| ShotBy::Player(p.entity_id as u32)),
            Who::Mob(id) => self.ecs.entity_of(id).map(ShotBy::Entity),
        }
    }
    /// Hurts the ECS entity `id` by `amount`, from `attacker` (a player is its source by entity id).
    pub(in crate::game) fn ecs_hurt_by(&mut self, id: u32, amount: f32, attacker: Option<Who>) {
        let Some(e) = self.ecs.entity_of(id) else {
            return;
        };
        let source = match attacker {
            Some(Who::Player) => self
                .player
                .as_ref()
                .map_or(HurtSource::Generic, |p| HurtSource::Player(p.entity_id as u32)),
            Some(Who::Mob(m)) => HurtSource::Entity(m),
            None => HurtSource::Generic,
        };
        if let HurtSource::Player(player) = source {
            self.ecs_note_killer(e, player);
        }
        self.ecs_with(|ecs, mw| ecs.hurt(mw, e, amount, source));
    }
    /// `pullEntity` on the ECS entity `id`: its velocity becomes `pull` of it; false when the id is not in the level.
    pub(in crate::game) fn ecs_pull(&mut self, id: u32, pull: impl FnOnce(Vec3) -> Vec3) -> bool {
        let Some(e) = self.ecs.entity_of(id) else {
            return false;
        };
        let Some(v) = self.ecs.velocity(id) else {
            return false;
        };
        self.ecs.set_velocity(e, pull(v));
        true
    }
}
