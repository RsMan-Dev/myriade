//! The item modifiers of the live attribute maps of mobs, armor stands and the
//! dragon (`LivingEntity.collectEquipmentChanges`). The map and the NBT are in
//! `myriade_engine::living_attributes`.
use super::Game;
use super::commands::Target;
use horizon_ecs_proto::attr_mod::{Modifier as EcsModifier, Operation as EcsOp, value as value_of};
use horizon_ecs_proto::spawn_sheet::{AttrEdit, SpawnSheet};
use horizon_world::attributes::{
    AttributeError, Instance, Modifier, Observer, Operation,
    entity::{AttributeId, Catalog},
};
use std::convert::Infallible;

/// The attribute name without its `minecraft:` prefix.
fn attr_key(name: &str) -> &str {
    name.strip_prefix("minecraft:").unwrap_or(name)
}

/// The base the sheet gives attribute `name`: the last base written, a reset dropping it; None: the species' own.
pub(in crate::game) fn sheet_base(sheet: &SpawnSheet, name: &str) -> Option<f64> {
    sheet.attributes.iter().filter(|(n, _)| attr_key(n) == attr_key(name)).fold(None, |base, (_, edit)| match edit {
        AttrEdit::Base(v) => Some(*v),
        AttrEdit::ResetBase => None,
        AttrEdit::Add(_) | AttrEdit::Remove(_) => base,
    })
}

/// The modifiers the sheet gives attribute `name`, in the order they were added.
fn sheet_modifiers(sheet: &SpawnSheet, name: &str) -> Vec<EcsModifier> {
    let mut mods = Vec::new();
    for (_, edit) in sheet.attributes.iter().filter(|(n, _)| attr_key(n) == attr_key(name)) {
        match edit {
            AttrEdit::Add(m) => mods.push(m.clone()),
            AttrEdit::Remove(id) => mods.retain(|m: &EcsModifier| m.id != *id),
            AttrEdit::Base(_) | AttrEdit::ResetBase => {}
        }
    }
    mods
}

/// The value of attribute `name` on a fresh mob of `kind` as the sheet makes it, from the species' base, the modifiers of the pieces
/// it wears (`worn`, the rows `equip_attrs` grants) and the sheet's modifiers.
pub(in crate::game) fn sheet_value(catalog: &Catalog, kind: &str, sheet: &SpawnSheet, worn: &[(String, EcsModifier)], name: &str) -> Option<f64> {
    let id: AttributeId = catalog.id(name)?;
    let base = sheet_base(sheet, name).or_else(|| catalog.default_base(kind, id))?;
    let mut mods: Vec<EcsModifier> = worn.iter().filter(|(n, _)| attr_key(n) == attr_key(name)).map(|(_, m)| m.clone()).collect();
    mods.extend(sheet_modifiers(sheet, name));
    Some(value_of(base, &mods))
}

/// An attribute write on a mob that is in no level yet (`finalize_spawn`'s lent sheet): recorded as an edit of the sheet, which the
/// spawn applies. Only the attributes the species has take it. None for another attribute, `Some(false)` when an add finds the id
/// taken or a remove finds none.
pub(in crate::game) fn write_unspawned_attribute(catalog: &Catalog, kind: &str, sheet: &mut SpawnSheet, name: &str, write: AttributeWrite) -> Option<bool> {
    let id = catalog.id(name)?;
    catalog.default_base(kind, id)?;
    let mods = sheet_modifiers(sheet, name);
    let done = match write {
        AttributeWrite::Base(v) => {
            sheet.attribute(name, AttrEdit::Base(v));
            true
        }
        AttributeWrite::ResetBase => {
            sheet.attribute(name, AttrEdit::ResetBase);
            true
        }
        AttributeWrite::Remove(id) => {
            let had = mods.iter().any(|m| m.id == id);
            if had {
                sheet.attribute(name, AttrEdit::Remove(id));
            }
            had
        }
        AttributeWrite::Add { modifier, permanent, replace } => {
            let taken = mods.iter().any(|m| m.id == modifier.id());
            if taken && !replace {
                return Some(false);
            }
            if taken {
                sheet.attribute(name, AttrEdit::Remove(modifier.id().to_owned()));
            }
            let operation = match modifier.operation() {
                Operation::AddValue => EcsOp::AddValue,
                Operation::AddMultipliedBase => EcsOp::AddMultipliedBase,
                Operation::AddMultipliedTotal => EcsOp::AddMultipliedTotal,
            };
            let m = EcsModifier { id: modifier.id().to_owned(), amount: modifier.amount(), operation, permanent };
            sheet.attribute(name, AttrEdit::Add(m));
            true
        }
    };
    Some(done)
}

impl Game<'_> {
    /// `LivingEntity.tick`'s `collectEquipmentChanges` for the armor stands and
    /// the dragon, which the ECS does not hold: each slot whose item is not the
    /// one the map holds swaps the old item's attribute modifiers for the new
    /// one's. The mobs take theirs at the event of a slot change, in P
    /// (`equip_attrs`), never by tick.
    pub(super) fn tick_equipment_attributes(&mut self) {}
}

/// One attribute write: the one path of `/attribute` (`AttributeCommand`'s
/// `base set`, `base reset`, `modifier add` and `modifier remove`), through
/// `Game::write_attribute`.
#[derive(Clone)]
pub(in crate::game) enum AttributeWrite {
    /// `addTransientModifier` or `addPermanentModifier`, refused when the id
    /// is taken; with `replace`, the modifier with that id goes first
    /// (`addOrUpdateTransientModifier`, `addOrReplacePermanentModifier`).
    Add {
        modifier: Modifier,
        permanent: bool,
        replace: bool,
    },
    /// `removeModifier`.
    Remove(String),
    /// `setBaseValue`.
    Base(f64),
    /// `setBaseValue` of the entity type's default.
    ResetBase,
}

impl AttributeWrite {
    /// An add of the modifier `id`, or None for an id `Modifier` refuses.
    pub(in crate::game) fn add(
        id: &str,
        amount: f64,
        operation: Operation,
        permanent: bool,
        replace: bool,
    ) -> Option<Self> {
        Some(Self::Add {
            modifier: Modifier::new(id, amount, operation).ok()?,
            permanent,
            replace,
        })
    }

    /// The write on an instance, whose type's default base is `default`:
    /// false when an add finds the id taken or a remove finds nothing.
    fn apply<O: Observer<Error = Infallible>>(
        self,
        value: &mut Instance,
        observer: &mut O,
        default: f64,
    ) -> Result<bool, AttributeError<Infallible>> {
        match self {
            Self::Add {
                modifier,
                permanent,
                replace,
            } => {
                if value.modifier(modifier.id()).is_some() {
                    if !replace {
                        return Ok(false);
                    }
                    value.remove(modifier.id(), observer)?;
                }
                if permanent {
                    value.add_permanent(modifier, observer)?;
                } else {
                    value.add_transient(modifier, observer)?;
                }
                Ok(true)
            }
            Self::Remove(id) => value.remove(&id, observer),
            Self::Base(base) => {
                value.set_base_value(base, observer)?;
                Ok(true)
            }
            Self::ResetBase => {
                value.set_base_value(default, observer)?;
                Ok(true)
            }
        }
    }

    /// What a live map keeps of the transient modifiers written to it after
    /// this write: the added one, or None to forget the id.
    fn transient_record(&self) -> Option<(&str, Option<Modifier>)> {
        match self {
            Self::Add {
                modifier,
                permanent,
                ..
            } => Some((modifier.id(), (!permanent).then(|| modifier.clone()))),
            Self::Remove(id) => Some((id, None)),
            Self::Base(_) | Self::ResetBase => None,
        }
    }
}

impl Game<'_> {
    /// Applies `write` to the attribute `name` of `t` on the record that the
    /// entity's gameplay and its clients' attribute sync read:
    /// - a player's (or a guest's, in its turn) own attribute map;
    /// - for a mob, `finalize`'s store for the nine attributes its behavior
    ///   reads there (so it moves and hits by them at once); for its armor and
    ///   toughness its armor map (`mobs/armor.rs`); for the rest, as for an
    ///   armor stand, its generic map (`cmds.attributes`), which the sync sends.
    ///
    /// A mob's and a stand's live map (`living_attributes`, which the mod
    /// API's reads and the save use) takes the same write and keeps it
    /// through a rebuild. A lower max health cuts a mob's health
    /// (`LivingEntity.onAttributeUpdated`).
    ///
    /// None when the entity has no such attribute or takes no attribute writes
    /// (the dragon, as `/attribute` refuses it); `Some(false)` when an add
    /// finds the id taken or a remove finds none.
    pub(in crate::game) fn write_attribute(
        &mut self,
        t: Target,
        name: &str,
        write: AttributeWrite,
    ) -> Option<bool> {
        let done = self.apply_attribute_write(t, name, write)?;
        Some(done)
    }

    fn apply_attribute_write(
        &mut self,
        t: Target,
        name: &str,
        write: AttributeWrite,
    ) -> Option<bool> {
        let catalog = self.motion.attributes.clone();
        let id = catalog.id(name)?;
        let default = match write {
            AttributeWrite::ResetBase => catalog.default_base(&self.target_type(t), id)?,
            _ => 0.0,
        };
        match t {
            Target::Player | Target::Guest(_) => self
                .for_player(t, |g| {
                    let live = &mut g.player.as_mut()?.attributes;
                    let handle = live.get_instance(Some(live.key(name)?)).ok()??;
                    live.edit(&handle, |value, observer| {
                        write.apply(value, observer, default)
                    })
                    .ok()
                })
                .flatten(),
            Target::Mob(mob) => {
                // The attributes live in the ECS entity: the writes go through its bridge.
                if self.ecs.is_active() && self.ecs.attribute(mob, name, horizon_world::attribute_map::ValueKind::Base).is_some() {
                    return Some(match write {
                        AttributeWrite::Add { modifier, permanent, replace } => {
                            use horizon_ecs_proto::attr_mod::{Modifier as EcsModifier, Operation as EcsOp};
                            if replace {
                                self.ecs.remove_attribute_modifier(mob, name, modifier.id());
                            }
                            let operation = match modifier.operation() {
                                Operation::AddValue => EcsOp::AddValue,
                                Operation::AddMultipliedBase => EcsOp::AddMultipliedBase,
                                Operation::AddMultipliedTotal => EcsOp::AddMultipliedTotal,
                            };
                            let m = EcsModifier { id: modifier.id().to_owned(), amount: modifier.amount(), operation, permanent };
                            self.ecs.add_attribute_modifier(mob, name, m)
                        }
                        AttributeWrite::Remove(modifier) => self.ecs.remove_attribute_modifier(mob, name, &modifier),
                        AttributeWrite::Base(v) => self.ecs.set_attribute_base(mob, name, v),
                        AttributeWrite::ResetBase => self.ecs.reset_attribute_base(mob, name),
                    });
                }
                // RESTE-SANS-ECS: the mob's own attribute maps have no ECS write yet.
                self.write_generic_attribute(t, name, id, write, default)
            }
            Target::Stand(_) => self.write_generic_attribute(t, name, id, write, default),
            _ => None,
        }
    }

    fn write_generic_attribute(
        &mut self,
        t: Target,
        name: &str,
        id: AttributeId,
        write: AttributeWrite,
        default: f64,
    ) -> Option<bool> {
        let map = self.generic_attributes(t, name)?;
        if !map.has(id) {
            return None;
        }
        let done = map
            .edit(id, |value, observer| {
                write.clone().apply(value, observer, default)
            })?
            .ok()?;
        // RESTE-SANS-ECS: a mob's live attribute map (`extra.live`) has no ECS write yet.
        Some(done)
    }
}
