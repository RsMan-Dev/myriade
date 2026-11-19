//! The rules of a species that are not goals (`Chicken.aiStep`'s egg timer, `Sheep`'s wool, the wings, the sun that burns the
//! undead): ONE form, a `Rule` with an `equip` that gives the entity its state (its systems read that state, filtered by it),
//! and a `RuleEntry { rule, params }` in the `rules` section of a species configuration. A species that has none of them has
//! no cost for them; nothing here knows a species.
use bevy_ecs::prelude::*;
use std::any::Any;

pub struct Rule {
    pub name: &'static str,
    /// Once at creation: the state components of the rule from its parameters (a rule reads the entity's own components: body
    /// attributes, the random stream; never a species).
    pub equip: fn(&mut World, Entity, &(dyn Any + Sync)),
    /// The body attributes the rule gives (read when the bundles are made, so that a bundle made without `equip` has them).
    pub attrs: fn(&(dyn Any + Sync)) -> Attrs,
    /// A player used an item on the entity (true: handled).
    pub interact: fn(&mut World, Entity, &(dyn Any + Sync), &crate::behaviours::Interaction) -> bool,
}
pub fn no_interact(_: &mut World, _: Entity, _: &(dyn Any + Sync), _: &crate::behaviours::Interaction) -> bool {
    false
}
/// What a rule changes of the attributes of the body and the immunities.
#[derive(Clone, Copy, Debug, Default)]
pub struct Attrs {
    pub fall_factor: Option<f64>,
    pub immune: u32,
}
pub fn no_attrs(_: &(dyn Any + Sync)) -> Attrs {
    Attrs::default()
}
/// The attributes of all the rules of a species, folded.
pub fn attrs_of(rules: &[RuleEntry]) -> Attrs {
    rules.iter().fold(Attrs::default(), |a, r| {
        let b = (r.rule.attrs)(r.params);
        Attrs { fall_factor: b.fall_factor.or(a.fall_factor), immune: a.immune | b.immune }
    })
}
#[derive(Clone, Copy)]
pub struct RuleEntry {
    pub rule: &'static Rule,
    pub params: &'static (dyn Any + Sync),
}
impl RuleEntry {
    pub const fn new(rule: &'static Rule, params: &'static (dyn Any + Sync)) -> Self {
        Self { rule, params }
    }
}
pub fn params<T: 'static>(p: &(dyn Any + Sync)) -> &T {
    let p: &dyn Any = p;
    p.downcast_ref::<T>().expect("the parameters of another rule")
}
/// The rules of the base, in the order of their registration (a mod adds its own to `species::Builder`).
pub static BUILTIN: &[&Rule] = &[&crate::farm::WOOL, &crate::farm::LAY_EGG, &crate::farm::WING_FLAP, &crate::hostile::SUN_BURN, &crate::tame::TAME, &crate::loot::XP, &crate::mount::RIDE, &crate::leash::LEASH, &crate::size::SIZE, &crate::effects::ON_HIT, &crate::behaviours::breed::LOVE];
/// Gives an entity the state of every rule of its configuration.
pub fn equip_all(world: &mut World, e: Entity, rules: &[RuleEntry]) {
    for r in rules {
        (r.rule.equip)(world, e, r.params);
    }
}
