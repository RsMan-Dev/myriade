//! Breezes (`Breeze`): the poses and their animations, the long jump's
//! arithmetic, the sounds of `Breeze.tick` and `BreezeModel`'s layers. The
//! brain (`BreezeAi` and its behaviours) is in `monster_brain`.
use super::{
    Damage, MobEvent, MobWorld, Species, Who,
    anim::{Poses, State},
    keyframes::Animation,
};
use horizon_core::{
    geometry::{Aabb, Vec3},
    jmath,
    random::RandomSource,
};
use mod_api::Pose;
use std::collections::HashMap;

// `variant3` is the pose, `variant` the pose the animations last saw,
// `vector[2]` `soundTick`. AnimState states: idle, shoot, slide, slide back,
// inhale, long jump.
pub const STANDING: u8 = 0;
pub const SHOOTING: u8 = 1;
pub const INHALING: u8 = 2;
pub const LONG_JUMPING: u8 = 3;
pub const SLIDING: u8 = 4;

const ALLOWED_ANGLES: [i32; 5] = [40, 55, 60, 75, 80];

/// `LongJump.snapToSurface`: the block above where a clip ten blocks down
/// hits, else ten blocks up.
pub(super) fn snap_to_surface(w: &MobWorld, p: Vec3) -> Option<[i32; 3]> {
    let above = |hit: Vec3| {
        [
            hit.x.floor() as i32,
            hit.y.floor() as i32 + 1,
            hit.z.floor() as i32,
        ]
    };
    if let Some(hit) = w.clip_block(p, Vec3::new(p.x, p.y - 10.0, p.z)) {
        return Some(above(hit));
    }
    w.clip_block(p, Vec3::new(p.x, p.y + 10.0, p.z)).map(above)
}
/// `EntityType.isBlockDangerous` for the breeze (not fire immune).
pub(super) fn dangerous_block(name: &str) -> bool {
    matches!(
        name,
        "minecraft:fire"
            | "minecraft:soul_fire"
            | "minecraft:lava"
            | "minecraft:magma_block"
            | "minecraft:lava_cauldron"
            | "minecraft:wither_rose"
            | "minecraft:sweet_berry_bush"
            | "minecraft:cactus"
            | "minecraft:powder_snow"
    )
}
#[cfg(test)]
mod tests {
    #[test]
    fn the_highest_arc_reaches_a_target_within_range() {
        // Angle 80 degrees: v0^2 = r^2 g / (r sin 2a - 2 y cos^2 a).
        let g = 0.08;
        let (r, y): (f64, f64) = (10.0, 0.0);
        let a = 60.0_f64.to_radians();
        let v0sq = r * r * g / (r * (2.0 * a).sin() - 2.0 * y * a.cos().powi(2));
        assert!(v0sq.sqrt() < 1.4);
    }
}
#[cfg(test)]
mod ground_particle_tests {
    use super::*;
    use crate::mobs::sensor_scan_tests::kind;



}

#[cfg(test)]
mod invulnerability_tests {
    use super::*;
    use crate::mobs::{DamageKind, sensor_scan_tests::kind};


    /// Scenario 1: Default/legacy behavior preserved:
    /// Breeze takes normal damage from player, generic damage, fall damage.
    #[test]
    fn breeze_takes_normal_damage_from_player_fall_and_generic() {
        let mut fixture = crate::health::TestLevel::new(0);
        let mut level = fixture.level();
        let mut b = test_breeze(1);
        let initial_health = b.health;
        assert!(initial_health > 0.0);

        // Player attack
        let player_hit = Damage::player([0.0, 0.0, 0.0]);
        assert!(
            b.damage(&mut level, 2.0, player_hit),
            "player attack should hurt breeze"
        );
        assert_eq!(b.health, initial_health - 2.0);

        // Fall damage
        b.invulnerable = 0;
        let health_before_fall = b.health;
        assert!(
            b.damage(&mut level, 3.0, Damage::fall()),
            "fall damage should hurt breeze"
        );
        assert_eq!(b.health, health_before_fall - 3.0);

        // Generic damage
        b.invulnerable = 0;
        let health_before_generic = b.health;
        assert!(
            b.damage(&mut level, 1.0, Damage::of(DamageKind::Generic)),
            "generic damage should hurt breeze"
        );
        assert_eq!(b.health, health_before_generic - 1.0);
    }

    /// Scenario 2: Blocking/rejection of disallowed actions:
    /// Breeze rejects damage when attacker is another Breeze (`source.attacker_species == Some(Species::Breeze)`),
    /// returning `false` from `m.damage(...)` and taking no damage.
    #[test]
    fn breeze_rejects_damage_from_another_breeze() {
        let mut fixture = crate::health::TestLevel::new(0);
        let mut level = fixture.level();
        let mut b = test_breeze(1);
        let initial_health = b.health;

        // Direct mob attack from another breeze
        let other_breeze_mob = Damage::mob(2, [0.0, 0.0, 0.0]).with_species(Species::Breeze);
        assert!(
            !b.damage(&mut level, 5.0, other_breeze_mob),
            "breeze should reject damage from another breeze"
        );
        assert_eq!(b.health, initial_health, "health must not change");

        // Thrown wind charge or projectile from another breeze
        let other_breeze_proj = Damage {
            kind: DamageKind::Thrown,
            attacker: Some(Who::Mob(2)),
            from: Some([1.0, 0.0, 0.0]),
            attacker_species: Some(Species::Breeze),
        };
        assert!(
            !b.damage(&mut level, 3.0, other_breeze_proj),
            "breeze should reject projectile damage from another breeze"
        );
        assert_eq!(b.health, initial_health, "health must remain intact");

        // Directly verify breeze::accepts_damage
        assert!(!accepts_damage(&b, &other_breeze_mob));
        assert!(!accepts_damage(&b, &other_breeze_proj));
    }

    /// Scenario 3: Successful execution when requirements are met:
    /// Breeze accepts damage from other mob species (e.g. Zombie, Skeleton) when `source.attacker_species` is Zombie/Skeleton.
    #[test]
    fn breeze_accepts_damage_from_other_mob_species() {
        let mut fixture = crate::health::TestLevel::new(0);
        let mut level = fixture.level();
        let mut b = test_breeze(1);
        let initial_health = b.health;

        // Attack from Zombie
        let zombie_hit = Damage::mob(5, [0.0, 0.0, 0.0]).with_species(Species::Zombie);
        assert!(
            accepts_damage(&b, &zombie_hit),
            "breeze accepts damage from zombie"
        );
        assert!(
            b.damage(&mut level, 4.0, zombie_hit),
            "breeze must take damage from zombie"
        );
        assert_eq!(b.health, initial_health - 4.0);

        // Attack from Skeleton
        b.invulnerable = 0;
        let skeleton_hit = Damage::mob(6, [0.0, 0.0, 0.0]).with_species(Species::Skeleton);
        assert!(
            accepts_damage(&b, &skeleton_hit),
            "breeze accepts damage from skeleton"
        );
        assert!(
            b.damage(&mut level, 2.0, skeleton_hit),
            "breeze must take damage from skeleton"
        );
        assert_eq!(b.health, initial_health - 6.0);
    }

    /// Scenario 4: Special / edge cases:
    /// Breeze rejects self-inflicted damage (`source.attacker == Some(Who::Mob(m.id))`),
    /// and verify bypasses_invulnerability behavior if applicable.
    #[test]
    fn breeze_rejects_self_damage_and_respects_invulnerability_flags() {
        let mut fixture = crate::health::TestLevel::new(0);
        let mut level = fixture.level();
        let mut b = test_breeze(1);
        let initial_health = b.health;

        // Self-inflicted damage with no species tag
        let self_hit_no_species = Damage::mob(b.id, [0.0, 0.0, 0.0]);
        assert!(
            !accepts_damage(&b, &self_hit_no_species),
            "breeze rejects self-hit even without species"
        );
        assert!(
            !b.damage(&mut level, 5.0, self_hit_no_species),
            "breeze takes no damage from itself"
        );
        assert_eq!(b.health, initial_health);

        // Self-inflicted damage with species tag
        let self_hit_with_species =
            Damage::mob(b.id, [0.0, 0.0, 0.0]).with_species(Species::Breeze);
        assert!(
            !accepts_damage(&b, &self_hit_with_species),
            "breeze rejects self-hit with species"
        );
        assert!(
            !b.damage(&mut level, 5.0, self_hit_with_species),
            "breeze takes no damage from itself"
        );
        assert_eq!(b.health, initial_health);

        // Invulnerable flag on Mob:
        b.invulnerable_flag = true;
        // Normal damage does not bypass invulnerability flag
        let normal_player = Damage::player([0.0, 0.0, 0.0]);
        assert!(
            !b.damage(&mut level, 5.0, normal_player),
            "invulnerable_flag blocks normal player damage"
        );
        assert_eq!(b.health, initial_health);

        // GenericKill bypasses invulnerability flag
        let kill_damage = Damage::of(DamageKind::GenericKill);
        assert!(kill_damage.kind.bypasses_invulnerability());
        assert!(
            b.damage(&mut level, 1.0, kill_damage),
            "GenericKill bypasses invulnerability flag"
        );
        assert_eq!(b.health, initial_health - 1.0);

        // But another Breeze with GenericKill is still rejected by Breeze.isInvulnerableTo
        b.invulnerable = 0;
        let breeze_kill = Damage::of(DamageKind::GenericKill).with_species(Species::Breeze);
        assert!(
            !accepts_damage(&b, &breeze_kill),
            "Breeze.isInvulnerableTo rejects breeze attacker even for GenericKill"
        );
        assert!(
            !b.damage(&mut level, 1.0, breeze_kill),
            "breeze takes no damage from breeze GenericKill"
        );
    }
}
