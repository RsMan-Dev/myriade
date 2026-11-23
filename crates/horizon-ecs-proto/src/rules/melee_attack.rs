//! `MeleeAttack` (`brain/behaviours/melee_attack.rs`): the melee reach and the blow's damage.
use horizon_core::geometry::{Aabb, Axis, Vec3};
use horizon_client_lib::items::ItemStack;
use horizon_items::properties::resolvable::Float;
use horizon_core::nbt::Tag;
use bevy_ecs::prelude::Resource;

/// The prototype attack range of an item by its id (the host's item defaults, as the component's tag); the host sets it once.
pub type PrototypeRangeFn = Box<dyn Fn(&str) -> Option<Tag> + Send + Sync>;
#[derive(Resource)]
pub struct PrototypeRange(pub PrototypeRangeFn);

/// `Mob.isWithinMeleeAttackRange`'s inflation: `sqrt(2.04) - 0.6`, on x and z.
pub fn reach() -> f64 {
    2.04_f32.sqrt() as f64 - 0.6
}

/// `Mob.isWithinMeleeAttackRange`: the target is in the strike box of the max reach and not in the one of the min reach. The box is the
/// attacker's, joined to its vehicle's when it rides. The range is the held item's `ATTACK_RANGE` (its patch, else its prototype's; none
/// when removed), else [`reach`] and min 0; the mob factor scales both. `hitboxMargin` is not in this check (`AttackRange.isInRange` only).
pub fn within_strike(own: Aabb, vehicle: Option<Aabb>, target: Aabb, held: Option<&ItemStack>, prototype: Option<&Tag>) -> bool {
    let patch = held.and_then(|s| s.data());
    let range = if patch.is_some_and(|d| d.other("!minecraft:attack_range").is_some()) {
        None
    } else {
        patch.and_then(|d| d.other("minecraft:attack_range")).or(prototype).and_then(horizon_items::properties::attack_range::decode)
    };
    let (max, min) = match range {
        Some([min, max, _, _, _, factor]) => (num(max) * num(factor), num(min) * num(factor)),
        None => (reach(), 0.0),
    };
    let strike = |m: f64| strike_box(own, vehicle, m).intersects(target);
    strike(max) && (min <= 0.0 || !strike(min))
}

fn num(f: Float<'_>) -> f64 {
    match f {
        Float::Constant(v) => f64::from(v),
        Float::Reference(_) => 0.0,
    }
}

/// `LivingEntity.getAttackBoundingBox(margin)`: the box of the attacker (joined to its vehicle's on x and z), inflated by `margin` on x and z.
fn strike_box(own: Aabb, vehicle: Option<Aabb>, m: f64) -> Aabb {
    let b = match vehicle {
        Some(v) => Aabb::new(
            Vec3::new(own.min(Axis::X).min(v.min(Axis::X)), own.min(Axis::Y), own.min(Axis::Z).min(v.min(Axis::Z))),
            Vec3::new(own.max(Axis::X).max(v.max(Axis::X)), own.max(Axis::Y), own.max(Axis::Z).max(v.max(Axis::Z))),
        ),
        None => own,
    };
    b.inflate(Vec3::new(m, 0.0, m))
}
