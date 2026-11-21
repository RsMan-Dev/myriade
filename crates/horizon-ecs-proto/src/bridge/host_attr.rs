//! The host's writes on a living entity's absorption and attribute modifiers (lot 75). The reads stay in host_read_living.rs.
use crate::absorption::{self, Absorption};
use crate::attr_mod::{AttrMods, Modifier};
use crate::bridge::host::EcsWorld;
use bevy_ecs::{entity::Entity, prelude::*};

/// The attribute name without its `minecraft:` prefix: the key of `AttrMods`.
fn key(name: &str) -> &str {
    name.strip_prefix("minecraft:").unwrap_or(name)
}

/// Runs `f` on the attribute list of `e`, created when absent and dropped when it empties; false when `e` does not exist.
fn edit(w: &mut World, e: Entity, name: &str, f: impl FnOnce(&mut AttrMods) -> bool) -> bool {
    if w.get_entity(e).is_err() {
        return false;
    }
    if w.get::<AttrMods>(e).is_none() {
        w.entity_mut(e).insert(AttrMods::default());
    }
    let Some(mut list) = w.get_mut::<AttrMods>(e) else { return false };
    let done = f(&mut *list);
    list.0.retain(|x| !(x.1.is_nan() && x.2.is_empty()));
    let empty = list.0.is_empty();
    drop(list);
    if empty {
        w.entity_mut(e).remove::<AttrMods>();
    }
    if done {
        crate::attr_mod::touched(w, e, name);
    }
    done
}

impl EcsWorld {
    /// `LivingEntity.getAbsorptionAmount`: 0 when the entity has none or does not exist.
    pub fn absorption(&self, e: Entity) -> f32 {
        self.world().get::<Absorption>(e).map_or(0.0, |a| a.0)
    }

    /// `LivingEntity.setAbsorptionAmount`: writes the absorption, 0 removes it.
    pub fn set_absorption(&mut self, e: Entity, amount: f32) {
        absorption::set(self.world_mut(), e, amount);
    }

    /// Adds a modifier to attribute `name`; false when `e` does not exist or a modifier of the same id is already there.
    pub fn add_attribute_modifier(&mut self, e: Entity, name: &str, m: Modifier) -> bool {
        let k = key(name).to_owned();
        edit(self.world_mut(), e, &k, |list| {
            let i = match list.0.iter().position(|x| x.0 == k) {
                Some(i) => i,
                None => {
                    list.0.push((k, f64::NAN, Vec::new()));
                    list.0.len() - 1
                }
            };
            let mods = &mut list.0[i].2;
            if mods.iter().any(|x| x.id == m.id) {
                return false;
            }
            mods.push(m);
            true
        })
    }

    /// Removes the modifier `id` from attribute `name`; false when `e` does not exist or has no such modifier.
    pub fn remove_attribute_modifier(&mut self, e: Entity, name: &str, id: &str) -> bool {
        let k = key(name).to_owned();
        edit(self.world_mut(), e, &k, |list| {
            let Some(i) = list.0.iter().position(|x| x.0 == k) else { return false };
            let mods = &mut list.0[i].2;
            let Some(j) = mods.iter().position(|x| x.id == id) else { return false };
            mods.remove(j);
            true
        })
    }

    /// Writes the base of attribute `name` over the species' one; false when `e` does not exist.
    pub fn set_attribute_base(&mut self, e: Entity, name: &str, base: f64) -> bool {
        let k = key(name).to_owned();
        edit(self.world_mut(), e, &k, |list| {
            match list.0.iter().position(|x| x.0 == k) {
                Some(i) => list.0[i].1 = base,
                None => list.0.push((k, base, Vec::new())),
            }
            true
        })
    }

    /// `setBaseValue` of the species' own base (`/attribute base reset`): drops the written base, keeping the modifiers; the entry
    /// leaves the component once it holds nothing. False when `e` does not exist.
    pub fn reset_attribute_base(&mut self, e: Entity, name: &str) -> bool {
        let k = key(name).to_owned();
        edit(self.world_mut(), e, &k, |list| {
            if let Some(i) = list.0.iter().position(|x| x.0 == k) {
                list.0[i].1 = f64::NAN;
            }
            true
        })
    }

    /// The entities whose attribute instances changed since the last call (`getAttributesToSync`); empty, and free, at rest.
    pub fn drain_attribute_changes(&mut self) -> Vec<Entity> {
        let mut v: Vec<Entity> =
            self.world_mut().get_resource_mut::<crate::attr_mod::AttrDirty>().map_or_else(Vec::new, |mut d| d.0.drain().collect());
        v.sort_unstable_by_key(|e: &Entity| e.to_bits());
        v
    }

    /// The attribute instances `e` holds that the clients may be told of: (name without `minecraft:`, the base written over the species'
    /// (None: the species' own), the modifiers). The host keeps the syncable ones and gives the species' base.
    pub fn attribute_instances(&self, e: Entity) -> Vec<(String, Option<f64>, Vec<Modifier>)> {
        let w = self.world();
        let mut names: Vec<String> = w.get::<AttrMods>(e).map_or_else(Vec::new, |l| l.0.iter().map(|x| x.0.clone()).collect());
        if let Some(l) = w.get::<crate::equip_attrs::WornModifiers>(e) {
            names.extend(l.0.iter().map(|r| r.1.clone()));
        }
        if w.get::<crate::equipment::Armor>(e).is_some() || w.get::<crate::attributes::ArmorModifiers>(e).is_some() {
            names.push("armor".to_owned());
        }
        if w.get::<crate::cow::Body>(e).is_some() {
            names.extend(["movement_speed".to_owned(), "max_health".to_owned()]);
        }
        names.extend(crate::effects::attribute_names(w, e).map(str::to_owned));
        names.sort_unstable();
        names.dedup();
        names
            .into_iter()
            .map(|n| {
                let base = w.get::<AttrMods>(e).and_then(|l| l.0.iter().find(|x| x.0 == n)).map(|x| x.1).filter(|b| !b.is_nan());
                let mods = self.attribute_modifiers(e, &n);
                (n, base, mods)
            })
            .collect()
    }
}
