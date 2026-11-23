//! A player's blow at a ghast's fireball or a wind charge
//! (`#minecraft:redirectable_projectile`): `Player.attack` turns it with
//! `ProjectileDeflection.AIM_DEFLECT` instead of hurting anything. It flies on
//! along the player's view and is the player's now (`Projectile.deflect`
//! sets the owner), so a ghast it meets dies of it (`Ghast.isReflectedFireball`).
//! The projectiles are ECS entities read by id; the deflection itself is P's.
use super::{Game, ray_box, view_vector};
use myriade_engine::{
    mobs::Who,
    projectiles::{Projectile, ProjectileKind},
};
use horizon_core::geometry::{Aabb, Vec3};

/// `AbstractHurtingProjectile.getPickRadius`: the crosshair finds it a block
/// round its box.
const PICK_RADIUS: f64 = 1.0;

/// The tag of `#minecraft:redirectable_projectile` on an ECS projectile.
pub(in crate::game) const REDIRECTABLE_TAG: &str = "minecraft:redirectable_projectile";

/// `#minecraft:redirectable_projectile`: a ghast's fireball and both wind charges.
pub(super) fn redirectable(p: &Projectile) -> bool {
    !p.stuck
        && matches!(
            p.kind,
            ProjectileKind::LargeFireball { .. }
                | ProjectileKind::WindCharge
                | ProjectileKind::BreezeWindCharge
        )
}

impl Game<'_> {
    /// The bounds of the ECS projectile `id` as a box; None when it is no projectile.
    fn ecs_box(&self, id: u32) -> Option<Aabb> {
        let (lo, hi) = self.ecs.projectile_bounds(id)?;
        Some(Aabb::new(
            Vec3::new(lo[0], lo[1], lo[2]),
            Vec3::new(hi[0], hi[1], hi[2]),
        ))
    }
    /// `Entity.getPickRadius`: a redirectable projectile has one (1); a shulker bullet's is 0.
    fn ecs_pick_radius(&self, id: u32) -> f64 {
        if self.ecs.has_tag(id, REDIRECTABLE_TAG) {
            PICK_RADIUS
        } else {
            0.0
        }
    }
    /// Its box for the crosshair: the box of `ecs_box` grown by the pick radius.
    fn ecs_pick_box(&self, id: u32) -> Option<Aabb> {
        let r = self.ecs_pick_radius(id);
        Some(self.ecs_box(id)?.inflate(Vec3::new(r, r, r)))
    }
    /// Whether a blow turns or pops the projectile `id`: a flying redirectable one, or a flying shulker bullet (a stuck one is no target).
    fn ecs_attackable(&self, id: u32) -> bool {
        self.ecs.projectile_flying(id)
            && (self.ecs.has_tag(id, REDIRECTABLE_TAG) || self.ecs.shulker_bullet(id))
    }

    /// The redirectable projectile under the crosshair within the attack
    /// reach, by id, and how far along the view it is.
    pub(super) fn redirectable_hit(&self) -> Option<(u32, f64)> {
        let (eye, dir) = self.aim()?;
        let reach: f64 = self.entity_reach;
        let end = [0, 1, 2].map(|a| eye[a] + dir[a] * reach);
        let min = [0, 1, 2].map(|a| eye[a].min(end[a]) - PICK_RADIUS);
        let max = [0, 1, 2].map(|a| eye[a].max(end[a]) + PICK_RADIUS);
        self.ecs
            .projectiles_in(min, max)
            .into_iter()
            .filter(|&id| self.ecs_attackable(id))
            .filter_map(|id| ray_box(eye, dir, reach, self.ecs_pick_box(id)?).map(|t| (id, t)))
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }

    /// `Player.attack` on a redirectable projectile nearer than any mob or
    /// block in view: `AIM_DEFLECT` (its movement the unit view vector), the
    /// player its owner, its acceleration power back to 0.1
    /// (`onDeflection` by an attack), and the no-damage sound. True when a
    /// projectile was turned.
    pub(super) fn attack_redirectable(&mut self) -> bool {
        let Some((id, t)) = self.redirectable_hit() else {
            return false;
        };
        if self.mob_hit().is_some_and(|(_, d, _)| d < t) {
            return false;
        }
        if let (Some(hit), Some(eye)) = (self.pick_unfiltered(), self.eye_position()) {
            let d = (0..3)
                .map(|a| (hit.point[a] - eye[a]).powi(2))
                .sum::<f64>()
                .sqrt();
            if d < t {
                return false;
            }
        }
        if self.ecs.shulker_bullet(id) {
            self.hit_shulker_bullet(id);
        } else {
            self.deflect(id);
        }
        true
    }

    /// `ShulkerBullet.hurtServer` from a player's blow: the bullet is struck in the ECS (`shulker_bullet::struck`, its hurt sound,
    /// fifteen crit particles, then gone). The blow is `Player.attack` on a target that is not a mob: its sounds, then the swing.
    pub(super) fn hit_shulker_bullet(&mut self, id: u32) {
        let Some(b) = self.ecs_box(id) else {
            return;
        };
        let (_, _, _, magic) = self.melee_damage("minecraft:shulker_bullet", false, Some(id));
        let sweep = self.sweep_before_hit("minecraft:shulker_bullet", false);
        self.ecs_with(|ecs, mw| ecs.shulker_struck(mw, id));
        self.vehicle_attack_sounds(b, sweep, magic.then_some(id as i32));
        self.reset_attack_strength();
        self.swing_arm();
    }

    /// `Player.canInteractWithEntity(box, 3.0)` as the server checks an
    /// attack packet: the projectile's box within the reach plus 3 of the eyes.
    pub(super) fn within_interaction_range(&self, id: u32) -> bool {
        let (Some(eye), Some(b)) = (self.eye_position(), self.ecs_pick_box(id)) else {
            return false;
        };
        let r = self.ecs_pick_radius(id);
        let (lo, hi) = (b.min_position(), b.max_position());
        let (lo, hi) = (
            [lo.x + r, lo.y + r, lo.z + r],
            [hi.x - r, hi.y - r, hi.z - r],
        );
        let d2: f64 = (0..3)
            .map(|a| (eye[a] - eye[a].clamp(lo[a], hi[a])).powi(2))
            .sum();
        let range = self.entity_reach + 3.0;
        d2 < range * range
    }

    /// `Player.attack` on a redirectable projectile: `onAttack` resets the
    /// attack strength, then `Projectile.deflect(AIM_DEFLECT, player, player,
    /// true)`. When the projectile refuses (a wind charge in its first five
    /// ticks) the blow goes on as an ordinary attack, which a projectile does
    /// not take: `hurtOrSimulate` fails and `PLAYER_ATTACK_NODAMAGE` plays,
    /// the same sound. Either way the swing follows. True when it turned.
    pub(super) fn deflect(&mut self, id: u32) -> bool {
        self.reset_attack_strength();
        let turned = self.turn_projectile(id);
        self.swing_arm();
        let from = self
            .player
            .as_ref()
            .map(|p| [p.position.x, p.position.y, p.position.z]);
        self.audio
            .play("minecraft:entity.player.attack.nodamage", from, 1.0, 1.0);
        turned
    }

    /// The deflection itself, in the ECS (`EcsLevel::deflect`, `Projectile.deflect` of P): the view as the new movement, the player as
    /// owner. False, and nothing changes, when the projectile refuses.
    fn turn_projectile(&mut self, id: u32) -> bool {
        let dir = view_vector(self.yaw, self.pitch);
        let by = self.ecs_owner(Some(Who::Player));
        self.ecs
            .deflect(id, by, Vec3::new(dir[0], dir[1], dir[2]))
    }
}
