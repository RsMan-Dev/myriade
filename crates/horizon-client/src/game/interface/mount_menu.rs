//! The horse inventory's server state (`HorseInventoryMenu`): the saddle, body armour and
//! chest slots of the mount the window's player opened. A server's players get theirs from
//! game/network/lan_mount_menu.rs.
use super::Game;
use horizon_client::{
    items::ItemStack,
    mobs::{Species, equine, llama, nautilus},
};
use horizon_core::nbt::Tag;

/// Menu slot ids: the saddle, the body armour and the chest from 710.
pub(in crate::game) const SADDLE: usize = 700;
pub(in crate::game) const ARMOR: usize = 701;
pub(in crate::game) const CHEST: usize = 710;
/// `AbstractChestedHorse.INV_CHEST_COUNT` and `getInventoryColumns`.
const CHEST_SIZE: usize = 15;
const COLUMNS: usize = 5;

/// Whether the mount has a horse inventory (`AbstractHorse`, `AbstractNautilus`).
pub(in crate::game) fn has_mount_menu(species: Species) -> bool {
    equine::is_equine(species) || llama::is_llama(species) || nautilus::is_nautilus(species)
}
/// The saddle slot is active (`canUseSlot(SADDLE)` and `CAN_EQUIP_SADDLE`):
/// not on llamas.
fn shows_saddle(species: Species) -> bool {
    !llama::is_llama(species)
}
/// The body slot is active: horse armour, a llama's carpet, or nautilus armour.
fn shows_body(species: Species) -> bool {
    can_wear_horse_armor(species) || llama::is_llama(species) || nautilus::is_nautilus(species)
}

/// The open mount menu: copies of the mount's slots, written back on every change.
#[derive(Default)]
pub(in crate::game) struct MountMenu {
    open: Option<Open>,
}
impl MountMenu {
    /// The mount whose menu is open.
    pub(in crate::game) fn opened_mob(&self) -> Option<u32> {
        self.open.as_ref().map(|o| o.mob)
    }
}
struct Open {
    mob: u32,
    species: Species,
    columns: usize,
    saddle: Option<ItemStack>,
    armor: Option<ItemStack>,
    items: Vec<Option<ItemStack>>,
    /// `shouldRenderSaddleSlot` / `shouldRenderArmorSlot`.
    show_saddle: bool,
    show_armor: bool,
}

/// `EntityTypeTags.CAN_WEAR_HORSE_ARMOR`.
fn can_wear_horse_armor(species: Species) -> bool {
    matches!(species, Species::Horse | Species::ZombieHorse)
}
/// Whether `item` fits the body slot of the species (`isEquippableInSlot`).
fn body_armor_fits(species: Species, item: &str) -> bool {
    if llama::is_llama(species) {
        return item.ends_with("_carpet");
    }
    // EntityTypeTags.CAN_WEAR_NAUTILUS_ARMOR: the nautilus armours.
    if nautilus::is_nautilus(species) {
        return item.ends_with("_nautilus_armor");
    }
    can_wear_horse_armor(species) && item.ends_with("_horse_armor")
}

impl Game<'_> {
    /// `AbstractHorse.openCustomInventoryScreen` for a tamed adult mount.
    pub(in crate::game) fn open_mount_menu(&mut self, id: u32) {
        let Some(species) = self.creature_species(id) else {
            return;
        };
        if !has_mount_menu(species) {
            return;
        }
        let Some((saddle, body, mut items)) = self.mount_contents_of(id) else {
            return;
        };
        items.resize(CHEST_SIZE, None);
        let show_armor = shows_body(species);
        self.mount_menu.open = Some(Open {
            mob: id,
            species,
            columns: self.mount_columns_of(id).unwrap_or(0),
            saddle,
            armor: body.flatten().filter(|_| show_armor),
            items,
            show_saddle: shows_saddle(species),
            show_armor,
        });
        self.show_mount_menu();
    }
    fn show_mount_menu(&mut self) {
        self.station = None;
        self.container = None;
        self.craft_size = 2;
    }
    /// E while riding a tamed mount opens its inventory instead of the player's.
    pub(in crate::game) fn open_ridden_mount_menu(&mut self) -> bool {
        let Some(id) = self.riding else {
            return false;
        };
        let Some(species) = self.creature_species(id) else {
            return false;
        };
        if !has_mount_menu(species) || !self.mount_tamed(id) || !self.ecs.alive(id) {
            return false;
        }
        self.open_mount_menu(id);
        true
    }
    /// The mount `id` is tamed: an `Owner`, or the `Tame` flag of a horse tamed without one.
    fn mount_tamed(&self, id: u32) -> bool {
        self.ecs_mob_tree(id).is_some_and(|t| {
            let t = Tag::Compound(t);
            t.get("Owner").is_some() || matches!(t.get("Tame"), Some(Tag::Byte(v)) if *v != 0)
        })
    }
    /// Riding a mount that has a horse inventory (`HasCustomInventoryScreen`).
    pub(in crate::game) fn riding_mount_with_menu(&self) -> bool {
        self.riding.and_then(|id| self.creature_species(id)).is_some_and(has_mount_menu)
    }
    /// A server closed its window: the screen's copy goes.
    pub(in crate::game) fn mount_menu_reset(&mut self) {
        self.mount_menu.open = None;
    }
    /// The entity type of the open mount menu's mount (`minecraft:horse`).
    pub(in crate::game) fn mount_menu_entity(&self) -> Option<String> {
        let open = self.mount_menu.open.as_ref()?;
        self.mount_kind_of(open.mob)
    }
    /// The chest columns of the open mount menu.
    pub(in crate::game) fn mount_menu_columns(&self) -> Option<usize> {
        self.mount_menu.open.as_ref().map(|o| o.columns)
    }
    /// The menu ends when the mount dies or its box leaves the entity
    /// interaction range plus 4 of the player's eyes
    /// (`AbstractMountInventoryMenu.stillValid` with
    /// `Player.isWithinEntityInteractionRange(mount, 4.0)`: 7 blocks away, 9
    /// in creative).
    pub(in crate::game) fn tick_mount_menu(&mut self) {
        let Some(open) = self.mount_menu.open.as_ref() else {
            return;
        };
        let mob = open.mob;
        let valid = self.ecs.alive(mob) && self.ecs.bounds(mob).is_some_and(|b| self.within_entity_interaction_range(&b, 4.0));
        if !valid {
            self.close_container();
        }
    }
    /// Closing the screen keeps what is in the slots on the mount.
    pub(in crate::game) fn mount_closed(&mut self) {
        self.mount_sync();
        self.mount_menu.open = None;
    }
    /// Writes the menu slots to the mount, with the equip sounds.
    fn mount_sync(&mut self) {
        let Some(open) = self.mount_menu.open.as_ref() else {
            return;
        };
        let (mob, saddle, armor) = (open.mob, open.saddle.clone(), open.armor.clone());
        let body = open.show_armor.then_some(armor);
        let items = open.items.clone();
        self.mount_store(mob, saddle, body, items);
    }
    /// The menu's slots back on mount `id` (`body` None: not its slot), with the equip sound of what was put on
    /// (`Equippable.equipSound`) and the equip event.
    pub(in crate::game) fn mount_store(
        &mut self,
        id: u32,
        saddle: Option<ItemStack>,
        body: Option<Option<ItemStack>>,
        mut items: Vec<Option<ItemStack>>,
    ) {
        let (Some(bounds), Some(pos), Some((saddled, body_now, _))) =
            (self.ecs.bounds(id), self.ecs.position(id), self.mount_contents_of(id))
        else {
            return;
        };
        let at = bounds.center();
        let at = [at.x, at.y, at.z];
        let body_id = body_now.flatten();
        let saddle_put_on = saddle.is_some() && saddled.is_none();
        let put_on = if saddle_put_on {
            saddle.as_ref()
        } else {
            body.as_ref().and_then(Option::as_ref).filter(|_| body_id.is_none())
        };
        let sound = put_on.map(|s| {
            horizon_client::item_sync::equip_sound(s, self.res.items())
                .unwrap_or_else(|| "minecraft:item.armor.equip_generic".into())
        });
        let slot = if saddle_put_on { "saddle" } else { "body" };
        let sound = match (sound, put_on) {
            (Some(sound), Some(stack)) => self.mod_equip_sound(stack, slot, Some(sound)),
            (sound, _) => sound,
        };
        let saddle_off = saddled.is_some() && saddle.is_none();
        let body_off = body.as_ref().is_some_and(|b| b.is_none() && body_id.is_some());
        let body_on = body.as_ref().is_some_and(|b| b.is_some() && body_id.is_none());
        items.resize(CHEST_SIZE, None);
        self.ecs.mount_store(id, saddle, body, items);
        let (x, z) = (at[0].floor() as i32 >> 4, at[2].floor() as i32 >> 4);
        self.world.modified.insert((x, z));
        if let Some(sound) = sound {
            self.audio.play(&sound, Some(at), 1.0, 1.0);
        }
        if saddle_put_on || body_on {
            self.mob_equip_event(id, pos, true);
        } else if saddle_off || body_off {
            self.mob_equip_event(id, pos, false);
        }
    }
    pub(in crate::game) fn mount_slot(&self, i: usize) -> &Option<ItemStack> {
        match self.mount_menu.open.as_ref() {
            Some(open) if i == SADDLE => &open.saddle,
            Some(open) if i == ARMOR => &open.armor,
            Some(open) if i >= CHEST && i - CHEST < CHEST_SIZE => &open.items[i - CHEST],
            _ => &self.scratch_slot,
        }
    }
    pub(in crate::game) fn mount_slot_mut(&mut self, i: usize) -> &mut Option<ItemStack> {
        match self.mount_menu.open.as_mut() {
            Some(open) if i == SADDLE => &mut open.saddle,
            Some(open) if i == ARMOR => &mut open.armor,
            Some(open) if i >= CHEST && i - CHEST < CHEST_SIZE => &mut open.items[i - CHEST],
            _ => &mut self.scratch_slot,
        }
    }
    /// `ArmorSlot.mayPlace` and the stack limits of the mount's slots.
    pub(in crate::game) fn mount_slot_rules(&self, slot: usize, stack: &ItemStack) -> (bool, u32) {
        let Some(open) = self.mount_menu.open.as_ref() else {
            return (false, 64);
        };
        match slot {
            SADDLE => (&*stack.item == "minecraft:saddle", 1),
            ARMOR => (body_armor_fits(open.species, &stack.item), 1),
            _ => (true, 64),
        }
    }
    /// The inventory slot ids in menu order.
    fn mount_player_slots() -> Vec<usize> {
        (9..36).chain(0..9).collect()
    }
    /// `AbstractMountInventoryMenu.quickMoveStack`.
    fn mount_quick_move(&mut self, slot: usize) {
        let Some(open) = self.mount_menu.open.as_ref() else {
            return;
        };
        let (columns, show_saddle, show_armor) = (open.columns, open.show_saddle, open.show_armor);
        let Some(mut stack) = self.slot_mut(slot).take() else {
            return;
        };
        if slot >= SADDLE {
            let player = Self::mount_player_slots();
            self.move_to(&mut stack, &player, true);
        } else {
            let armor_free = self.mount_slot_mut(ARMOR).is_none();
            let saddle_free = self.mount_slot_mut(SADDLE).is_none();
            let armor_ok = show_armor && self.mount_slot_rules(ARMOR, &stack).0 && armor_free;
            let saddle_ok = show_saddle && self.mount_slot_rules(SADDLE, &stack).0 && saddle_free;
            if armor_ok {
                self.move_to(&mut stack, &[ARMOR], false);
            } else if saddle_ok {
                self.move_to(&mut stack, &[SADDLE], false);
            } else {
                let chest: Vec<usize> = (0..columns * 3).map(|i| CHEST + i).collect();
                let moved = columns > 0 && self.move_to(&mut stack, &chest, false);
                if !moved {
                    // Between the hotbar and the main inventory.
                    let other: Vec<usize> = if slot < 9 {
                        (9..36).collect()
                    } else {
                        (0..9).collect()
                    };
                    self.move_to(&mut stack, &other, false);
                }
            }
        }
        if stack.count > 0 {
            *self.slot_mut(slot) = Some(stack);
        }
    }
}
