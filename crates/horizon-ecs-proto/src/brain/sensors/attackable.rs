//! `BreezeAttackEntitySensor`: the nearest of `mobs` the entity may attack (`Sensor.isEntityAttackable`: `TargetingConditions.forCombat()`
//! within the follow range, in sight), a player only when not creative or spectator, a creature only when `Params.creature` allows its
//! species (the breeze: iron golems). Written to `nearest_attackable`, erased when none.
use crate::behaviours::Cap;
use crate::brain::*;
use crate::cow::*;
use crate::goals::d2;
use crate::targeting::{self, Eye, FollowRange, Who};

pub static NEAREST_ATTACKABLE: MemKey = MemKey::new("minecraft:nearest_attackable", Shape::Who);

pub static SENSOR: Sensor = Sensor { name: "minecraft:breeze_attack_entity", mems: &[&NEAREST_ATTACKABLE], scan, requires: &[Cap::Targets], ..Sensor::DEFAULT };

/// The species a creature must be to be attacked (a name, resolved with the registry; `None`: any creature).
pub struct Params {
    pub creature: Option<&'static str>,
}
impl Params {
    pub const DEFAULT: Params = Params { creature: None };
}
fn params(c: &Ctx) -> &'static Params {
    let p: &'static dyn std::any::Any = c.params;
    p.downcast_ref::<Params>().unwrap_or(&Params::DEFAULT)
}

/// `Sensor.isEntityAttackable`: in the follow range, in sight, and (`LivingEntity.canAttack` with the species rule) a creature of the
/// allowed species or a player who is not creative or spectator.
pub fn attackable(w: &World, e: Entity, who: Who, creature: Option<&str>) -> bool {
    let Some(i) = targeting::info(w, who) else { return false };
    if i.exempt {
        return false;
    }
    if let (Who::Entity(_), Some(name)) = (who, creature)
        && crate::species::registry().id(name) != Some(usize::from(i.species))
    {
        return false;
    }
    let (Some(pos), Some(eye), Some(range)) = (w.get::<Pos>(e).map(|p| p.0), w.get::<Eye>(e), w.get::<FollowRange>(e)) else { return false };
    if d2(pos, i.pos) > range.0 * range.0 {
        return false;
    }
    let mw = w.resource::<Level>().mw.0;
    mw.line_of_sight_for(Some(pos.y), horizon_core::geometry::Vec3::new(pos.x, pos.y + eye.0, pos.z), horizon_core::geometry::Vec3::new(i.pos.x, i.eye, i.pos.z))
}

fn scan(w: &mut World, e: Entity, c: &Ctx) {
    let creature = params(c).creature;
    // The `Mob` runs `sense_living` first in the same scan: the list is read afresh, not from `mobs`.
    let list = super::living::living_near(w, e);
    match list.into_iter().find(|&who| attackable(w, e, who, creature)) {
        Some(who) => set(w, e, &NEAREST_ATTACKABLE, MemValue::Who(who), None),
        None => erase(w, e, &NEAREST_ATTACKABLE),
    }
}
