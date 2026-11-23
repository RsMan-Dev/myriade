//! A spawn the game has decided but not yet made, lent to one hook call
//! (`finalize_spawn`'s `mob`, docs/modding/src/how-to/world/spawning.md): the lend,
//! the handle's reads (`UnspawnedMobRef`), the scoped editor handed out with it
//! (`UnspawnedMobEdits`), and the spawn group data at the API boundary
//! (`SpawnGroup`). What is lent is its `SpawnSheet`: the edits and the group,
//! applied by the ECS when the entity is made.
//!
//! A lend moves the sheet into `Game::lent_mobs` for the call and moves it back
//! after, so it works where the caller holds the level shared (natural
//! spawning's and the spawners' passes): reads and edits reach that copy only.
//! A handle from a finished call, or another lend's, finds no copy and is
//! refused with `Expired`.

use super::Game;
use super::api_values::TryIntoEngine;
use super::living_attributes::{AttributeWrite, sheet_base, sheet_value, write_unspawned_attribute};
use bevy_ecs::prelude::World;
use horizon_client::text::Text;
use horizon_ecs_proto::attr_mod::Modifier as EcsModifier;
use horizon_ecs_proto::equip_attrs::ItemModifiers;
use horizon_ecs_proto::equip_stack::SLOTS;
use horizon_ecs_proto::spawn_finalize::{Group, Shape};
use horizon_ecs_proto::spawn_sheet::{AttrEdit, SpawnSheet};
use horizon_world::attributes::Operation;
use mod_api::{
    AgeableGroup, AttributeModifier, AttributeOperation, EquipmentSlot, Identifier, ItemStack,
    SpawnGroup, UnspawnedMobEdits, UnspawnedMobError, UnspawnedMobRef, WorldEpoch,
};
use std::sync::atomic::{AtomicU64, Ordering};

/// The most tags an entity holds (`Entity.addTag`).
const MAX_TAGS: usize = 1024;

/// Lends are numbered across every `Game` of the process, so no two calls'
/// handles are ever equal.
static NEXT_LEND: AtomicU64 = AtomicU64::new(1);

/// The sheets lent to the hook calls running now, the latest last.
#[derive(Default)]
pub(in crate::game) struct LentMobs(Vec<Lent>);

#[cfg(test)]
impl LentMobs {
    /// Whether no lend is open.
    pub(in crate::game) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// One lend: the handle's numbers, what every lend answers, and the sheet when
/// the game keeps one (not for the ender dragon).
struct Lent {
    handle: UnspawnedMobRef,
    kind: String,
    position: [f64; 3],
    sheet: Option<SpawnSheet>,
}

/// Ends a lend when it drops, after the call or while a listener's panic
/// unwinds: the lend is withdrawn, so its handle answers `Expired`, and the
/// lent sheet moves back into the caller's slot.
struct Withdraw<'a, 'g, 'm> {
    game: &'a Game<'g>,
    handle: UnspawnedMobRef,
    slot: Option<&'m mut SpawnSheet>,
}

impl Drop for Withdraw<'_, '_, '_> {
    fn drop(&mut self) {
        let lent = {
            let mut lent = self.game.lent_mobs.borrow_mut();
            let at = lent.0.iter().rposition(|l| l.handle == self.handle);
            at.map(|at| lent.0.remove(at))
        };
        if let (Some(slot), Some(Lent { sheet: Some(back), .. })) = (self.slot.take(), lent) {
            *slot = back;
        }
    }
}

/// The editor a lend hands out: it reaches only the lent sheets, through the
/// game it was made from.
struct Edits<'a, 'g> {
    game: &'a Game<'g>,
}

impl Game<'_> {
    /// Lends the sheet `sheet` (or, with `None`, only the `kind` and `position`
    /// of the spawn) to `call`, then takes it back with whatever the call
    /// changed. `call` gets the handle and the editor; both are dead once it
    /// returns. A listener that panics still ends the lend and puts the sheet
    /// back ([`Withdraw`]).
    pub(in crate::game) fn lend_unspawned<R>(
        &self,
        kind: &str,
        position: [f64; 3],
        sheet: Option<&mut SpawnSheet>,
        call: impl FnOnce(UnspawnedMobRef, &dyn UnspawnedMobEdits) -> R,
    ) -> R {
        let handle =
            UnspawnedMobRef::new(self.lend_epoch(), NEXT_LEND.fetch_add(1, Ordering::Relaxed));
        let (lent, slot) = match sheet {
            Some(slot) => (Some(std::mem::take(slot)), Some(slot)),
            None => (None, None),
        };
        self.lent_mobs.borrow_mut().0.push(Lent {
            handle,
            kind: kind.to_owned(),
            position,
            sheet: lent,
        });
        let _withdraw = Withdraw {
            game: self,
            handle,
            slot,
        };
        call(handle, &Edits { game: self })
    }

    /// The world a lend's handles belong to; a game with none open lends
    /// under epoch 0, which no open world has.
    pub(in crate::game) fn lend_epoch(&self) -> WorldEpoch {
        self.native_lifecycle.epoch().unwrap_or(WorldEpoch::new(0))
    }

    /// Runs `read` on the lend `handle` names.
    fn with_lent<T>(
        &self,
        handle: UnspawnedMobRef,
        read: impl FnOnce(&Lent) -> Result<T, UnspawnedMobError>,
    ) -> Result<T, UnspawnedMobError> {
        let lent = self.lent_mobs.borrow();
        read(
            lent.0
                .iter()
                .find(|l| l.handle == handle)
                .ok_or(UnspawnedMobError::Expired)?,
        )
    }

    /// Runs `read` on the sheet the lend `handle` names; `Unsupported` for a
    /// spawn with no sheet (the dragon's).
    fn with_lent_sheet<T>(
        &self,
        handle: UnspawnedMobRef,
        read: impl FnOnce(&Lent, &SpawnSheet) -> Result<T, UnspawnedMobError>,
    ) -> Result<T, UnspawnedMobError> {
        self.with_lent(handle, |l| {
            let sheet = l.sheet.as_ref().ok_or(UnspawnedMobError::Unsupported)?;
            read(l, sheet)
        })
    }

    /// Runs `edit` on the sheet the lend `handle` names, with the game and the
    /// spawn's kind.
    fn edit_lent_sheet<T>(
        &self,
        handle: UnspawnedMobRef,
        edit: impl FnOnce(&Game<'_>, &str, &mut SpawnSheet) -> Result<T, UnspawnedMobError>,
    ) -> Result<T, UnspawnedMobError> {
        let mut lent = self.lent_mobs.borrow_mut();
        let Lent { kind, sheet, .. } = lent
            .0
            .iter_mut()
            .find(|l| l.handle == handle)
            .ok_or(UnspawnedMobError::Expired)?;
        edit(
            self,
            kind.as_str(),
            sheet.as_mut().ok_or(UnspawnedMobError::Unsupported)?,
        )
    }

    /// The mob's entity type (`minecraft:zombie`).
    pub(super) fn unspawned_mob_kind(
        &self,
        mob: UnspawnedMobRef,
    ) -> Result<Identifier, UnspawnedMobError> {
        self.with_lent(mob, |l| {
            Identifier::parse(&l.kind).map_err(|_| UnspawnedMobError::Unsupported)
        })
    }

    /// Where the mob will join the level.
    pub(super) fn unspawned_mob_position(
        &self,
        mob: UnspawnedMobRef,
    ) -> Result<[f64; 3], UnspawnedMobError> {
        self.with_lent(mob, |l| Ok(l.position))
    }

    /// The stack in `slot` (`LivingEntity.getItemBySlot`) and its count; `None`
    /// for an empty slot. A spawn wears nothing until its edits or its
    /// finalization put gear on it.
    pub(super) fn unspawned_mob_equipment(
        &self,
        mob: UnspawnedMobRef,
        slot: EquipmentSlot,
    ) -> Result<Option<horizon_client::items::ItemStack>, UnspawnedMobError> {
        self.with_lent_sheet(mob, |_, sheet| Ok(sheet_stack(sheet, slot as usize)))
    }

    /// An attribute's base value, before modifiers (`AttributeMap.getBaseValue`).
    /// `Unsupported` for an attribute the mob's type does not have.
    pub(super) fn unspawned_mob_attribute_base(
        &self,
        mob: UnspawnedMobRef,
        attribute: &Identifier,
    ) -> Result<f64, UnspawnedMobError> {
        let catalog = &self.motion.attributes;
        self.with_lent_sheet(mob, |l, sheet| {
            let name = attribute.to_string();
            let species = || catalog.id(&name).and_then(|id| catalog.default_base(&l.kind, id));
            sheet_base(sheet, &name).or_else(species).ok_or(UnspawnedMobError::Unsupported)
        })
    }

    /// An attribute's value with its modifiers (`AttributeMap.getValue`).
    /// `Unsupported` for an attribute the mob's type does not have.
    pub(super) fn unspawned_mob_attribute_value(
        &self,
        mob: UnspawnedMobRef,
        attribute: &Identifier,
    ) -> Result<f64, UnspawnedMobError> {
        let catalog = &self.motion.attributes;
        let name = attribute.to_string();
        self.with_lent_sheet(mob, |l, sheet| {
            let worn = sheet_worn(self.ecs.world(), sheet);
            sheet_value(catalog, &l.kind, sheet, &worn, &name).ok_or(UnspawnedMobError::Unsupported)
        })
    }

    /// The tags a mod gave the mob so far (`Entity.getTags`).
    pub(super) fn unspawned_mob_tags(
        &self,
        mob: UnspawnedMobRef,
    ) -> Result<Vec<String>, UnspawnedMobError> {
        self.with_lent_sheet(mob, |_, sheet| Ok(sheet.tags.clone()))
    }

    /// The mob's custom name, if it has one.
    pub(super) fn unspawned_mob_custom_name(
        &self,
        mob: UnspawnedMobRef,
    ) -> Result<Option<Text>, UnspawnedMobError> {
        // `Text::with_component` replaces the whole component with the decoded JSON (the empty literal is discarded),
        // so the name keeps the style its JSON carries.
        self.with_lent_sheet(mob, |_, sheet| {
            Ok(sheet
                .name
                .as_deref()
                .and_then(|json| serde_json::from_str(json).ok())
                .map(|json| Text::literal("").with_component(json)))
        })
    }
}

/// The stack the sheet puts in `slot`: its last edit; None for no edit or an emptied slot.
fn sheet_stack(sheet: &SpawnSheet, slot: usize) -> Option<horizon_client::items::ItemStack> {
    sheet
        .equipment
        .iter()
        .rev()
        .find(|(s, _)| *s == slot)
        .and_then(|(_, stack)| stack.clone())
}

/// The modifiers the sheet's pieces grant, as `equip_attrs` grants them once worn (none without the host's table).
fn sheet_worn(world: Option<&World>, sheet: &SpawnSheet) -> Vec<(String, EcsModifier)> {
    let Some(table) = world.and_then(|w| w.get_resource::<ItemModifiers>()) else { return Vec::new() };
    (0..SLOTS).filter_map(|slot| sheet_stack(sheet, slot).map(|stack| (table.0)(&stack, slot))).flatten().collect()
}

/// The game's group data as the API's, a variant by its name in the species' table (`species::variant_names`)
/// and an axolotl's pair by its indexes there; None where the table has no name for it, or for a school (its
/// leader is not in the group yet).
pub(in crate::game) fn native_spawn_group(kind: &str, group: &Group) -> Option<SpawnGroup> {
    let names = horizon_ecs_proto::species::variant_names(kind);
    let name = |v: u32| usize::try_from(v).ok().and_then(|i| names.get(i)).map(|s| (*s).to_owned());
    Some(match *group {
        Group::Ageable { should_baby, size, chance, variant: None } => {
            SpawnGroup::Ageable(native_ageable(should_baby, size, chance))
        }
        Group::Zombie { baby, jockey, tried_camel } => SpawnGroup::Zombie { baby, jockey, tried_camel },
        Group::Spider { effect } => SpawnGroup::Spider {
            effect: match effect {
                Some(i) => Some(Identifier::parse(horizon_ecs_proto::species::spider::EFFECTS.get(usize::from(i))?).ok()?),
                None => None,
            },
        },
        Group::Shaped { shape, should_baby, size, chance, variant: Some(v) } => {
            let ageable = native_ageable(should_baby, size, chance);
            let variant = name(u32::from(v))?;
            match shape {
                Shape::Fox => SpawnGroup::Fox { ageable, variant },
                Shape::Horse => SpawnGroup::Horse { ageable, variant },
                Shape::Llama => SpawnGroup::Llama { ageable, variant },
                Shape::Rabbit => SpawnGroup::Rabbit { ageable, variant },
                Shape::Wolf => SpawnGroup::Wolf { ageable, variant: Identifier::parse(&variant).ok()? },
            }
        }
        Group::Pack { vals: [a, b], n: 2, size, should_baby, chance } if kind == "minecraft:axolotl" => SpawnGroup::Axolotl {
            ageable: native_ageable(should_baby, size, chance),
            variants: [name(a)?, name(b)?],
        },
        _ => return None,
    })
}

/// The API's group data as the game's, a variant as its index in the species' table; None for a value the game
/// does not hold, which drops the group.
pub(in crate::game) fn spawn_group(kind: &str, group: &SpawnGroup) -> Option<Group> {
    let names = horizon_ecs_proto::species::variant_names(kind);
    let index = |v: &str| names.iter().position(|n| *n == v).and_then(|i| u8::try_from(i).ok());
    let ageable = |a: &AgeableGroup| u32::try_from(a.size).ok().map(|size| (a.should_spawn_baby, size, a.baby_chance));
    let shaped = |a: &AgeableGroup, shape: Shape, variant: &str| {
        let (should_baby, size, chance) = ageable(a)?;
        Some(Group::Shaped { shape, should_baby, size, chance, variant: Some(index(variant)?) })
    };
    Some(match group {
        SpawnGroup::Ageable(a) => {
            let (should_baby, size, chance) = ageable(a)?;
            Group::Ageable { should_baby, size, chance, variant: None }
        }
        SpawnGroup::Zombie { baby, jockey, tried_camel } => Group::Zombie { baby: *baby, jockey: *jockey, tried_camel: *tried_camel },
        SpawnGroup::Spider { effect } => Group::Spider {
            effect: match effect {
                Some(e) => Some(
                    horizon_ecs_proto::species::spider::EFFECTS
                        .iter()
                        .position(|n| *n == e.to_string())
                        .and_then(|i| u8::try_from(i).ok())?,
                ),
                None => None,
            },
        },
        SpawnGroup::Fox { ageable: a, variant } => shaped(a, Shape::Fox, variant)?,
        SpawnGroup::Horse { ageable: a, variant } => shaped(a, Shape::Horse, variant)?,
        SpawnGroup::Llama { ageable: a, variant } => shaped(a, Shape::Llama, variant)?,
        SpawnGroup::Rabbit { ageable: a, variant } => shaped(a, Shape::Rabbit, variant)?,
        SpawnGroup::Wolf { ageable: a, variant } => shaped(a, Shape::Wolf, &variant.to_string())?,
        SpawnGroup::Axolotl { ageable: a, variants: [x, y] } => {
            let (should_baby, size, chance) = ageable(a)?;
            Group::Pack { vals: [u32::from(index(x)?), u32::from(index(y)?)], n: 2, size, should_baby, chance }
        }
        _ => return None,
    })
}

fn native_ageable(should_baby: bool, size: u32, chance: f32) -> AgeableGroup {
    let mut out = AgeableGroup::new(should_baby, chance);
    out.size = i32::try_from(size).unwrap_or(i32::MAX);
    out
}

impl UnspawnedMobEdits for Edits<'_, '_> {
    fn set_equipment(
        &self,
        mob: UnspawnedMobRef,
        slot: EquipmentSlot,
        item: Option<ItemStack>,
    ) -> Result<(), UnspawnedMobError> {
        self.game.edit_lent_sheet(mob, |game, _, sheet| {
            let stack = item
                .try_into_engine(world_items!(game))
                .map_err(|_| UnspawnedMobError::Invalid)?;
            if slot as usize >= SLOTS {
                return Err(UnspawnedMobError::Unsupported);
            }
            sheet.set_equipment(slot as usize, stack);
            Ok(())
        })
    }

    fn set_attribute_base(
        &self,
        mob: UnspawnedMobRef,
        attribute: &Identifier,
        base: f64,
    ) -> Result<(), UnspawnedMobError> {
        if !base.is_finite() {
            return Err(UnspawnedMobError::Invalid);
        }
        self.write_attribute(mob, attribute, AttributeWrite::Base(base))
            .map(|_| ())
    }

    fn add_attribute_modifier(
        &self,
        mob: UnspawnedMobRef,
        attribute: &Identifier,
        modifier: AttributeModifier,
    ) -> Result<(), UnspawnedMobError> {
        let operation = match modifier.operation {
            AttributeOperation::AddValue => Operation::AddValue,
            AttributeOperation::AddMultipliedBase => Operation::AddMultipliedBase,
            AttributeOperation::AddMultipliedTotal => Operation::AddMultipliedTotal,
            _ => return Err(UnspawnedMobError::Invalid),
        };
        if !modifier.amount.is_finite() {
            return Err(UnspawnedMobError::Invalid);
        }
        let write = AttributeWrite::add(
            &modifier.id.to_string(),
            modifier.amount,
            operation,
            true,
            false,
        )
        .ok_or(UnspawnedMobError::Invalid)?;
        match self.write_attribute(mob, attribute, write)? {
            true => Ok(()),
            false => Err(UnspawnedMobError::Invalid),
        }
    }

    fn remove_attribute_modifier(
        &self,
        mob: UnspawnedMobRef,
        attribute: &Identifier,
        id: &Identifier,
    ) -> Result<bool, UnspawnedMobError> {
        self.write_attribute(mob, attribute, AttributeWrite::Remove(id.to_string()))
    }

    fn add_tag(&self, mob: UnspawnedMobRef, tag: &str) -> Result<bool, UnspawnedMobError> {
        self.game.edit_lent_sheet(mob, |_, _, sheet| {
            let tags = &mut sheet.tags;
            if tags.iter().any(|t| t == tag) {
                return Ok(false);
            }
            if tag.is_empty() || tags.len() >= MAX_TAGS {
                return Err(UnspawnedMobError::Invalid);
            }
            tags.push(tag.to_owned());
            Ok(true)
        })
    }

    fn remove_tag(&self, mob: UnspawnedMobRef, tag: &str) -> Result<bool, UnspawnedMobError> {
        self.game.edit_lent_sheet(mob, |_, _, sheet| {
            let tags = &mut sheet.tags;
            let before = tags.len();
            tags.retain(|t| t != tag);
            Ok(tags.len() != before)
        })
    }

    fn set_custom_name(
        &self,
        mob: UnspawnedMobRef,
        name: Option<mod_api::Text>,
    ) -> Result<(), UnspawnedMobError> {
        let name = name.try_into_engine(&())?;
        self.game.edit_lent_sheet(mob, |_, _, sheet| {
            sheet.name = name.as_ref().map(|t| t.component().to_string());
            Ok(())
        })
    }
}

impl Edits<'_, '_> {
    /// An attribute write on the lent sheet; `Unsupported` for an attribute the
    /// species does not have.
    fn write_attribute(
        &self,
        mob: UnspawnedMobRef,
        attribute: &Identifier,
        write: AttributeWrite,
    ) -> Result<bool, UnspawnedMobError> {
        let name = attribute.to_string();
        self.game.edit_lent_sheet(mob, |game, kind, sheet| {
            write_unspawned_attribute(&game.motion.attributes, kind, sheet, &name, write)
                .ok_or(UnspawnedMobError::Unsupported)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::creatures::SpawnSite;
    use super::super::native_entity_refs::tests::with_game;
    use super::*;
    use horizon_client::mobs::spawn_rules::Reason;
    use mod_api::{FinalizeSpawnEvent, Flow, Host};
    use std::sync::{Arc, Mutex};

    type Listen = dyn Fn(&dyn Host, &mut FinalizeSpawnEvent<'_>) -> Flow + Send + Sync;
    /// Answers `finalize_spawn` with the test's closure.
    struct Listener(Box<Listen>);
    #[mod_api::__subscriptions]
    impl mod_api::Hooks for Listener {
        fn finalize_spawn(&self, host: &dyn Host, event: &mut FinalizeSpawnEvent<'_>) -> Flow {
            (self.0)(host, event)
        }
    }
    fn listening<R>(
        listen: impl Fn(&dyn Host, &mut FinalizeSpawnEvent<'_>) -> Flow + Send + Sync + 'static,
        run: impl FnOnce() -> R,
    ) -> R {
        let listener = Listener(Box::new(listen));
        mod_platform::testing::with_mods(
            vec![mod_api::ModEntry::new("unspawned", Box::new(listener))].into(),
            run,
        )
    }

    const AT: [f64; 3] = [8.5, 64.0, 8.5];
    fn spawn_pig(game: &mut Game<'_>) -> Option<u32> {
        game.spawn_mob(
            "minecraft:pig",
            AT,
            false,
            SpawnSite::Finalizing(Reason::Natural),
        )
    }

    /// A handle kept past its hook reads `Expired`, and the next call's
    /// editor refuses it the same way: it reaches only its own lend.
    #[test]
    #[ignore = "daily"]
    fn a_handle_kept_past_its_hook_is_expired() {
        let kept: Arc<Mutex<Vec<UnspawnedMobRef>>> = Arc::default();
        let refused = Arc::new(Mutex::new(Vec::new()));
        let (keep, refuse) = (kept.clone(), refused.clone());
        listening(
            move |_, event| {
                let earlier = keep.lock().unwrap().last().copied();
                if let Some(earlier) = earlier {
                    refuse
                        .lock()
                        .unwrap()
                        .push(event.edits.add_tag(earlier, "test:late"));
                }
                keep.lock().unwrap().push(event.mob);
                Flow::Continue
            },
            || {
                with_game(|game| {
                    spawn_pig(game).unwrap();
                    spawn_pig(game).unwrap();
                    let old = kept.lock().unwrap()[0];
                    assert_eq!(old.kind(game), Err(UnspawnedMobError::Expired));
                    assert_eq!(old.position(game), Err(UnspawnedMobError::Expired));
                });
            },
        );
        assert_eq!(*refused.lock().unwrap(), [Err(UnspawnedMobError::Expired)]);
        let kept = kept.lock().unwrap();
        assert_ne!(kept[0], kept[1], "each call lends anew");
    }

    /// A listener that panics still ends its lend: the handle is withdrawn
    /// and the sheet in the caller's slot is the one that was lent, its edits
    /// and all.
    #[test]
    #[ignore = "daily"]
    fn a_panicking_listener_still_ends_the_lend() {
        with_game(|game| {
            let mut sheet = SpawnSheet {
                name: Some("\"Lent\"".to_owned()),
                ..SpawnSheet::default()
            };
            let mut kept = None;
            let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                game.lend_unspawned("minecraft:zombie", AT, Some(&mut sheet), |handle, _| {
                    kept = Some(handle);
                    panic!("a listener's bug");
                })
            }));
            assert!(panicked.is_err());
            assert!(game.lent_mobs.borrow().is_empty(), "the lend is withdrawn");
            let kept = kept.expect("the call ran");
            assert_eq!(kept.kind(game), Err(UnspawnedMobError::Expired));
            assert_eq!(sheet.name.as_deref(), Some("\"Lent\""));
        });
    }

    /// Group data crosses the boundary as the API's ageable and zombie groups,
    /// both ways; the other kinds are not held yet (see the TODO above).
    #[test]
    fn group_data_crosses_both_ways() {
        let cases = [
            (
                Group::Ageable {
                    should_baby: true,
                    size: 2,
                    chance: 0.05,
                    variant: None,
                },
                SpawnGroup::Ageable({
                    let mut a = AgeableGroup::new(true, 0.05);
                    a.size = 2;
                    a
                }),
            ),
            (
                Group::Zombie {
                    baby: true,
                    jockey: false,
                    tried_camel: None,
                },
                SpawnGroup::Zombie {
                    baby: true,
                    jockey: false,
                    tried_camel: None,
                },
            ),
        ];
        let effect = Identifier::parse("minecraft:invisibility").unwrap();
        assert_eq!(
            native_spawn_group("minecraft:spider", &Group::Spider { effect: Some(3) }),
            Some(SpawnGroup::Spider { effect: Some(effect.clone()) })
        );
        assert_eq!(spawn_group("minecraft:spider", &SpawnGroup::Spider { effect: Some(effect) }), Some(Group::Spider { effect: Some(3) }));
        for (game, api) in cases {
            assert_eq!(native_spawn_group("minecraft:zombie", &game).as_ref(), Some(&api));
            assert_eq!(spawn_group("minecraft:zombie", &api), Some(game));
        }
    }
}
