//! Spears: `KineticWeapon` (what holding the use button does: the speed
//! conditions of a stab, `Item.Properties.spear`'s numbers) and the reach of a stab.
use horizon_core::geometry::{Aabb, Vec3 as Vec3d};
use horizon_items::kinetic_weapon::UseDuration;
pub use horizon_items::kinetic_weapon::UseTiming;

/// `KineticWeapon.Condition`.
#[derive(Clone, Copy, Debug)]
pub struct Condition {
    pub max_duration_ticks: i32,
    pub min_speed: f32,
    pub min_relative_speed: f32,
}
impl Condition {
    pub fn test(
        self,
        ticks_used: i32,
        attacker_speed: f64,
        relative_speed: f64,
        entity_factor: f64,
    ) -> bool {
        ticks_used <= self.max_duration_ticks
            && attacker_speed >= f64::from(self.min_speed) * entity_factor
            && relative_speed >= f64::from(self.min_relative_speed) * entity_factor
    }
}

/// `KineticWeapon`: a spear's charge.
#[derive(Clone, Copy, Debug)]
pub struct Kinetic {
    pub contact_cooldown_ticks: i32,
    pub delay_ticks: i32,
    pub dismount: Condition,
    pub knockback: Condition,
    pub damage: Condition,
    pub forward_movement: f32,
    pub damage_multiplier: f32,
    pub sound: &'static str,
    pub hit_sound: &'static str,
    /// The `attack_animation` duration, and the `piercing_weapon` sound of a
    /// jab.
    pub attack_ticks: i32,
    pub attack_sound: &'static str,
}

impl From<&Kinetic> for UseTiming {
    fn from(k: &Kinetic) -> Self {
        Self {
            delay_ticks: k.delay_ticks,
            dismount_conditions: Some(UseDuration {
                max_duration_ticks: k.dismount.max_duration_ticks,
            }),
            knockback_conditions: Some(UseDuration {
                max_duration_ticks: k.knockback.max_duration_ticks,
            }),
            damage_conditions: Some(UseDuration {
                max_duration_ticks: k.damage.max_duration_ticks,
            }),
        }
    }
}

/// `AttackRange` of a spear: the reach of its stabs.
pub struct Reach {
    pub min: f32,
    pub max: f32,
    pub min_creative: f32,
    pub max_creative: f32,
    pub hitbox_margin: f32,
    pub mob_factor: f32,
}
pub const REACH: Reach = Reach {
    min: 2.0,
    max: 4.5,
    min_creative: 2.0,
    max_creative: 6.5,
    hitbox_margin: 0.125,
    mob_factor: 0.5,
};

impl Kinetic {
    /// The `kinetic_weapon` of a spear item (`Item.Properties.spear`).
    pub fn of(item: &str) -> Option<Self> {
        // (attack duration, damage multiplier, delay, dismount time and
        // speed, knockback time and speed, damage time and speed): seconds
        // and blocks per second.
        let (attack, multiplier, delay, dismount, knockback, damage) = match item {
            "minecraft:wooden_spear" => (0.65, 0.7, 0.75, (5.0, 14.0), (10.0, 5.1), (15.0, 4.6)),
            "minecraft:stone_spear" => (0.75, 0.82, 0.7, (4.5, 13.0), (9.0, 5.1), (13.75, 4.6)),
            "minecraft:copper_spear" => (0.85, 0.82, 0.65, (4.0, 12.0), (8.25, 5.1), (12.5, 4.6)),
            "minecraft:iron_spear" => (0.95, 0.95, 0.6, (2.5, 11.0), (6.75, 5.1), (11.25, 4.6)),
            "minecraft:golden_spear" => (0.95, 0.7, 0.7, (3.5, 13.0), (8.5, 5.1), (13.75, 4.6)),
            "minecraft:diamond_spear" => (1.05, 1.075, 0.5, (3.0, 10.0), (6.5, 5.1), (10.0, 4.6)),
            "minecraft:netherite_spear" => (1.15, 1.2, 0.4, (2.5, 9.0), (5.5, 5.1), (8.75, 4.6)),
            _ => return None,
        };
        let wood = item == "minecraft:wooden_spear";
        let ticks = |seconds: f32| (seconds * 20.0) as i32;
        Some(Self {
            contact_cooldown_ticks: 10,
            delay_ticks: ticks(delay),
            // `Condition.ofAttackerSpeed`, and `ofRelativeSpeed` for the damage.
            dismount: Condition {
                max_duration_ticks: ticks(dismount.0),
                min_speed: dismount.1,
                min_relative_speed: 0.0,
            },
            knockback: Condition {
                max_duration_ticks: ticks(knockback.0),
                min_speed: knockback.1,
                min_relative_speed: 0.0,
            },
            damage: Condition {
                max_duration_ticks: ticks(damage.0),
                min_speed: 0.0,
                min_relative_speed: damage.1,
            },
            forward_movement: 0.38,
            damage_multiplier: multiplier,
            sound: if wood {
                "minecraft:item.spear_wood.use"
            } else {
                "minecraft:item.spear.use"
            },
            hit_sound: if wood {
                "minecraft:item.spear_wood.hit"
            } else {
                "minecraft:item.spear.hit"
            },
            attack_ticks: ticks(attack),
            attack_sound: if wood {
                "minecraft:item.spear_wood.attack"
            } else {
                "minecraft:item.spear.attack"
            },
        })
    }

    /// `KineticWeapon.computeDamageUseDuration`: the delay and the time the
    /// damage condition lasts.
    pub fn damage_use_duration(&self) -> i32 {
        self.delay_ticks + self.damage.max_duration_ticks
    }

    /// The spear's `attack_animation`: a STAB (`SwingAnimationType` 2) as long
    /// as its attack duration.
    pub fn attack_animation(&self) -> SwingAnimation {
        SwingAnimation {
            kind: 2,
            duration: self.attack_ticks,
        }
    }
}

/// `KineticWeapon.HIT_FEEDBACK_TICKS`.
pub const HIT_FEEDBACK_TICKS: i64 = 10;

/// `ProjectileUtil.getManyEntityHitResult` over `boxes`: those the segment
/// `from`..`to` meets, or comes within the margin of with no block between
/// (`clip`: where a segment first meets a block), each with where it is hit.
fn hits_along(
    boxes: &[(usize, Aabb)],
    from: Vec3d,
    to: Vec3d,
    margin: f64,
    clip: &impl Fn(Vec3d, Vec3d) -> Option<Vec3d>,
) -> Vec<(usize, Vec3d)> {
    let grow = Vec3d::new(margin, margin, margin);
    let mut hits = Vec::new();
    for &(i, bounds) in boxes {
        let hit = if bounds.contains(from) {
            Some(from)
        } else if let Some(at) = bounds.clip(from, to) {
            Some(at)
        } else if margin > 0.0 {
            bounds.inflate(grow).clip(from, to).and_then(|outside| {
                let centre = bounds.center();
                let toward = clip(outside, centre).unwrap_or(centre);
                bounds.clip(outside, toward)
            })
        } else {
            None
        };
        if let Some(at) = hit {
            hits.push((i, at));
        }
    }
    hits
}

/// `ProjectileUtil.getHitEntitiesAlong` with an `AttackRange` of `reach` (its
/// effective min and max range and hitbox margin): the boxes along `look`
/// from the minimum reach to the maximum plus the attacker's own movement,
/// none when a block (`clip`) is closer than the minimum.
pub fn hits_in_reach(
    boxes: &[(usize, Aabb)],
    eye: Vec3d,
    look: Vec3d,
    movement: Vec3d,
    (min, max, margin): (f32, f32, f32),
    clip: impl Fn(Vec3d, Vec3d) -> Option<Vec3d>,
) -> Vec<(usize, Vec3d)> {
    let from = eye.add_vector(look.scale(f64::from(min)));
    let mut to = eye.add_vector(look.scale(f64::from(max) + movement.dot(look).max(0.0)));
    // A block closer than the minimum reach takes the whole stab.
    if let Some(block) = clip(eye, to) {
        to = block;
        if eye.distance_squared(to) < eye.distance_squared(from) {
            return Vec::new();
        }
    }
    hits_along(boxes, from, to, f64::from(margin), &clip)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinetic_numeric_codecs_normalize_before_rendering() {
        use serde_json::json;
        for (value, ticks) in [
            (json!(40.0), 40),
            (json!(40.75), 40),
            (json!(-0.5), 0),
            (json!(4_294_967_336_i64), 40),
            (json!(4_294_967_336.75), 40),
        ] {
            let timing = UseTiming::from_json(&json!({
                "delay_ticks": value,
                "damage_conditions": {"max_duration_ticks": value}
            }))
            .unwrap();
            assert_eq!(timing.delay_ticks, ticks);
            assert_eq!(timing.damage_conditions.unwrap().max_duration_ticks, ticks);
        }
        for value in [json!(-1), json!("40"), json!(true), json!(2_147_483_648.0)] {
            assert!(UseTiming::from_json(&json!({"delay_ticks": value})).is_none());
        }
        assert_eq!(
            UseTiming::from_json(&json!({"delay_ticks": null, "damage_conditions": null})),
            UseTiming::from_json(&json!({}))
        );
        let mut stack = crate::items::ItemStack::new("minecraft:stick", 1);
        for (number, ticks) in [
            (horizon_core::nbt::Tag::Double(40.75f64.to_bits()), 40),
            (horizon_core::nbt::Tag::Float(f32::NAN.to_bits()), 0),
        ] {
            stack.data_mut().set_other(
                "minecraft:kinetic_weapon",
                Some(horizon_core::nbt::Tag::compound([("delay_ticks", number)])),
            );
            assert_eq!(
                UseTiming::of_stack(&stack, None).unwrap().delay_ticks,
                ticks
            );
        }
    }
}
