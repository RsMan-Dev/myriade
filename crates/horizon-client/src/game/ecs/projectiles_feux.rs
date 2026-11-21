//! Projectiles, fireworks, tridents and hooks on the ECS level: the launch of a projectile kind, the owner as an id, the
//! targets read by id around a segment or a blast, the hurt and the pull of an entity by id. Written, not run.
use crate::game::Game;
use crate::mobs::{Species, Who};
use horizon_core::geometry::{Aabb, Vec3};
use horizon_ecs_proto::{bridge::host::HurtSource, targeting::Who as ShotBy};

impl Game<'_> {
    /// Launches `kind` from `from` at `velocity`, owned by `owner`, in the ECS level and links its network id; None when the
    /// level is inert or refuses the kind.
    pub(in crate::game) fn ecs_launch(&mut self, kind: &str, from: [f64; 3], velocity: [f64; 3], owner: Option<ShotBy>) -> Option<u32> {
        self.ecs_launch_item(kind, from, velocity, owner, None)
    }
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
        if id != 0 && self.native_hears_impacts() {
            self.ecs.set_mod_gate(id, true);
        }
        (id != 0).then_some(id)
    }
    /// The owner a projectile records: the acting player (as `Who::Player`), or the ECS entity of a mob; None when unknown.
    pub(in crate::game) fn ecs_owner(&self, who: Option<Who>) -> Option<ShotBy> {
        match who? {
            Who::Player => self.player.as_ref().map(|p| ShotBy::Player(p.entity_id as u32)),
            Who::Mob(id) => self.ecs.entity_of(id).map(ShotBy::Entity),
        }
    }
    /// The bounds of the ECS entities a segment from `from` to `to` may meet, found by the index (`ids_near`), not by a scan.
    pub(in crate::game) fn ecs_boxes(&self, from: Vec3, to: Vec3) -> Vec<(u32, Aabb)> {
        let mid = [(from.x + to.x) * 0.5, (from.y + to.y) * 0.5, (from.z + to.z) * 0.5];
        let reach = to.subtract(from).length() * 0.5 + 4.0;
        self.ecs
            .ids_near(mid, reach)
            .into_iter()
            .filter_map(|id| Some((id, self.ecs.bounds(id)?)))
            .collect()
    }
    /// The position and height of the ECS entity `id`, when the level holds it.
    pub(in crate::game) fn ecs_pose(&self, id: u32) -> Option<(Vec3, f64)> {
        let p = self.ecs.position(id)?;
        Some((Vec3::new(p[0], p[1], p[2]), f64::from(self.ecs.height(id)?)))
    }
    /// Whether the mob `id` deflects projectiles (the breeze): the tag `minecraft:deflects_projectiles` of its ECS entity, else
    /// the host's mob of the breeze's species.
    pub(in crate::game) fn deflects_projectiles(&self, id: u32) -> bool {
        if self.ecs.is_active() {
            self.ecs.has_tag(id, "minecraft:deflects_projectiles")
        } else {
            self.remote_entities.iter().any(|m| m.id == id && m.species() == Species::Breeze)
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
        let mw = self.mob_world_base(&[]);
        self.ecs.hurt(&mw, e, amount, source);
    }
    /// `pullEntity` on the ECS entity `id`: its velocity becomes `pull` of it; false when the id is not in the level.
    pub(in crate::game) fn ecs_pull(&mut self, id: u32, pull: impl FnOnce(Vec3) -> Vec3) -> bool {
        let Some(e) = self.ecs.entity_of(id) else {
            return false;
        };
        let Some(v) = self.ecs.velocity(e) else {
            return false;
        };
        self.ecs.set_velocity(e, pull(v));
        true
    }
}
