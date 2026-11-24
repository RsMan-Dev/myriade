//! The modifiers of a living entity's attributes (`AttributeInstance`, lot 75). A sparse component holding only what differs from the
//! species' configuration: a base written over it, and the modifiers. The value is computed on read, never kept.
use bevy_ecs::prelude::*;

/// How a modifier applies (`AttributeModifier.Operation`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    AddValue,
    AddMultipliedBase,
    AddMultipliedTotal,
}

/// One modifier: its id, its amount and how it applies. A permanent one is saved with the entity (`addPermanentModifier`), a
/// transient one is not (`addOrUpdateTransientModifier`).
#[derive(Clone, Debug, PartialEq)]
pub struct Modifier {
    pub id: String,
    pub amount: f64,
    pub operation: Operation,
    pub permanent: bool,
}

/// The attributes this entity changes: (attribute name without `minecraft:`, base written over the species', modifiers). A base that
/// was never written is NaN: the species' base applies.
#[derive(Component, Clone, Debug, Default, PartialEq)]
#[component(storage = "SparseSet")]
pub struct AttrMods(pub Vec<(String, f64, Vec<Modifier>)>);

/// `RangedAttribute.sanitizeValue`, applied to the calculated value: a NaN is the minimum of the range, anything else is clamped to it.
pub fn sanitize(v: f64, min: f64, max: f64) -> f64 {
    if v.is_nan() { min } else { v.clamp(min, max) }
}

/// The value of an attribute from its base and its modifiers, in vanilla's order (`AttributeInstance.calculateValue`): the added
/// values join the base; the base-multiplied amounts apply to that sum; the total multipliers apply one after another. The clamp to
/// the attribute's range is not done here.
pub fn value(base: f64, mods: &[Modifier]) -> f64 {
    let of = |op: Operation| mods.iter().filter(move |m| m.operation == op).map(|m| m.amount);
    let v1 = of(Operation::AddValue).fold(base, |acc, a| acc + a);
    let v2 = of(Operation::AddMultipliedBase).fold(v1, |acc, a| acc + v1 * a);
    of(Operation::AddMultipliedTotal).fold(v2, |acc, a| acc * (1.0 + a))
}

/// The value of attribute `name` of `e` now: its written base (else `species`), its written modifiers and the effects' ones. Read at the
/// event that uses it (a hit), never kept; the range is the caller's.
pub fn read(w: &World, e: Entity, name: &str, species: f64) -> f64 {
    let (base, mut mods) = match w.get::<AttrMods>(e).and_then(|m| m.0.iter().find(|x| x.0 == name)) {
        Some(x) => (Some(x.1).filter(|b| !b.is_nan()), x.2.clone()),
        None => (None, Vec::new()),
    };
    mods.extend(crate::effects::attribute_modifiers(w, e, name));
    value(base.unwrap_or(species), &mods)
}

/// The entities whose attribute instances changed since the host last read them (`AttributeMap.getAttributesToSync`): raised by the
/// writes, emptied by `EcsWorld::drain_attribute_changes`. Empty at rest.
#[derive(Resource, Default, Debug)]
pub struct AttrDirty(pub std::collections::HashSet<Entity>);

/// The attribute `name` (without `minecraft:`) of `e` changed (an `AttrMods` write or a worn modifier): the components the rules read at a
/// hit (`Armor`, `Resist`, `MaxHealth`) are brought up to date here, once, and the entity is marked for the sync to the clients.
pub fn touched(w: &mut World, e: Entity, name: &str) {
    match name {
        "armor" | "armor_toughness" => crate::equipment::refresh_armor(w, e),
        "knockback_resistance" => {
            let (base, mods) = match w.get::<AttrMods>(e).and_then(|m| m.0.iter().find(|x| x.0 == name)) {
                Some(x) => (Some(x.1).filter(|b| !b.is_nan()), x.2.clone()),
                None => (None, Vec::new()),
            };
            let species = w
                .get::<crate::cow::SpeciesId>(e)
                .and_then(|s| w.get_resource::<crate::cow::SpeciesTable>().map(|t| t.0[s.0 as usize].knockback_resistance));
            if let Some(species) = species {
                let all: Vec<Modifier> = mods.into_iter().chain(crate::equip_attrs::of_attribute(w, e, name)).collect();
                let v = sanitize(value(base.unwrap_or(species), &all), -2.0, 1.0);
                match w.get_mut::<crate::behaviours::stare_freeze::Braced>(e) {
                    Some(mut b) => b.0 = v,
                    None => {
                        w.entity_mut(e).insert(crate::cow::Resist(v));
                    }
                }
            }
        }
        "attack_damage" => {
            let (base, mods) = match w.get::<AttrMods>(e).and_then(|m| m.0.iter().find(|x| x.0 == name)) {
                Some(x) => (Some(x.1).filter(|b| !b.is_nan()), x.2.clone()),
                None => (None, Vec::new()),
            };
            let species = w
                .get::<crate::cow::SpeciesId>(e)
                .and_then(|s| w.get_resource::<crate::cow::SpeciesTable>().map(|t| t.0[s.0 as usize].attack_damage));
            if let Some(species) = species {
                // The variant's added attack (`Rabbit.setVariant`) stays on top of the species' base.
                let add = match (w.get::<crate::behaviours::variant::Variant>(e), crate::behaviours::variant::table_of(w, e)) {
                    (Some(v), Some(t)) => t.traits.iter().find(|x| x.variant == v.0).map_or(0.0, |x| f64::from(x.damage_add)),
                    _ => 0.0,
                };
                let all: Vec<Modifier> = mods.into_iter().chain(crate::equip_attrs::of_attribute(w, e, name)).collect();
                let v = value(base.unwrap_or(f64::from(species) + add), &all) as f32;
                w.entity_mut(e).insert(crate::cow::AttackDamage(v));
            }
        }
        "max_health" => {
            if w.get::<crate::cow::SpeciesId>(e).is_some() {
                let max = crate::cow::max_health(w, e);
                w.entity_mut(e).insert(crate::cow::MaxHealth(max));
                if let Some(mut h) = w.get_mut::<crate::cow::Health>(e)
                    && h.0 > max
                {
                    h.0 = max;
                }
                crate::bridge::view::mark(w, e);
            }
        }
        "scale" => crate::cow::refresh_box(w, e),
        "step_height" | "movement_speed" | "follow_range" | "jump_strength" | "gravity" | "flying_speed" | "tempt_range" | "water_movement_efficiency" | "safe_fall_distance" | "fall_damage_multiplier" | "movement_efficiency" | "oxygen_bonus" => {
            crate::cow::refresh_attr(w, e, name)
        }
        _ => {}
    }
    w.get_resource_or_insert_with(AttrDirty::default).0.insert(e);
}

/// When a load instance is made: always, on a baby, or on a mob with no owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadWhen {
    Always,
    Baby,
    Unowned,
    /// A transient `minecraft:baby` modifier of `base` amount, ADD_MULTIPLIED_BASE, on a baby (`Zombie.SPEED_MODIFIER_BABY`).
    BabyBonus,
}

/// The id of the baby's transient modifier (`Zombie.BABY_SPEED_MODIFIER_ID`, `Piglin`'s the same).
pub const BABY_BONUS: &str = "minecraft:baby";

/// An instance a species gets when its entity is loaded (the `setBaby`, `setAge` and `finalizeSpawn` calls of the load). Its base is
/// the species' own value of the attribute when `base` is `None`. Written only where the saved state has no base.
#[derive(Clone, Copy, Debug)]
pub struct LoadAttr {
    pub name: &'static str,
    pub base: Option<f64>,
    pub when: LoadWhen,
}

/// Makes the load instances of `e`'s species, after its saved state is read: a base already saved is kept.
pub fn apply_load(w: &mut World, e: Entity) {
    let Some(id) = w.get::<crate::cow::SpeciesId>(e).map(|s| s.0 as usize) else { return };
    let Some(table) = w.get_resource::<crate::cow::SpeciesTable>() else { return };
    let species = &table.0[id];
    let list = species.load_attrs;
    if list.is_empty() {
        return;
    }
    let defaults = [
        ("movement_speed", species.speed),
        ("max_health", f64::from(species.max_health)),
        ("attack_damage", f64::from(species.attack_damage)),
    ];
    let baby = w.get::<crate::cow::GrowAt>(e).is_some();
    let owned = w.get::<crate::tame::Owner>(e).is_some_and(|o| o.0.is_some());
    let mut made = Vec::new();
    let mut bonus = Vec::new();
    for l in list {
        if l.when == LoadWhen::BabyBonus {
            if baby {
                bonus.push((l.name, l.base.unwrap_or(0.0)));
            }
            continue;
        }
        let wanted = match l.when {
            LoadWhen::Always => true,
            LoadWhen::Baby => baby,
            LoadWhen::Unowned => !owned,
            LoadWhen::BabyBonus => false,
        };
        // `setTame(false, true)` of an unowned load replaces the saved base (`Wolf.applyTamingSideEffects`): only the others keep it.
        let kept = l.when != LoadWhen::Unowned && w.get::<AttrMods>(e).and_then(|m| m.0.iter().find(|x| x.0 == l.name)).is_some_and(|x| !x.1.is_nan());
        if !wanted || kept {
            continue;
        }
        let base = l.base.or_else(|| defaults.iter().find(|x| x.0 == l.name).map(|x| x.1)).unwrap_or(f64::NAN);
        made.push((l.name, base));
    }
    for (name, base) in made {
        let mut list = w.get::<AttrMods>(e).cloned().unwrap_or_default();
        match list.0.iter_mut().find(|x| x.0 == name) {
            Some(x) => x.1 = base,
            None => list.0.push((name.to_owned(), base, Vec::new())),
        }
        w.entity_mut(e).insert(list);
        touched(w, e, name);
    }
    for (name, amount) in bonus {
        let mut list = w.get::<AttrMods>(e).cloned().unwrap_or_default();
        let m = Modifier { id: BABY_BONUS.to_owned(), amount, operation: Operation::AddMultipliedBase, permanent: false };
        match list.0.iter_mut().find(|x| x.0 == name) {
            Some(x) => {
                if !x.2.iter().any(|m| m.id == BABY_BONUS) {
                    x.2.push(m);
                }
            }
            None => list.0.push((name.to_owned(), f64::NAN, vec![m])),
        }
        w.entity_mut(e).insert(list);
        touched(w, e, name);
    }
}

/// `setBaby(false)` on growing up (`Goat.ageBoundaryReached`, `Hoglin.setBaby`): the instances a baby had go, and the species' own values
/// stand again.
pub fn grow_up(w: &mut World, e: Entity) {
    let Some(id) = w.get::<crate::cow::SpeciesId>(e).map(|s| s.0 as usize) else { return };
    let Some(table) = w.get_resource::<crate::cow::SpeciesTable>() else { return };
    let names: Vec<&'static str> = table.0[id].load_attrs.iter().filter(|l| l.when == LoadWhen::Baby).map(|l| l.name).collect();
    let bonus: Vec<&'static str> = table.0[id].load_attrs.iter().filter(|l| l.when == LoadWhen::BabyBonus).map(|l| l.name).collect();
    let Some(mut list) = w.get::<AttrMods>(e).cloned() else { return };
    let before = list.clone();
    list.0.retain(|x| !names.contains(&x.0.as_str()));
    for x in list.0.iter_mut().filter(|x| bonus.contains(&x.0.as_str())) {
        x.2.retain(|m| m.id != BABY_BONUS);
    }
    if list == before {
        return;
    }
    w.entity_mut(e).insert(list);
    for name in names.into_iter().chain(bonus) {
        touched(w, e, name);
    }
}
