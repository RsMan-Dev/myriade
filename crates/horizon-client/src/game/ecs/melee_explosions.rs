//! The melee hit and the blast on the ECS level (lot 16). `attack_entity` is the ECS side of `attack_mob`: the damage is the host's
//! (`melee_damage_with_admission`), the blow is `EcsLevel::hurt`, the knockback is a new velocity on the creature. `explode_ecs` hands
//! a blast to P (`EcsLevel::explode`): P hurts and pushes the creatures it reaches (P/explosion.rs:147-173), hurts the players in reach
//! (P/explosion.rs:182-200) and breaks their blocks (`BlockChanges`, next step). The host keeps what P does not (see `explosion.rs`).
use crate::game::{Game, ray_box};
use horizon_client::{ecs_bridge::EcsLevel, mobs::{MobWorld, Species, Who}, text::Text};
use horizon_core::geometry::{Aabb, Vec3};
use horizon_ecs_proto::{
    bridge::host::HurtSource,
    projectile::Blast,
    targeting,
};

impl Game<'_> {
    /// Lends the ECS level to `f` with the world's view of the game, as `tick_ecs` does, so the view does not borrow `self.ecs`.
    fn ecs_lend_world<R>(&mut self, f: impl FnOnce(&mut EcsLevel, &MobWorld<'_>) -> R) -> R {
        let mut ecs = std::mem::replace(&mut self.ecs, EcsLevel::inert());
        let r = f(&mut ecs, &self.mob_world_base(&[]));
        self.ecs = ecs;
        r
    }

    /// `attack_mob` for the ECS creature `id`. False when `id` is no creature of the level.
    pub(in crate::game) fn attack_entity(&mut self, id: u32) -> bool {
        let (Some(e), Some(kind)) = (self.ecs.entity_of(id), self.ecs.kind(id)) else {
            return false;
        };
        let attacker = self.player.as_ref().map_or(0, |p| p.entity_id as u32);
        let ((amount, scale, crit, magic), admitted) = self.melee_damage_with_admission(kind, true, Some(id));
        // Player.onAttack: the hit starts the attack strength again, hurt or not.
        self.attack_ticks = 0;
        if !admitted {
            return true;
        }
        let (knockback, knockback_attack) = self.extra_knockback(kind, scale);
        let from = self.player.as_ref().map(|p| [p.position.x, p.position.y, p.position.z]);
        if knockback_attack {
            self.audio.play("minecraft:entity.player.attack.knockback", from, 1.0, 1.0);
        }
        // LivingEntity.setLastHurtMob (tamed wolves join in).
        self.combat.last_hurt_mob = Some((id, self.world.game_time));
        // Player.attack: the sweep is decided before the hit, on the host's rule (`sweep_attack_position`).
        let sweep = self.sweep_attack_position(scale, crit, kind);
        let area = self.mob_box(id);
        let old_health = self.ecs.health(id).unwrap_or(0.0);
        self.ecs_note_killer(e, attacker);
        self.ecs_lend_world(|l, mw| l.hurt(mw, e, amount, HurtSource::Player(attacker)));
        // `damage_dealt` counts what the hit took off the creature's health.
        self.mace_hurt_enemy_ecs(id);
        let dealt = old_health - self.ecs.health(id).unwrap_or(0.0);
        self.award_custom_stat("damage_dealt", (dealt * 10.0).round() as i32);
        if let Some(at) = sweep {
            // Player.doSweepAttack: the sound, the victims around the target (ECS ones included, `sweep_damage`) and the arc.
            self.sweep_attack(at, from, area.map(|a| (a, crate::game::mace::Spared::Mob(id))), scale);
        }
        // The crit and magic emitters around the creature's box.
        if let Some(area) = self.mob_box(id).filter(|_| crit || magic) {
            if crit {
                self.area_critical_hit(id as i32, area, false);
            }
            if magic {
                self.area_critical_hit(id as i32, area, true);
            }
        }
        if knockback > 0.0 {
            let (xd, zd) = self.extra_knockback_direction();
            let v = self.ecs.motion(id).map_or([0.0; 3], |m| m.velocity);
            let [x, y, z] = horizon_client::legacy_combat::extra_knockback(v, f64::from(knockback), (xd, zd));
            self.ecs.set_velocity(e, Vec3::new(x, y, z));
            self.stop_sprint_after_extra_knockback();
        }
        true
    }

    /// The blast on the ECS level: `source` is the exploding creature, spared by its own blast; the indirect source (a TNT's owner, the mob
    /// whose blast it is) is the blast's owner, so mob griefing spares the vehicles and the items of a mob's blast. P hurts and pushes
    /// what is in reach: creatures, items, orbs, carts, boats, projectiles. Blocks stay the host's (`interaction`), so P breaks none.
    /// The boats' blows come back from P and are played through `hurt_boat_result` (the mods' `vehicle_damage` hook, ENTITY_DAMAGE).
    pub(in crate::game) fn explode_ecs(&mut self, center: [f64; 3], radius: f32, fire: bool, source: Option<u32>) {
        let at = Vec3::new(center[0], center[1], center[2]);
        let source = source.and_then(|id| self.ecs.entity_of(id));
        let owner = match self.blast_indirect_source() {
            Some(Who::Player) => self.acting_player_uuid_id().map(targeting::Who::Player),
            Some(Who::Mob(id)) => self.ecs.entity_of(id).map(targeting::Who::Entity),
            _ => None,
        };
        let blast = Blast { at, radius, fire, owner, source };
        let drops = self.session.rules.bool_at(horizon_client::gamerules::vanilla::ENTITY_DROPS);
        // `ItemStack.canBeHurtBy`: the stacks that resist an explosion take no damage from it.
        let stacks: Vec<(u32, _)> = self
            .ecs
            .ids_near(center, f64::from(radius * 2.0))
            .into_iter()
            .filter_map(|id| self.ecs.item_stack(id).map(|s| (id, s)))
            .collect();
        let resistant: Vec<u32> = stacks
            .into_iter()
            .filter(|(_, s)| self.stack_can_be_hurt_by(s, "minecraft:explosion") == Some(false))
            .map(|(id, _)| id)
            .collect();
        self.ecs.spare_from_blast(&resistant);
        self.ecs.route_boat_blows(true);
        self.ecs_lend_world(|l, mw| {
            l.set_entity_drops(drops);
            l.explode_with(mw, blast, false)
        });
        for (id, damage, attacker) in self.ecs.take_boat_blows() {
            self.hurt_boat_result(id, damage, false, attacker);
        }
    }

    /// `mob_hit` for the ECS creatures: the nearest one the aim ray meets within reach, as (id, distance).
    pub(in crate::game) fn ecs_mob_hit(&self) -> Option<(u32, f64)> {
        let (eye, dir) = self.aim()?;
        let reach = self.entity_reach;
        let mut best: Option<(u32, f64)> = None;
        for id in self.ecs.ids_near(eye, reach) {
            if self.creature_species(id).is_none() {
                continue;
            }
            let Some(bx) = self.mob_box(id) else {
                continue;
            };
            if let Some(d) = ray_box(eye, dir, reach, bx) && best.is_none_or(|b| d < b.1) {
                best = Some((id, d));
            }
        }
        best
    }

    /// The creature a spin attack's swept box `swept` meets, the riding one spared: the ECS's, found by `ids_near` around the box.
    pub(in crate::game) fn ecs_spin_target(&self, swept: Aabb, riding: Option<u32>) -> Option<u32> {
        let (lo, hi) = (swept.min_position(), swept.max_position());
        let at = [(lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0, (lo.z + hi.z) / 2.0];
        let reach = (hi.x - lo.x).max(hi.z - lo.z).max(hi.y - lo.y) / 2.0 + 4.0;
        self.ecs.ids_near(at, reach).into_iter().find(|&id| {
            Some(id) != riding
                && self.creature_species(id).is_some()
                && self.mob_box(id).is_some_and(|b| b.intersects(swept))
        })
    }

    /// True when `id` is a creature (`live`: and alive): the ECS's, or the host's mobs outside the ECS.
    pub(in crate::game) fn creature_is(&self, id: u32, live: bool) -> bool {
        if !self.ecs.is_active() {
            return self.remote_entities.iter().any(|m| m.id == id && (!live || m.alive()));
        }
        self.ecs.exists(id) && self.ecs.kind(id).is_some() && (!live || self.ecs.alive(id))
    }

    /// The species of the creature `id`: from its mob type on the ECS (kinds are already namespaced), else the host mob's.
    pub(in crate::game) fn creature_species(&self, id: u32) -> Option<Species> {
        if !self.ecs.is_active() {
            return self.remote_entities.iter().find(|m| m.id == id).map(|m| m.species());
        }
        let kind = self.ecs.kind(id)?;
        self.res.mob_types().get(kind).map(|t| t.species)
    }

    /// `mob_display_name` for the creature `id`: the translated kind name on the ECS (custom names are not on the bridge yet).
    pub(in crate::game) fn blast_creature_name(&self, id: u32) -> Option<Text> {
        if !self.ecs.is_active() {
            return self.mob_display_name(id);
        }
        let kind = self.ecs.kind(id)?;
        Some(Text::translate(&format!("entity.{}", kind.replace(':', ".")), &[]))
    }
}
