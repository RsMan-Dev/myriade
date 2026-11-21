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
fn edit(w: &mut World, e: Entity, f: impl FnOnce(&mut AttrMods) -> bool) -> bool {
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
        edit(self.world_mut(), e, |list| {
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
        edit(self.world_mut(), e, |list| {
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
        edit(self.world_mut(), e, |list| {
            match list.0.iter().position(|x| x.0 == k) {
                Some(i) => list.0[i].1 = base,
                None => list.0.push((k, base, Vec::new())),
            }
            true
        })
    }
}
