//! The age lock (`AgeableMob.mobInteract`, `setAgeLocked`): a golden dandelion on a baby flips its lock. Locked, the baby does not grow:
//! no growth deadline is armed (`AgeLocked` keeps the ticks left, the wheel drops the deadline). Each flip puts the age back to the baby
//! start age (`getBabyStartAge`), uses the item, keeps the baby (`setPersistenceRequired` once locked), plays a sound and, for 40 ticks,
//! one particle every other tick (`ageLockParticleTimer`, here a pulse chain on the wheel, only while it lasts).
use crate::behaviours::breed::BreedData;
use crate::behaviours::breed_love::GROWN;
use crate::cow::*;
use crate::goals::*;
use bevy_ecs::prelude::*;

/// The baby does not grow up; the ticks of its age left are kept here (`GrowAt` is stale while it is on).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
#[component(storage = "SparseSet", on_insert = crate::bridge::view::touched, on_remove = crate::bridge::view::touched)]
pub struct AgeLocked(pub u64);

/// The pulses of growth particles left, one every 2 ticks (`ageLockParticleTimer` / 2).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct LockParticles(pub u8);

const PULSES: u8 = 20;

/// `AgeableMob.canUseGoldenDandelion`, then `setAgeLocked`: true when the item was used.
pub fn interact(w: &mut World, e: Entity, i: &crate::behaviours::Interaction) -> bool {
    if !crate::items::is("minecraft:golden_dandelion", i.item) || w.get::<GrowAt>(e).is_none() || w.get::<LockParticles>(e).is_some() {
        return false;
    }
    if crate::type_tags::named(w, "minecraft:cannot_be_age_locked", e) {
        return false;
    }
    let start = w.get::<BreedData>(e).map_or(GROWN, |d| d.baby_age);
    let now = now_of(w, e);
    let locked = w.get::<AgeLocked>(e).is_none();
    if locked {
        w.entity_mut(e).insert(AgeLocked(start));
        crate::despawn::keep(w, e);
    } else {
        w.entity_mut(e).remove::<AgeLocked>().insert(GrowAt(now + start));
        arm_at(w, e, now + start, Due::Grow);
    }
    w.entity_mut(e).insert(LockParticles(PULSES));
    after(w, e, 1, pulse);
    if let Some(at) = w.get::<Pos>(e).map(|p| p.0) {
        let name = if locked { "item.golden_dandelion.use" } else { "item.golden_dandelion.unuse" };
        w.resource_mut::<Events>().host.sound(name, [at.x.floor(), at.y.floor(), at.z.floor()], 1.0, 1.0);
    }
    crate::bridge::view::mark(w, e);
    crate::give_item::take_from_hand(w, i);
    true
}

/// `AgeableMob.makeAgeLockedParticle`: a particle above the baby (a pause sign when locked, a reset sign when freed).
fn pulse(w: &mut World, e: Entity) {
    let Some(left) = w.get::<LockParticles>(e).map(|l| l.0) else { return };
    let (Some(at), Some(b)) = (w.get::<Pos>(e).map(|p| p.0), w.get::<Body>(e).map(|b| (f64::from(b.width), f64::from(b.height)))) else { return };
    let locked = w.get::<AgeLocked>(e).is_some();
    let r = with_rng(w, e, |r| [r.next_f64(), r.next_f64(), r.next_f64()]);
    let p = [at.x + b.0 * (2.0 * r[0] - 1.0), at.y + b.1 * ((2.0 * r[1] - 1.0) * 0.2) + b.1 + if locked { f64::from(0.2_f32) } else { 0.0 }, at.z + b.0 * (2.0 * r[2] - 1.0)];
    w.resource_mut::<Events>().host.particles(if locked { "pause_mob_growth" } else { "reset_mob_growth" }, p, [0.0; 3], 0.0, 1);
    if left <= 1 {
        w.entity_mut(e).remove::<LockParticles>();
    } else {
        w.entity_mut(e).insert(LockParticles(left - 1));
        after(w, e, 2, pulse);
    }
}

/// `AgeableMob.forcedAge`: the ticks of growth given by feeding, saved as `ForcedAge`; absent while nothing was given.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[component(storage = "SparseSet")]
pub struct ForcedAge(pub i32);
