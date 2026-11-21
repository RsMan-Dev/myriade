//! The item modifiers of the live attribute maps of mobs, armor stands and the
//! dragon (`LivingEntity.collectEquipmentChanges`). The map and the NBT are in
//! `horizon_client::living_attributes`.
use super::Game;
use super::commands::Target;
use horizon_client::mobs::{
    Mob,
    finalize::{self, Attr, Op},
};
use horizon_items::items::Items;
use horizon_world::attributes::{
    AttributeError, Instance, Modifier, Observer, Operation,
    entity::{AttributeId, Catalog},
};
use std::convert::Infallible;
use std::sync::Arc;

/// One mob's `collectEquipmentChanges`: each slot whose stack is not the one
/// the map holds swaps the old stack's attribute modifiers for the new one's,
/// full stacks with their components compared, as the tick's pass and a write
/// both run it. The map is taken out so the seed can read the mob.
pub(in crate::game) fn collect_equipment(catalog: &Arc<Catalog>, items: &Items, m: &mut Mob) {
    horizon_client::mobs::armor::prepare(m, catalog);
    let mut live = std::mem::take(&mut m.extra.live);
    let seed = || {
        horizon_client::entity_nbt::attribute_entries(m)
            .into_iter()
            .map(|(_, entry)| entry)
            .collect()
    };
    let changed =
        live.collect_equipment_changes(catalog, &m.kind.id, &m.extra.equipment, items, seed);
    warn_failed(m.id, &m.kind.id, changed);
    m.extra.live = live;
}

/// The attribute `name` when it is one a mob keeps itself: `finalize`'s store,
/// which its gameplay reads and its save writes, for the attributes its type has.
fn own_attribute(m: &Mob, name: &str) -> Option<Attr> {
    let short = name.strip_prefix("minecraft:").unwrap_or(name);
    finalize::command_attr(short).filter(|a| finalize::default_value(m, *a).is_some())
}

/// `write` on one of the mob's own attributes. A lower max health marks the attribute dirty;
/// the mob's next tick cuts its health (`LivingEntity.refreshDirtyAttributes`, `finalize::refresh`).
fn write_own_attribute(m: &mut Mob, a: Attr, write: AttributeWrite) -> bool {
    write.apply_to_mob(m, a)
}

/// An attribute write on a mob that is in no level yet (`finalize_spawn`'s
/// lent mob), then its equipment's modifiers collected again, as
/// `Game::write_attribute` does for a joined one. Only the attributes the mob
/// keeps itself take it: the rest live in maps the level keeps per joined
/// entity. None for another attribute, `Some(false)` when an add finds the id
/// taken or a remove finds none.
pub(in crate::game) fn write_unspawned_attribute(
    catalog: &Arc<Catalog>,
    items: &Items,
    m: &mut Mob,
    name: &str,
    write: AttributeWrite,
) -> Option<bool> {
    let done = write_own_attribute(m, own_attribute(m, name)?, write);
    collect_equipment(catalog, items, m);
    Some(done)
}

impl Game<'_> {
    /// `LivingEntity.tick`'s `collectEquipmentChanges` for the armor stands and
    /// the dragon, which the ECS does not hold: each slot whose item is not the
    /// one the map holds swaps the old item's attribute modifiers for the new
    /// one's. The mobs take theirs at the event of a slot change, in P
    /// (`equip_attrs`), never by tick.
    pub(super) fn tick_equipment_attributes(&mut self) {
        if self.remote.is_some() || self.options.execution_role.is_client() {
            return;
        }
        let catalog = self.motion.attributes.clone();
        let items = world_items!(self);
        for a in self.armor_stands.iter_mut() {
            if a.equipment.is_empty() && !a.live.has_worn() {
                continue;
            }
            let changed = a.live.collect_equipment_changes(
                &catalog,
                "minecraft:armor_stand",
                &a.equipment,
                items,
                Vec::new,
            );
            warn_failed(a.id, "minecraft:armor_stand", changed);
        }
        if let Some(d) = self.dragon.as_mut()
            && !(d.equipment.is_empty() && !d.live.has_worn())
        {
            let changed = d.live.collect_equipment_changes(
                &catalog,
                "minecraft:ender_dragon",
                &d.equipment,
                items,
                Vec::new,
            );
            warn_failed(d.id, "minecraft:ender_dragon", changed);
        }
    }
}

/// An item modifier the map refused (a bad modifier id) is logged, naming the
/// entity, instead of being lost.
fn warn_failed(id: u32, kind: &str, result: horizon_client::Result<()>) {
    if let Err(error) = result {
        log::warn!("equipment attributes of {kind} {id}: {error}");
    }
}

/// One attribute write: the one path of `/attribute` (`AttributeCommand`'s
/// `base set`, `base reset`, `modifier add` and `modifier remove`) and the mod
/// API's `LivingRef` writes, through `Game::write_attribute`.
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

    /// The write on one of the attributes a mob's gameplay reads from
    /// `finalize`, which saves its bases and permanent modifiers.
    fn apply_to_mob(self, m: &mut Mob, a: Attr) -> bool {
        let taken = |m: &Mob, id: &str| {
            finalize::has_modifier(m, a, id)
                || finalize::has_transient_modifier(m, a, id)
                || (id == "minecraft:baby" && finalize::transient(m, a).is_some())
        };
        let remove = |m: &mut Mob, id: &str| {
            if id == "minecraft:baby" && finalize::transient(m, a).is_some() {
                m.extra
                    .attributes
                    .get_or_insert_with(Box::default)
                    .baby_speed_removed = true;
            }
            finalize::remove_modifier(m, a, id);
        };
        match self {
            Self::Add {
                modifier,
                permanent,
                replace,
            } => {
                let id = modifier.id();
                if taken(m, id) {
                    if !replace {
                        return false;
                    }
                    remove(m, id);
                }
                let op = match modifier.operation() {
                    Operation::AddValue => Op::AddValue,
                    Operation::AddMultipliedBase => Op::AddMultipliedBase,
                    Operation::AddMultipliedTotal => Op::AddMultipliedTotal,
                };
                if permanent {
                    finalize::add_modifier(m, a, id.to_owned(), modifier.amount(), op);
                } else {
                    finalize::add_transient_modifier(m, a, id, modifier.amount(), op);
                }
                true
            }
            Self::Remove(id) => {
                let had = taken(m, &id);
                if had {
                    remove(m, &id);
                }
                had
            }
            Self::Base(base) => {
                finalize::set_base(m, a, base);
                true
            }
            Self::ResetBase => {
                let default = finalize::default_value(m, a).unwrap_or_default();
                finalize::set_base(m, a, default);
                true
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
                let m = self.remote_entities.iter_mut().find(|m| m.id == mob && !m.removed)?;
                if let Some(a) = own_attribute(m, name) {
                    return Some(write_own_attribute(m, a, write));
                }
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
        if done {
            self.write_live_attributes(t, name, write, default);
        }
        Some(done)
    }

    /// The same write on a mob's or a stand's live map: its base and
    /// permanent modifiers go to the map's saved entries, a transient modifier
    /// to its own list, so a rebuilt map has them again.
    fn write_live_attributes(
        &mut self,
        t: Target,
        name: &str,
        write: AttributeWrite,
        default: f64,
    ) {
        let catalog = self.motion.attributes.clone();
        let (live, entity, seed) = match t {
            Target::Mob(id) => {
                let Some(m) = self.remote_entities.iter_mut().find(|m| m.id == id && !m.removed) else {
                    return;
                };
                // The seed is only read when the map is not built yet.
                let seed = if m.extra.live.built().is_none() {
                    horizon_client::entity_nbt::attribute_entries(m)
                        .into_iter()
                        .map(|(_, entry)| entry)
                        .collect()
                } else {
                    Vec::new()
                };
                (&mut m.extra.live, m.kind.id.clone(), seed)
            }
            Target::Stand(id) => {
                let Some(a) = self.armor_stands.iter_mut().find(|a| a.id == id) else {
                    return;
                };
                (&mut a.live, "minecraft:armor_stand".to_owned(), Vec::new())
            }
            _ => return,
        };
        let record = write.transient_record().map(|(id, m)| (id.to_owned(), m));
        let done = live
            .edit(
                &catalog,
                &entity,
                name,
                true,
                || seed,
                |value, observer| write.apply(value, observer, default),
            )
            .unwrap_or(false);
        if done && let Some((id, modifier)) = record {
            live.keep_transient(name, &id, modifier);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::native_entity_refs::tests::with_game;
    use crate::game::native_merchant_refs::tests::spawn_id;
    use horizon_client::{entities::ArmorStand, items::ItemStack};
    use horizon_core::geometry::Vec3;
    use horizon_items::equipment::{CHEST, HEAD};
    use mod_api::EquipmentSlot;

    fn stack(item: &str) -> Option<ItemStack> {
        Some(ItemStack::new(item, 1))
    }

    /// A stack of `item` whose own `attribute_modifiers` hold one row.
    fn with_row(item: &str, attribute: &str, id: &str, amount: f64, slot: &str) -> ItemStack {
        use horizon_core::nbt::Tag;
        let mut own = ItemStack::new(item, 1);
        own.data_mut().set_other(
            "minecraft:attribute_modifiers",
            Some(Tag::List(vec![Tag::compound([
                ("type", Tag::String(attribute.into())),
                ("id", Tag::String(id.into())),
                ("amount", Tag::Double(amount.to_bits())),
                ("operation", Tag::String("add_value".into())),
                ("slot", Tag::String(slot.into())),
            ])])),
        );
        own
    }

    /// The ids of the permanent modifiers the ECS saves for `id`.
    fn saved_modifier_ids(game: &Game<'_>, id: u32) -> Vec<String> {
        use horizon_core::nbt::{JavaString, Tag};
        let saved = game.ecs.entity_nbt(game.ecs.entity_of(id).unwrap()).unwrap();
        match saved.get(&JavaString::from("attributes")) {
            Some(Tag::List(list)) => saved_ids(list),
            _ => Vec::new(),
        }
    }

    /// A zombie on the ECS, an armor stand and the dragon (which the ECS does
    /// not hold yet), ids 12, 13 and 14.
    fn spawn(game: &mut Game<'_>) {
        spawn_id(game, 12, "minecraft:zombie");
        game.armor_stands.push(ArmorStand::new(13, Vec3::ZERO, 0.0));
        let mut dragon = crate::game::dragon::Dragon::new(Vec3::ZERO, 0.0, 0);
        dragon.id = 14;
        game.dragon.replace(dragon);
    }

    fn read(game: &Game<'_>, id: u32, name: &str) -> (Option<f64>, Option<f64>) {
        let living = game.living(game.entity(id).unwrap()).unwrap();
        (
            living.attribute_value(game, name),
            living.attribute_base(game, name),
        )
    }

    /// A held stack with its own `attribute_modifiers` hits by them, not by
    /// its item type's defaults (arismoko/horizon#941).
    #[test]
    #[ignore = "daily"]
    fn a_changed_held_stack_hits_by_its_own_modifiers() {
        with_game(|game| {
            spawn(game);
            let zombie = living(game, 12);
            let attack = |game: &Game<'_>| read(game, 12, "minecraft:attack_damage").0.unwrap();
            assert_eq!(attack(game), 3.0, "a zombie's own attack damage");
            let own = with_row("minecraft:diamond_sword", "minecraft:attack_damage", "test:dull", 1.0, "mainhand");
            assert!(
                game.living_set_equipment(zombie, EquipmentSlot::MainHand, Some(own)),
                "the write takes"
            );
            assert_eq!(attack(game), 4.0, "the stack's own +1, not the sword's +6");
            assert!(game.living_set_equipment(
                zombie,
                EquipmentSlot::MainHand,
                stack("minecraft:diamond_sword")
            ));
            assert_eq!(attack(game), 9.0, "the plain sword's own +6");
            assert!(game.living_set_equipment(zombie, EquipmentSlot::MainHand, None));
            assert_eq!(attack(game), 3.0);
        });
    }

    /// The mob's attributes are its live map's instances (arismoko/horizon#941): base
    /// and each modifier once — the equipment's own rows and `/attribute`'s
    /// writes among them. The server's sync of them (`mob_attributes`) reads a
    /// client's `Mob` and has no ECS counterpart, so the instances are read
    /// through the bridge, which is what a sync would send.
    #[test]
    #[ignore = "daily"]
    fn the_mob_attribute_packet_is_the_live_maps_instances() {
        use AttributeOperation::AddValue;
        with_game(|game| {
            spawn(game);
            let zombie = living(game, 12);
            let own = with_row("minecraft:diamond_chestplate", "minecraft:armor", "test:thin", 4.0, "chest");
            assert!(game.living_set_equipment(zombie, EquipmentSlot::Chest, Some(own)));
            // A finalize write, whose rebuild keeps the equipment's rows.
            assert!(zombie.set_attribute_base(game, "minecraft:max_health", 25.0));
            // Armor: the zombie's base and only the stack's own row.
            assert_eq!(read(game, 12, "minecraft:armor").1, Some(2.0));
            assert_eq!(
                game.ecs.attribute_modifiers(12, "minecraft:armor").unwrap(),
                [modifier("test:thin", 4.0, AddValue)]
            );
            // The written max health base, as the map holds it.
            assert_eq!(read(game, 12, "minecraft:max_health").1, Some(25.0));
            assert!(game.ecs.attribute_modifiers(12, "minecraft:max_health").unwrap().is_empty());
            // Movement speed: the instance every mob has.
            let (_, base) = read(game, 12, "minecraft:movement_speed");
            assert!(base.unwrap() > 0.0, "{base:?}");
            assert!(game.ecs.attribute_modifiers(12, "minecraft:movement_speed").unwrap().is_empty());
            // The packet the trackers send (`ecs_attributes`): every syncable instance once, with its base and its modifiers.
            let e = game.ecs.entity_of(12).unwrap();
            let packet = game.ecs_attributes(e, "minecraft:zombie");
            let mut ids: Vec<i32> = packet.iter().map(|a| a.attribute).collect();
            let sent = ids.len();
            ids.sort_unstable();
            ids.dedup();
            assert_eq!(sent, ids.len(), "each attribute once: {packet:?}");
            let of = |name: &str| {
                let id = game.synced_attribute("minecraft:zombie", name).unwrap();
                packet.iter().find(|a| a.attribute == id)
            };
            let armor = of("minecraft:armor").expect("armor is synced");
            assert_eq!((armor.base, armor.modifiers.len()), (2.0, 1), "the base and the stack's row once");
            assert_eq!((armor.modifiers[0].id.as_str(), armor.modifiers[0].amount, armor.modifiers[0].operation), ("test:thin", 4.0, 0));
            assert_eq!(of("minecraft:max_health").expect("max health is synced").base, 25.0);
            assert!(of("minecraft:movement_speed").is_some());
            // A change is told once: the entity is in the changed set, then not.
            let changed = game.ecs.drain_attribute_changes();
            assert!(changed.contains(&e), "the writes raised the entity");
            assert!(game.ecs.drain_attribute_changes().is_empty(), "read once, nothing at rest");
            assert!(zombie.set_attribute_base(game, "minecraft:armor", 7.0));
            assert_eq!(game.ecs.drain_attribute_changes(), [e]);
            let armor = game.ecs_attributes(e, "minecraft:zombie").into_iter().find(|a| a.base == 7.0);
            assert!(armor.is_some(), "the new base is what is sent");
        });
    }

    #[test]
    #[ignore = "daily"]
    fn a_base_write_preserves_covered_for_damage_and_attribute_sync() {
        use AttributeOperation::AddValue;
        use ModifierLifetime::Permanent;
        with_game(|game| {
            spawn_id(game, 12, "minecraft:shulker");
            let shulker = living(game, 12);
            let armor = |game: &Game<'_>| {
                (
                    read(game, 12, "minecraft:armor").0.unwrap(),
                    read(game, 12, "minecraft:armor_toughness").0.unwrap(),
                )
            };
            // The permanent row installed by Shulker.setRawPeekAmount(0).
            assert!(shulker.add_attribute_modifier(
                game,
                "minecraft:armor",
                modifier("minecraft:covered", 20.0, AddValue),
                Permanent
            ));
            let e = game.ecs.entity_of(12).unwrap();
            let saved = game.ecs.entity_nbt(e).unwrap();
            game.next_entity = 49;
            assert_eq!(game.ecs_load(&saved), Some(50));
            assert_eq!(
                (read(game, 50, "minecraft:armor").0, read(game, 50, "minecraft:armor_toughness").0),
                (Some(20.0), Some(0.0))
            );
            game.apply_entity_nbt(Target::Mob(12), horizon_core::nbt::Tag::Compound(saved)).unwrap();
            assert_eq!(armor(game), (20.0, 0.0));
            assert!(shulker.set_attribute_base(game, "minecraft:max_health", 40.0));
            // No handle read in between: damage must not need a read to restore the map.
            assert_eq!(armor(game), (20.0, 0.0));
            // What the attribute sync sends: the armor's base and its covered row.
            assert_eq!(read(game, 12, "minecraft:armor").1, Some(0.0));
            assert_eq!(
                game.ecs.attribute_modifiers(12, "minecraft:armor").unwrap(),
                [modifier("minecraft:covered", 20.0, AddValue)]
            );
        });
    }

    /// The armor a hit reads follows the live map at once (arismoko/horizon#941): a stack a
    /// mod or `/item` changed protects by its own `attribute_modifiers`, a
    /// `/attribute` write takes effect without a tick, a horse's body piece
    /// armors it by its stack's rows, and the save keeps a written base and
    /// modifier.
    /// `/attribute` writes on knockback resistance, max health and armor toughness reach what a hit reads, at the write.
    #[test]
    #[ignore = "daily"]
    fn knockback_resistance_and_max_health_writes_reach_the_next_hit() {
        use AttributeOperation::AddValue;
        use ModifierLifetime::Permanent;
        use horizon_world::attribute_map::ValueKind;
        with_game(|game| {
            spawn(game);
            let zombie = living(game, 12);
            assert!(zombie.set_attribute_base(game, "minecraft:knockback_resistance", 0.5));
            assert!(zombie.add_attribute_modifier(game, "minecraft:knockback_resistance", modifier("test:kb", 0.25, AddValue), Permanent));
            assert_eq!(game.ecs.attribute(12, "minecraft:knockback_resistance", ValueKind::Computed), Some(0.75));
            assert!(zombie.set_attribute_base(game, "minecraft:max_health", 30.0));
            assert_eq!(game.ecs.attribute(12, "minecraft:max_health", ValueKind::Computed), Some(30.0));
            assert!(zombie.set_attribute_base(game, "minecraft:armor_toughness", 4.0));
            assert_eq!(read(game, 12, "minecraft:armor_toughness").0, Some(4.0));
        });
    }

    #[test]
    #[ignore = "daily"]
    fn a_hits_armor_follows_the_live_map_at_once() {
        use AttributeOperation::*;
        use ModifierLifetime::*;
        use horizon_ecs_proto::bridge::host::HurtSource;
        with_game(|game| {
            spawn(game);
            let values = |game: &Game<'_>, id: u32| {
                (
                    read(game, id, "minecraft:armor").0.unwrap(),
                    read(game, id, "minecraft:armor_toughness").0.unwrap(),
                )
            };
            let zombie = living(game, 12);
            // The zombie's own armor is 2, the diamond chestplate's 8 and 2
            // toughness: the write through the handle collects at once.
            assert!(game.living_set_equipment(
                zombie,
                EquipmentSlot::Chest,
                stack("minecraft:diamond_chestplate")
            ));
            assert_eq!(values(game, 12), (10.0, 2.0));
            // A chestplate with its own `attribute_modifiers` protects by
            // them, not by the diamond one's.
            let own = with_row("minecraft:diamond_chestplate", "minecraft:armor", "test:thin", 4.0, "chest");
            assert!(game.living_set_equipment(zombie, EquipmentSlot::Chest, Some(own)));
            assert_eq!(values(game, 12), (6.0, 0.0));
            // A `/attribute` base write lands at once, on top of the stack's.
            assert!(zombie.set_attribute_base(game, "minecraft:armor", 5.0));
            assert_eq!(values(game, 12), (9.0, 0.0));
            // Unequipped, the base and a permanent modifier are all that is
            // left, and the damage boundary reads the same value.
            assert!(zombie.add_attribute_modifier(
                game,
                "minecraft:armor",
                modifier("test:plated", 3.0, AddValue),
                Permanent
            ));
            assert!(game.living_set_equipment(zombie, EquipmentSlot::Chest, None));
            assert_eq!(values(game, 12), (8.0, 0.0));
            let e = game.ecs.entity_of(12).unwrap();
            assert!(game.ecs.set_health(12, 20.0));
            // A blow of an entity: armor stops it as it stops a cactus's.
            game.ecs_with(|ecs, mw| ecs.hurt(mw, e, 4.0, HurtSource::Entity(0)));
            game.tick_ecs().unwrap();
            // CombatRules.getDamageAfterAbsorb: (8 - 4/2) of 25 absorbs.
            let expected = 4.0 * (1.0 - 6.0 / 25.0);
            let health = game.ecs.health(12).unwrap();
            assert!((f64::from(health) - (20.0 - expected)).abs() < 1e-3, "{health}");
            // The written base and modifier survive the save and the load.
            let saved = game.ecs.entity_nbt(e).unwrap();
            game.next_entity = 49;
            assert_eq!(game.ecs_load(&saved), Some(50));
            assert_eq!(values(game, 50), (8.0, 0.0));
            // A horse's body piece armors it by its stack's rows.
            // The fixture dragon owns IDs 14 through 22, including its parts.
            spawn_id(game, 40, "minecraft:horse");
            let horse = living(game, 40);
            assert!(game.living_set_equipment(
                horse,
                EquipmentSlot::Body,
                stack("minecraft:diamond_horse_armor")
            ));
            let armored = values(game, 40);
            assert!(
                armored.0 >= 1.0,
                "the body piece armors the horse: {armored:?}"
            );
        });
    }

    #[test]
    #[ignore = "daily"]
    fn a_zombie_reads_its_defaults() {
        with_game(|game| {
            spawn(game);
            assert_eq!(
                read(game, 12, "minecraft:max_health"),
                (Some(20.0), Some(20.0))
            );
            assert_eq!(
                read(game, 12, "minecraft:attack_damage"),
                (Some(3.0), Some(3.0))
            );
            // Not an attribute of an armor stand.
            assert_eq!(read(game, 13, "minecraft:attack_damage"), (None, None));
        });
    }

    /// A read of a mob that wears nothing builds nothing and keeps nothing: the
    /// ECS holds no map to build, only the writes (`AttrMods`), and a read adds
    /// none. A written base is read at once, and a reset gives the species' back.
    #[test]
    #[ignore = "daily"]
    fn a_read_builds_the_map_once() {
        use horizon_ecs_proto::attr_mod::AttrMods;
        with_game(|game| {
            spawn(game);
            let e = game.ecs.entity_of(12).unwrap();
            let written = |game: &Game<'_>| game.ecs.world().unwrap().get::<AttrMods>(e).is_some();
            assert!(!written(game));
            read(game, 12, "minecraft:max_health");
            read(game, 12, "minecraft:attack_damage");
            read(game, 12, "minecraft:max_health");
            assert!(!written(game), "a read writes nothing");
            assert!(living(game, 12).set_attribute_base(game, "minecraft:max_health", 25.0));
            assert_eq!(
                read(game, 12, "minecraft:max_health"),
                (Some(25.0), Some(25.0))
            );
            assert!(game.ecs.reset_attribute_base(12, "minecraft:max_health"));
            assert_eq!(
                read(game, 12, "minecraft:max_health"),
                (Some(20.0), Some(20.0))
            );
        });
    }

    #[test]
    #[ignore = "daily"]
    fn a_held_sword_raises_the_attack_damage_until_it_is_put_away() {
        with_game(|game| {
            spawn(game);
            let zombie = living(game, 12);
            let damage = "minecraft:attack_damage";
            assert!(game.living_set_equipment(zombie, EquipmentSlot::MainHand, stack("minecraft:diamond_sword")));
            // +6 from the sword's `base_attack_damage` modifier; the base stays.
            assert_eq!(read(game, 12, damage), (Some(9.0), Some(3.0)));
            // The same item again changes nothing (no duplicate modifier).
            assert!(game.living_set_equipment(zombie, EquipmentSlot::MainHand, stack("minecraft:diamond_sword")));
            assert_eq!(read(game, 12, damage), (Some(9.0), Some(3.0)));
            // Another sword takes the first one's modifier off before its own.
            assert!(game.living_set_equipment(zombie, EquipmentSlot::MainHand, stack("minecraft:netherite_sword")));
            assert_eq!(read(game, 12, damage), (Some(10.0), Some(3.0)));
            assert!(game.living_set_equipment(zombie, EquipmentSlot::MainHand, None));
            assert_eq!(read(game, 12, damage), (Some(3.0), Some(3.0)));
            // A sword on the head is in no slot its modifiers name.
            assert!(game.living_set_equipment(zombie, EquipmentSlot::Head, stack("minecraft:diamond_sword")));
            assert_eq!(read(game, 12, damage), (Some(3.0), Some(3.0)));
        });
    }

    #[test]
    #[ignore = "daily"]
    fn a_stand_and_the_dragon_wear_armor_modifiers() {
        with_game(|game| {
            spawn(game);
            let armor = "minecraft:armor";
            game.armor_stands[0]
                .equipment
                .set(HEAD, stack("minecraft:diamond_helmet"));
            game.dragon
                .as_mut()
                .unwrap()
                .equipment
                .set(CHEST, stack("minecraft:diamond_chestplate"));
            game.tick_equipment_attributes();
            assert_eq!(read(game, 13, armor), (Some(3.0), Some(0.0)));
            assert_eq!(read(game, 14, armor), (Some(8.0), Some(0.0)));
            game.armor_stands[0].equipment.set(HEAD, None);
            game.tick_equipment_attributes();
            assert_eq!(read(game, 13, armor), (Some(0.0), Some(0.0)));
            assert_eq!(read(game, 14, armor), (Some(8.0), Some(0.0)));
        });
    }

    #[test]
    #[ignore = "daily"]
    fn a_stale_handle_reads_none() {
        with_game(|game| {
            spawn(game);
            let living = game.living(game.entity(12).unwrap()).unwrap();
            assert_eq!(
                living.attribute_value(game, "minecraft:max_health"),
                Some(20.0)
            );
            let e = game.ecs.entity_of(12).unwrap();
            game.ecs.despawn(e);
            assert_eq!(living.attribute_value(game, "minecraft:max_health"), None);
            assert_eq!(living.attribute_base(game, "minecraft:max_health"), None);
        });
    }

    #[test]
    #[ignore = "daily"]
    fn a_saved_base_and_modifier_are_read_and_saved_again() {
        use horizon_core::nbt::Tag;
        with_game(|game| {
            spawn(game);
            let saved = Tag::List(vec![Tag::compound([
                ("id", Tag::String("minecraft:max_health".into())),
                ("base", Tag::Double(40.0f64.to_bits())),
                (
                    "modifiers",
                    Tag::List(vec![Tag::compound([
                        ("id", Tag::String("test:bonus".into())),
                        ("amount", Tag::Double(0.5f64.to_bits())),
                        ("operation", Tag::String("add_multiplied_base".into())),
                    ])]),
                ),
            ])]);
            game.armor_stands[0].live.load(Some(&saved), |_| true);
            assert_eq!(
                read(game, 13, "minecraft:max_health"),
                (Some(60.0), Some(40.0))
            );
            let mut list = Vec::new();
            game.armor_stands[0].live.save_into(&mut list);
            assert_eq!(Tag::List(list), saved);
        });
    }

    /// The live map takes a held stack's own `attribute_modifiers`, and
    /// another stack of the same item with other components is a change.
    #[test]
    #[ignore = "daily"]
    fn a_stacks_own_modifiers_replace_its_items() {
        with_game(|game| {
            spawn(game);
            let zombie = living(game, 12);
            let damage = "minecraft:attack_damage";
            let own = with_row("minecraft:diamond_sword", damage, "test:dull", 1.0, "mainhand");
            assert!(game.living_set_equipment(zombie, EquipmentSlot::MainHand, Some(own)));
            // +1 from its own list, not the sword's +6.
            assert_eq!(read(game, 12, damage), (Some(4.0), Some(3.0)));
            assert!(game.living_set_equipment(zombie, EquipmentSlot::MainHand, stack("minecraft:diamond_sword")));
            assert_eq!(read(game, 12, damage), (Some(9.0), Some(3.0)));
        });
    }

    /// A mob's gear knockback resistance is what its map reads from the worn
    /// stacks, and follows them when one changes: here a chestplate with its
    /// own list. (The old per-mob cache of the gear read has no ECS
    /// counterpart: the map is the only state.)
    #[test]
    #[ignore = "daily"]
    fn gear_knockback_follows_the_gear() {
        with_game(|game| {
            spawn(game);
            let zombie = living(game, 12);
            let resistance = |game: &Game<'_>| read(game, 12, "minecraft:knockback_resistance").0.unwrap();
            let close = |value: f64, to: f64| (value - to).abs() < 1e-6;
            assert!(game.living_set_equipment(zombie, EquipmentSlot::Chest, stack("minecraft:netherite_chestplate")));
            let value = resistance(game);
            assert!(close(value, 0.1), "{value:?}");
            assert!(game.living_set_equipment(zombie, EquipmentSlot::Chest, stack("minecraft:netherite_chestplate")));
            assert!(close(resistance(game), 0.1), "the same gear counts once");
            let own = with_row(
                "minecraft:netherite_chestplate",
                "minecraft:knockback_resistance",
                "test:anchor",
                0.5,
                "chest",
            );
            assert!(game.living_set_equipment(zombie, EquipmentSlot::Chest, Some(own)));
            let value = resistance(game);
            assert!(close(value, 0.5), "{value:?}");
            assert!(game.living_set_equipment(zombie, EquipmentSlot::Chest, None));
            assert!(close(resistance(game), 0.0));
        });
    }

    fn modifier(id: &str, amount: f64, operation: AttributeOperation) -> AttributeModifier {
        AttributeModifier::new(Identifier::parse(id).unwrap(), amount, operation)
    }

    fn living(game: &Game<'_>, id: u32) -> mod_api::LivingRef {
        game.living(game.entity(id).unwrap()).unwrap()
    }

    /// The ids of the modifiers a saved `attributes` list holds.
    fn saved_ids<'a>(list: impl IntoIterator<Item = &'a horizon_core::nbt::Tag>) -> Vec<String> {
        use horizon_client_core::nbt_value::text;
        use horizon_core::nbt::Tag;
        list.into_iter()
            .flat_map(|entry| match entry.get("modifiers") {
                Some(Tag::List(l)) => l.iter().filter_map(|m| text(m.get("id"))).collect(),
                _ => Vec::new(),
            })
            .collect()
    }

    use mod_api::{AttributeModifier, AttributeOperation, Identifier, ModifierLifetime};

    /// A mob's movement speed is kept where its gameplay reads it: a mod's
    /// permanent modifier is saved, a transient one is not, both count at once.
    #[test]
    #[ignore = "daily"]
    fn a_mods_modifiers_on_a_mobs_speed_reach_its_gameplay() {
        use AttributeOperation::*;
        use ModifierLifetime::*;
        with_game(|game| {
            spawn(game);
            let speed = "minecraft:movement_speed";
            let zombie = living(game, 12);
            let (_, Some(base)) = read(game, 12, speed) else {
                panic!("a zombie has a movement speed");
            };
            assert!(zombie.add_attribute_modifier(
                game,
                speed,
                modifier("test:fast", 1.0, AddMultipliedBase),
                Permanent
            ));
            // The id is taken (vanilla throws).
            assert!(!zombie.add_attribute_modifier(
                game,
                speed,
                modifier("test:fast", 2.0, AddValue),
                Transient
            ));
            assert!(zombie.add_attribute_modifier(
                game,
                speed,
                modifier("test:boost", 0.5, AddMultipliedTotal),
                Transient
            ));
            let value = base * 2.0 * 1.5;
            assert_eq!(read(game, 12, speed), (Some(value), Some(base)));
            let direct = |game: &Game<'_>| {
                game.ecs.attribute(12, speed, horizon_world::attribute_map::ValueKind::Computed)
            };
            assert_eq!(direct(game), Some(value));
            assert_eq!(saved_modifier_ids(game, 12), ["test:fast"]);
            // Replacing takes the new lifetime: no longer saved.
            assert!(zombie.replace_attribute_modifier(
                game,
                speed,
                modifier("test:fast", 1.0, AddMultipliedBase),
                Transient
            ));
            assert!(saved_modifier_ids(game, 12).is_empty());
            assert_eq!(read(game, 12, speed), (Some(value), Some(base)));
            let boost = Identifier::parse("test:boost").unwrap();
            assert!(zombie.remove_attribute_modifier(game, speed, &boost));
            assert!(!zombie.remove_attribute_modifier(game, speed, &boost));
            assert_eq!(read(game, 12, speed), (Some(base * 2.0), Some(base)));
            assert!(zombie.set_attribute_base(game, speed, 0.5));
            assert_eq!(read(game, 12, speed), (Some(1.0), Some(0.5)));
            assert_eq!(direct(game), Some(1.0));
        });
    }

    /// An attribute only the overlay keeps keeps a mod's writes and saves the permanent ones: a save read back
    /// into a new entity brings the base and the permanent modifier, not the transient one.
    #[test]
    #[ignore = "daily"]
    fn a_mods_writes_outlive_a_rebuilt_map() {
        use AttributeOperation::*;
        use ModifierLifetime::*;
        use horizon_core::nbt::{JavaString, Tag};
        with_game(|game| {
            crate::game::native_merchant_refs::tests::spawn_id(game, 12, "minecraft:zombie");
            let gravity = "minecraft:gravity";
            let zombie = living(game, 12);
            assert!(zombie.set_attribute_base(game, gravity, 0.1));
            assert!(zombie.add_attribute_modifier(
                game,
                gravity,
                modifier("test:heavy", 1.0, AddMultipliedBase),
                Permanent
            ));
            assert!(zombie.add_attribute_modifier(
                game,
                gravity,
                modifier("test:float", -0.05, AddValue),
                Transient
            ));
            let both = Some((0.1 - 0.05) * 2.0);
            assert_eq!(read(game, 12, gravity), (both, Some(0.1)));
            let e = game.ecs.entity_of(12).unwrap();
            let saved = game.ecs.entity_nbt(e).unwrap();
            let Some(Tag::List(list)) = saved.get(&JavaString::from("attributes")) else {
                panic!("the permanent modifier is saved");
            };
            assert_eq!(saved_ids(list), ["test:heavy"]);
            let float = Identifier::parse("test:float").unwrap();
            assert!(zombie.remove_attribute_modifier(game, gravity, &float));
            assert_eq!(read(game, 12, gravity), (Some(0.2), Some(0.1)));
            // The save read into a new entity keeps the permanent write.
            game.next_entity = 49;
            assert_eq!(game.ecs_load(&saved), Some(50));
            assert_eq!(read(game, 50, gravity), (Some(0.2), Some(0.1)));
            // A base written as NaN is a reset to the species' own.
            assert!(zombie.set_attribute_base(game, gravity, f64::NAN));
            assert_ne!(read(game, 12, gravity).1, Some(0.1));
            // A mob's armor goes where its damage reads it.
            assert!(zombie.set_attribute_base(game, "minecraft:armor", 10.0));
            assert_eq!(read(game, 12, "minecraft:armor").1, Some(10.0));
        });
    }

    #[test]
    #[ignore = "daily"]
    fn a_stand_the_dragon_and_a_player_take_writes() {
        use AttributeOperation::*;
        use ModifierLifetime::*;
        use horizon_client::motion::PlayerMotion;
        with_game(|game| {
            spawn(game);
            let health = "minecraft:max_health";
            let stand = living(game, 13);
            assert!(stand.set_attribute_base(game, health, 4.0));
            assert_eq!(read(game, 13, health), (Some(4.0), Some(4.0)));
            // Not an attribute of an armor stand; a non-finite amount.
            let damage = modifier("test:x", 1.0, AddValue);
            assert!(!stand.add_attribute_modifier(
                game,
                "minecraft:attack_damage",
                damage,
                Permanent
            ));
            assert!(!stand.add_attribute_modifier(
                game,
                health,
                modifier("test:x", f64::NAN, AddValue),
                Permanent
            ));
            // `/attribute` refuses the dragon, and nothing reads its map yet.
            let dragon = living(game, 14);
            assert!(!dragon.add_attribute_modifier(
                game,
                health,
                modifier("test:tough", 100.0, AddValue),
                Transient
            ));
            assert_eq!(read(game, 14, health), (Some(200.0), Some(200.0)));

            game.player = Some(PlayerMotion::new(game.motion, Vec3::ZERO).unwrap());
            // The dragon owns its multipart IDs above 14; keep the fixture
            // player outside that range and reserve it against later spawns.
            game.next_entity = game.next_entity.max(100);
            crate::game::native_entity_refs::tests::bind_local_player(game, 100, 123);
            let player = living(game, 100);
            assert!(player.add_attribute_modifier(
                game,
                health,
                modifier("test:hearty", 0.5, AddMultipliedBase),
                Permanent
            ));
            assert_eq!(read(game, 100, health), (Some(30.0), Some(20.0)));
            let hearty = Identifier::parse("test:hearty").unwrap();
            assert!(player.remove_attribute_modifier(game, health, &hearty));
            assert_eq!(read(game, 100, health), (Some(20.0), Some(20.0)));
            // A stale handle writes nothing.
            let zombie = living(game, 12);
            let e = game.ecs.entity_of(12).unwrap();
            game.ecs.despawn(e);
            assert!(!zombie.set_attribute_base(game, "minecraft:gravity", 1.0));
        });
    }

    /// Plates every chestplate with +0.4 knockback resistance and no armor
    /// toughness (`item_attribute_modifier`).
    struct Plated;
    #[mod_api::__subscriptions]
    impl mod_api::Hooks for Plated {
        fn item_attribute_modifier(&self, event: &mut mod_api::ItemAttributeModifierEvent<'_>) {
            if !mod_api::__private::item_key_id(event.stack.item())
                .unwrap()
                .ends_with("_chestplate")
            {
                return;
            }
            let id = |text: &str| Identifier::parse(text).unwrap();
            event.modifiers.add(
                id("minecraft:knockback_resistance"),
                modifier("test:plated", 0.4, AttributeOperation::AddValue),
                mod_api::EquipmentSlotGroup::Chest,
            );
            event
                .modifiers
                .remove_all_for(&id("minecraft:armor_toughness"));
        }
    }

    /// A mod's answer to `item_attribute_modifier` is what the zombie's
    /// gameplay reads: its knockback and its live map.
    #[test]
    #[ignore = "daily"]
    fn a_mods_item_modifiers_reach_the_wearer() {
        // The game's stack snapshots, which the hook needs, as its startup installs them;
        // without this the test passes only after another test in the process installed them.
        crate::game::api_values::install_stack_hooks();
        let mods = vec![mod_api::ModEntry::new("plated", Box::new(Plated))].into();
        mod_platform::testing::with_mods(mods, || {
            with_game(|game| {
                spawn(game);
                let zombie = living(game, 12);
                let chestplate = stack("minecraft:netherite_chestplate");
                assert!(game.living_set_equipment(zombie, EquipmentSlot::Chest, chestplate));
                // Netherite's own 0.1, and the mod's 0.4.
                let resistance = read(game, 12, "minecraft:knockback_resistance").0.unwrap();
                assert!((resistance - 0.5).abs() < 1e-6, "{resistance}");
                let toughness = read(game, 12, "minecraft:armor_toughness");
                assert_eq!(toughness, (Some(0.0), Some(0.0)));
                // Netherite's 8 armor still counts, on the zombie's own 2.
                assert_eq!(read(game, 12, "minecraft:armor"), (Some(10.0), Some(2.0)));
            });
        });
    }
}
