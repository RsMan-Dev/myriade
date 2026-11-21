//! The horse inventory's server state (`HorseInventoryMenu`): the saddle, body armour and
//! chest slots of the mount the window's player opened. A server's players get theirs from
//! game/network/lan_mount_menu.rs.
use super::{Game, Screen};
use horizon_client::{
    items::ItemStack,
    mobs::{Mob, Species, equine, llama, nautilus},
};

/// Menu slot ids: the saddle, the body armour and the chest from 710.
pub(in crate::game) const SADDLE: usize = 700;
pub(in crate::game) const ARMOR: usize = 701;
pub(in crate::game) const CHEST: usize = 710;
/// `AbstractChestedHorse.INV_CHEST_COUNT` and `getInventoryColumns`.
const CHEST_SIZE: usize = 15;
const COLUMNS: usize = 5;

/// `AbstractHorse.getInventoryColumns`: five for a donkey's or mule's chest,
/// a llama's strength for its chest, else none.
pub(in crate::game) fn mount_columns(m: &Mob) -> usize {
    let species = m.species();
    if llama::is_llama(species) {
        llama::inventory_columns(m).clamp(0, 5) as usize
    } else if m.extra.chested && matches!(species, Species::Donkey | Species::Mule) {
        COLUMNS
    } else {
        0
    }
}
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
    /// Whether the mount menu is the open screen.
    pub(in crate::game) fn mount_active(&self) -> bool {
        self.screen == Screen::Inventory && self.mount_menu.open.is_some()
    }
    fn mount_mob_index(&self, id: u32) -> Option<usize> {
        self.remote_entities.iter().position(|m| m.id == id)
    }
    /// `AbstractHorse.openCustomInventoryScreen` for a tamed adult mount.
    pub(in crate::game) fn open_mount_menu(&mut self, index: usize) {
        let m = &self.remote_entities[index];
        let species = m.species();
        if !has_mount_menu(species) {
            return;
        }
        let mut items = m.extra.items.clone();
        items.resize(CHEST_SIZE, None);
        let show_armor = shows_body(species);
        self.mount_menu.open = Some(Open {
            mob: m.id,
            species,
            columns: mount_columns(m),
            saddle: m.saddle_item(),
            armor: show_armor
                .then(|| m.equipment_stacks()[6].clone())
                .flatten(),
            items,
            show_saddle: shows_saddle(species),
            show_armor,
        });
        self.show_mount_menu();
    }
    fn show_mount_menu(&mut self) {
        if self.lan_acting.is_none() {
            self.carried = None;
        }
        self.station = None;
        self.container = None;
        self.craft_size = 2;
    }
    /// E while riding a tamed mount opens its inventory instead of the player's.
    pub(in crate::game) fn open_ridden_mount_menu(&mut self) -> bool {
        let Some(id) = self.riding else {
            return false;
        };
        let Some(index) = self.mount_mob_index(id) else {
            return false;
        };
        let m = &self.remote_entities[index];
        if !has_mount_menu(m.species()) || !m.extra.tame || !m.alive() {
            return false;
        }
        self.open_mount_menu(index);
        true
    }
    /// Riding a mount that has a horse inventory (`HasCustomInventoryScreen`).
    pub(in crate::game) fn riding_mount_with_menu(&self) -> bool {
        self.riding
            .and_then(|id| self.mount_mob_index(id))
            .is_some_and(|i| has_mount_menu(self.remote_entities[i].species()))
    }
    /// A server closed its window: the screen's copy goes.
    pub(in crate::game) fn mount_menu_reset(&mut self) {
        self.mount_menu.open = None;
    }
    /// The entity type of the open mount menu's mount (`minecraft:horse`).
    pub(in crate::game) fn mount_menu_entity(&self) -> Option<String> {
        let open = self.mount_menu.open.as_ref()?;
        let m = self.remote_entities.iter().find(|m| m.id == open.mob)?;
        Some(m.kind.id.to_string())
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
        let mount = self
            .mount_mob_index(mob)
            .map(|i| (self.remote_entities[i].alive(), self.remote_entities[i].bounds()));
        let valid = match mount {
            Some((true, bounds)) => self.within_entity_interaction_range(&bounds, 4.0),
            _ => false,
        };
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
        let Some(index) = self.mount_mob_index(open.mob) else {
            return;
        };
        let (saddle, armor) = (open.saddle.clone(), open.armor.clone());
        let body = open.show_armor.then_some(armor);
        let items = open.items.clone();
        self.mount_store(index, saddle, body, items);
    }
    /// The slots of mount `index` as its menu shows them: the saddle, the
    /// body item (None when the slot isn't active), the chest.
    pub(in crate::game) fn mount_contents(
        &self,
        index: usize,
    ) -> (
        Option<ItemStack>,
        Option<Option<ItemStack>>,
        Vec<Option<ItemStack>>,
    ) {
        let m = &self.remote_entities[index];
        let species = m.species();
        let body = shows_body(species).then(|| m.equipment_stacks()[6].clone());
        let mut items = m.extra.items.clone();
        items.resize(mount_columns(m) * 3, None);
        (m.saddle_item(), body, items)
    }
    /// The menu's slots back on mount `index` (`body` None: not its slot),
    /// with the equip sound of what was put on (`Equippable.equipSound`).
    pub(in crate::game) fn mount_store(
        &mut self,
        index: usize,
        saddle: Option<ItemStack>,
        body: Option<Option<ItemStack>>,
        items: Vec<Option<ItemStack>>,
    ) {
        let m = &self.remote_entities[index];
        let at = m.center();
        let saddle_put_on = saddle.is_some() && !m.extra.saddled();
        let put_on = if saddle_put_on {
            saddle.as_ref()
        } else {
            body.as_ref()
                .and_then(Option::as_ref)
                .filter(|_| m.extra.body_id().is_none())
        };
        let sound = if saddle_put_on && nautilus::is_nautilus(m.species()) {
            // AbstractNautilus.getEquipSound: its own saddle sounds.
            Some(format!("minecraft:{}", nautilus::saddle_sound(m)))
        } else {
            put_on.map(|s| {
                horizon_client::item_sync::equip_sound(s, self.res.items())
                    .unwrap_or_else(|| "minecraft:item.armor.equip_generic".into())
            })
        };
        let slot = if saddle_put_on { "saddle" } else { "body" };
        let sound = match (sound, put_on) {
            (Some(sound), Some(stack)) => self.mod_equip_sound(stack, slot, Some(sound)),
            (sound, _) => sound,
        };
        let m = &mut self.remote_entities[index];
        let id = m.id;
        let pos = m.position;
        let saddle_off = m.extra.saddled() && saddle.is_none();
        let body_off = body
            .as_ref()
            .is_some_and(|b| b.is_none() && m.extra.body_id().is_some());
        let body_on = body
            .as_ref()
            .is_some_and(|b| b.is_some() && m.extra.body_id().is_none());
        m.set_saddle(saddle);
        if let Some(body) = body {
            m.extra.set_body_stack(body);
        }
        let mut items = items;
        let keep = m.extra.items.len().max(CHEST_SIZE);
        items.resize(keep, None);
        m.extra.items = items;
        let (x, z) = (at[0].floor() as i32 >> 4, at[2].floor() as i32 >> 4);
        self.world.modified.insert((x, z));
        if let Some(sound) = sound {
            self.audio.play(&sound, Some(at), 1.0, 1.0);
        }
        if saddle_put_on || body_on {
            self.mob_equip_event(id, [pos.x, pos.y, pos.z], true);
        } else if saddle_off || body_off {
            self.mob_equip_event(id, [pos.x, pos.y, pos.z], false);
        }
    }
    pub(in crate::game) fn mount_slots(&self) -> Option<Vec<usize>> {
        let open = self
            .mount_menu
            .open
            .as_ref()
            .filter(|_| self.mount_active())?;
        let mut v: Vec<usize> = (0..36).collect();
        if open.show_saddle {
            v.push(SADDLE);
        }
        if open.show_armor {
            v.push(ARMOR);
        }
        v.extend((0..open.columns * 3).map(|i| CHEST + i));
        Some(v)
    }
    /// How many slots `mount_slots` lists, counted without building it.
    pub(in crate::game) fn mount_slot_count(&self) -> Option<usize> {
        let open = self
            .mount_menu
            .open
            .as_ref()
            .filter(|_| self.mount_active())?;
        Some(36 + usize::from(open.show_saddle) + usize::from(open.show_armor) + open.columns * 3)
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
