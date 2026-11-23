//! `AbstractHorse.isPushable` (`!isVehicle()`): a horse that carries a rider is not in `Level.getPushableEntities`, so it does not fill
//! the crowd of `LivingEntity.pushEntities`'s cramming. The equines pose `Unpushable`; cramming skips it while it is `Ridden`.
use bevy_ecs::prelude::*;

/// `isPushable` is false for the mount that has a rider (`Ridden`); the other entities keep the default.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Unpushable;

/// The rule that poses `Unpushable` (no params).
pub static UNPUSHABLE: crate::rules::Rule = crate::rules::Rule {
    name: "unpushable",
    equip: |w, e, _| {
        w.entity_mut(e).insert(Unpushable);
    },
    attrs: crate::rules::no_attrs,
    interact: crate::rules::no_interact,
};
