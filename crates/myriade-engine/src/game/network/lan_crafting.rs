//! Crafting for LAN guests: the inventory's 2x2 grid (window 0) and the
//! crafting table (`minecraft:crafting`) run the host's menu engine
//! (`myriade_engine::menu`, `Kind::Inventory` / `Kind::Crafting`) over the
//! guest's inventory, with vanilla's packets: OpenScreen, container clicks
//! and their corrections, and the grid back into the inventory when the menu
//! closes (`AbstractContainerMenu.clearContainer`).
use super::{Game, transactions::decision::MenuOwner};
use myriade_engine::lan::recipes;
use myriade_engine::{
    containers::BlockEntity,
    furnace::FurnaceCooking,
    items::ItemStack,
    lan::{guest::Guest, menu::Slots},
    menu::{
        Env, Input, Kind, Menu,
        place::{PostPlace, TransferCheck, TransferError},
    },
};
use horizon_protocol::packets::play::{self, Clientbound as C};

/// `AbstractContainerMenu.dropOrPlaceInInventory`: actual network disconnects
/// drop inputs; a live close still returns them to the inventory.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum MenuRemoval {
    Live,
    Disconnected,
}

/// The guest's menus the game runs: kept between clicks, so a drag (quick
/// craft) spans them as in vanilla.
#[derive(Default)]
pub(super) struct GuestCrafting {
    /// The inventory menu; its grid lives in the hub's guest.
    pub(super) inventory: MenuOwner<Option<Box<Menu>>>,
    /// The open crafting table: its window, state id, menu and block.
    pub(super) table: MenuOwner<Option<(i32, i32, Box<Menu>, [i32; 3])>>,
    /// The open furnace's or brewing stand's window and menu (`AbstractFurnaceMenu`,
    /// `BrewingStandMenu`); its slots live in the block entity and the hub's
    /// container menu keeps the client in sync.
    pub(super) machine: MenuOwner<Option<(i32, Box<Menu>)>>,
}

/// `GuestPlay::recipe_checks`: what one viewer check adds, and from what the
/// next goes unanswered: ten at once, then one a second.
const RECIPE_CHECK_COST: u32 = 20;
const RECIPE_CHECK_LIMIT: u32 = 200;

/// ServerboundContainerClickPacket's input.
pub(super) fn input(id: i32) -> Input {
    match id {
        0 => Input::Pickup,
        1 => Input::QuickMove,
        2 => Input::Swap,
        3 => Input::Clone,
        4 => Input::Throw,
        5 => Input::QuickCraft,
        _ => Input::PickupAll,
    }
}

/// The armor's player-container slots, head to feet (`Inventory` 39..36).
const ARMOR: [usize; 4] = [39, 38, 37, 36];
const OFFHAND: usize = 40;

/// The guest's inventory, armor, off hand, selection and carried stack into `menu`.
pub(super) fn load_player(menu: &mut Menu, g: &Guest) {
    for i in 0..36 {
        menu.set_player_item(i, g.inventory[i].clone());
    }
    for (k, slot) in ARMOR.into_iter().enumerate() {
        menu.set_player_item(slot, g.armor[k].clone());
    }
    menu.set_player_item(OFFHAND, g.offhand.clone());
    menu.selected = g.selected;
    menu.creative = g.creative();
    menu.levels = g.xp_level;
    menu.carried = g.carried.clone();
}

/// The vanilla menu engine's kind for a guest's menu over block entity `entity`, with the
/// container's slots: a furnace's, a brewing stand's or a crafter's. None for the others, whose
/// clicks the hub runs.
pub(super) fn machine_menu(entity: &BlockEntity) -> Option<(Kind, Vec<Option<ItemStack>>)> {
    Some(match entity {
        BlockEntity::Furnace(f) => {
            let recipe = match f.kind.recipe_type() {
                "minecraft:blasting" => "blasting",
                "minecraft:smoking" => "smoking",
                _ => "smelting",
            };
            (Kind::Furnace(recipe), f.items.to_vec())
        }
        BlockEntity::Brewing(b) => (Kind::Brewing, b.items.to_vec()),
        BlockEntity::Crafter(c) => (Kind::CrafterBlock, c.items.to_vec()),
        _ => return None,
    })
}

/// `load_player` back.
pub(super) fn store_player(menu: &mut Menu, g: &mut Guest) {
    for i in 0..36 {
        g.inventory[i] = menu.player_item(i).cloned();
    }
    for (k, slot) in ARMOR.into_iter().enumerate() {
        g.armor[k] = menu.player_item(slot).cloned();
    }
    g.offhand = menu.player_item(OFFHAND).cloned();
    g.carried = menu.carried.take();
}

/// Inventory.placeItemBackInInventory for each stack: what doesn't fit is
/// left for Player.drop (creative players keep it too).
fn place_back(menu: &mut Menu, env: &Env, stacks: Vec<ItemStack>) {
    menu.creative = false;
    for mut stack in stacks {
        menu.give(env, &mut stack);
        if stack.count > 0 {
            menu.dropped.push(stack);
        }
    }
}

impl Game<'_> {
    /// `LocalPlayer.doLimitedCrafting`: whether limited crafting is active.
    pub fn limited_crafting(&self) -> bool {
        self.session
            .rules
            .bool_at(myriade_engine::gamerules::vanilla::LIMITED_CRAFTING)
    }

    pub(super) fn menu_env(&self) -> Env<'_> {
        Env {
            crafting_remainder: Some(self),
            player: self.lan_acting_uuid,
            player_entity_id: Some(self.own_entity_id()),
            loop_context: &self.loop_context,
            items: self.items(),
            recipes: self.recipes(),
            brewing: &self.stations().brewing,
            banners: &self.stations().banners,
            station_recipes: &self.stations().recipes,
            enchantments: &self.stations().enchantments,
            names: &self.pack.names,
            trade: None,
            maps: None,
            limited_crafting: self.limited_crafting(),
            can_equip: None,
            known_recipes: Some(&self.level.recipe_book),
            slot_rules: None,
        }
    }

    /// `Item.getCraftingRemainingItem` of the stack: the item left when one of its items is
    /// used up; None when it leaves nothing.
    pub(in crate::game) fn crafting_remainder(&self, stack: &ItemStack) -> Option<ItemStack> {
        myriade_engine::crafting::CraftingRemainder::crafting_remainder(self.items(), stack)
            .filter(|value| value.count > 0)
    }
}

impl myriade_engine::crafting::CraftingRemainder for Game<'_> {
    fn crafting_remainder(&self, stack: &ItemStack) -> Option<ItemStack> {
        Game::crafting_remainder(self, stack)
    }
}

impl Game<'_> {

    /// Runs `op` on the guest's menu with their inventory copied in and back.
    pub(super) fn guest_menu_op<R>(
        &mut self,
        gi: usize,
        menu: &mut Menu,
        op: impl FnOnce(&mut Menu, &Env) -> R,
    ) -> Option<R> {
        self.lan.as_ref()?;
        // Crafting and cartography read and derive this level's saved maps.
        let maps = matches!(
            menu.kind(),
            Kind::Cartography | Kind::Crafting | Kind::Inventory
        )
        .then(|| {
            let g = &self.lan.as_ref().unwrap().hub.guests[gi];
            (0..menu.slot_count())
                .filter_map(|s| menu.slot(s).cloned())
                .chain(g.inventory.iter().flatten().cloned())
                .chain(g.carried.clone())
                .collect::<Vec<_>>()
        })
        .filter(|held| {
            menu.kind() == Kind::Cartography
                || held
                    .iter()
                    .filter_map(super::maps::map_id)
                    .any(|id| self.map(id).is_some())
        })
        .map(|held| self.level_maps(&held));
        let host = self.lan.as_ref()?;
        // `ArmorSlot.mayPlace` asks the parts with the game as `Host`: the menu is the guest's own.
        let parts = self.menu_can_equip();
        let env = Env {
            crafting_remainder: Some(self),
            player: Some(host.hub.guests[gi].uuid),
            player_entity_id: Some(host.hub.guests[gi].entity_id),
            loop_context: &self.loop_context,
            maps: maps.as_ref().map(|m| m as &dyn myriade_engine::menu::Maps),
            limited_crafting: host.hub.limited_crafting,
            can_equip: Some(&parts),
            known_recipes: Some(&host.hub.guests[gi].recipe_book),
            ..self.menu_env()
        };
        let limits = host.hub.guests[gi]
            .menu
            .as_ref()
            .map(|opened| opened.slots.stack_limits(&self.world, opened.size))
            .unwrap_or_default();
        menu.set_container_limits(limits);
        menu.set_block_pos(
            host.hub.guests[gi]
                .menu
                .as_ref()
                .and_then(|opened| opened.pos)
                .map(|(x, y, z)| [x, y, z]),
        );
        load_player(menu, &host.hub.guests[gi]);
        let result = op(menu, &env);
        drop(parts);
        self.guest_bundle_sounds(gi, std::mem::take(&mut menu.player_sounds));
        if let Some(maps) = maps {
            self.register_made_maps(maps);
        }
        // A taken anvil result waits for AnvilCraftEvent.Pre: what the click
        // dropped waits with it (lan_station_op throws it or refuses the take).
        let dropped = if menu.anvil_craft.is_some() {
            Vec::new()
        } else {
            std::mem::take(&mut menu.dropped)
        };
        let equipped = std::mem::take(&mut menu.equipped);
        let brewed = std::mem::take(&mut menu.brewed);
        let crafted = std::mem::take(&mut menu.crafted);
        let item_crafted = std::mem::take(&mut menu.item_crafted);
        let crafted_counts = std::mem::take(&mut menu.crafted_counts);
        menu.drops.clear();
        let host = self.lan.as_mut()?;
        host.hub
            .guests
            .update(gi, |guest| store_player(menu, guest));
        let guest = &host.hub.guests[gi];
        let conn = guest.conn;
        // `ItemStack.onCraftedBy`'s `Stats.ITEM_CRAFTED` count, before the
        // recipe the take used.
        if !crafted_counts.is_empty() {
            self.as_guest(conn, |g| {
                for (item, count) in crafted_counts {
                    g.award_stat("crafted", &item, count);
                }
            });
        }
        if !brewed.is_empty() {
            self.as_guest(conn, |g| {
                for stack in &brewed {
                    g.trigger_brewed_potion(stack);
                }
            });
        }
        if !crafted.is_empty() {
            self.as_guest(conn, |g| {
                for (recipe, ingredients) in &crafted {
                    g.trigger_recipe_crafted(recipe, ingredients);
                }
            });
        }
        if !item_crafted.is_empty() {
            self.as_guest(conn, |g| {
                for (_, stack, _) in &item_crafted {
                    // ItemStack.onCraftedBy awards Stats.ITEM_CRAFTED.
                    g.award_stat("crafted", &stack.item, stack.count as i32);
                }
            });
        }
        if !equipped.is_empty() {
            self.as_guest(conn, |g| {
                for (slot, stack) in equipped {
                    g.player_equipped(slot, stack.as_ref());
                }
            });
        }
        self.drop_guest_items(gi, dropped);
        Some(result)
    }

    /// `InventoryMenu.slotsChanged` for guest `uuid`'s crafting grid written outside a click
    /// (a mod's menu slot write, at the root commit): the result made again from the grid, as
    /// a click has it, kept by the guest and their menu. The
    /// client hears it with the tick's inventory sync.
    pub(super) fn guest_craft_result(&mut self, conn: u32) {
        let Some(g) = self
            .lan
            .as_ref()
            .and_then(|host| host.hub.guest(conn).filter(|g| g.placed))
        else {
            return;
        };
        let uuid = g.uuid;
        let mut result = self
            .recipes()
            .find(&self.loop_context, &g.craft_grid, 2)
            .map(|(_, stack)| stack);
        let Some(host) = self.lan.as_mut() else {
            return;
        };
        if host
            .hub
            .update_guest(conn, |g| g.craft_result.clone_from(&result))
            .is_none()
        {
            return;
        }
        let menu = host
            .menus
            .get_mut(&conn)
            .and_then(|p| p.crafting.inventory.take());
        if let Some(mut menu) = menu {
            menu.set_slot(&self.menu_env(), 0, result);
            if let Some(play) = self.lan.as_mut().and_then(|host| host.menus.get_mut(&conn)) {
                *play.crafting.inventory = Some(menu);
            }
        }
    }

    /// `BundleItem.playInsertSound` and the others on the server: the guest's
    /// `Player.playSound`, at them in the players' source, for everyone near
    /// but them (their client played its own prediction). The pitch draws on
    /// the level's random as the click plays each one.
    pub(super) fn guest_bundle_sounds(&mut self, gi: usize, sounds: Vec<&'static str>) {
        if sounds.is_empty() {
            return;
        }
        let Some(host) = self.lan.as_ref() else {
            return;
        };
        let g = &host.hub.guests[gi];
        let players = myriade_data::sound_source::SOUND_SOURCES
            .iter()
            .position(|s| *s == "player")
            .unwrap_or(0) as i32;
        for name in sounds {
            let (volume, pitch) = myriade_engine::bundle::sound_volume_pitch(name, || {
                horizon_core::random::RandomSource::next_float(&mut self.world.random)
            });
            host.hub
                .sound(name, g.position, volume, pitch, players, Some(g.conn));
        }
    }

    /// A click on the guest's inventory menu (window 0): `InventoryMenu`,
    /// crafting grid included. False for other windows.
    pub(super) fn lan_inventory_click(&mut self, conn: u32, p: &play::ContainerClick) -> bool {
        if p.container != 0 {
            return false;
        }
        let Some(host) = self.lan.as_mut() else {
            return true;
        };
        let Some(gi) = host.hub.guests.iter().position(|g| g.conn == conn) else {
            return true;
        };
        if host.hub.guests[gi].spectator() {
            return true;
        }
        let crafting = host.menus.entry(conn).or_default();
        let mut menu = crafting
            .crafting
            .inventory
            .take()
            .unwrap_or_else(|| Box::new(Menu::new(Kind::Inventory)));
        let grid = host.hub.guests[gi].craft_grid.clone();
        let Some(changed) = self.guest_menu_op(gi, &mut menu, |menu, env| {
            for (k, s) in grid.into_iter().enumerate() {
                menu.set_slot(env, 1 + k, s);
            }
            let before: Vec<_> = (0..menu.slot_count())
                .map(|s| menu.slot(s).cloned())
                .collect();
            let _ = menu.click(env, i32::from(p.slot), i32::from(p.button), input(p.input));
            (0..menu.slot_count())
                .filter(|&s| menu.slot(s) != before[s].as_ref())
                .collect::<Vec<_>>()
        }) else {
            return true;
        };
        if self.lan.is_none() {
            return true;
        }
        let env = self.menu_env();
        let result = menu.slot(0).cloned();
        menu.set_slot(&env, 0, result);
        let Some(host) = self.lan.as_mut() else {
            return true;
        };
        host.hub.guests.update(gi, |g| {
            for k in 0..4 {
                g.craft_grid[k] = menu.slot(1 + k).cloned();
            }
            g.craft_result = menu.slot(0).cloned();
        });
        *host.menus.entry(conn).or_default().crafting.inventory = Some(menu);
        host.hub.inventory_clicked(conn, p, &self.world, &changed);
        true
    }

    /// A click on the guest's open furnace, blast furnace, smoker or brewing stand:
    /// the host's `AbstractFurnaceMenu` (fuel slot, take-only result, quick moves by
    /// recipe and fuel), `BrewingStandMenu` (one bottle per slot, ingredient and
    /// blaze powder slots) or a content mod's menu type (`Kind::Mod`) over the
    /// block entity's slots. False for other windows.
    pub(super) fn lan_machine_click(&mut self, conn: u32, p: &play::ContainerClick) -> bool {
        let Some(host) = self.lan.as_mut() else {
            return false;
        };
        let Some(gi) = host.hub.guests.iter().position(|g| g.conn == conn) else {
            return false;
        };
        let Some((pos, hidden, menu_id)) = host.hub.guests[gi]
            .menu
            .as_ref()
            .filter(|m| p.container != 0 && m.window == p.container)
            .and_then(|m| match m.slots {
                Slots::Block(pos) => Some((
                    pos,
                    m.hidden.clone(),
                    host.hub.registry_name("minecraft:menu", m.kind)?.to_owned(),
                )),
                _ => None,
            })
        else {
            return false;
        };
        let Some((kind, items)) = self
            .world
            .block_entities
            .get(&pos)
            .and_then(machine_menu)
        else {
            return false;
        };
        let furnace = matches!(kind, Kind::Furnace(_));
        // CrafterBlockEntity.containerData: the disabled slots, then triggered.
        let crafter_data = match self.world.block_entities.get(&pos) {
            Some(BlockEntity::Crafter(c)) => Some((c.disabled, c.is_triggered())),
            _ => None,
        };
        if host.hub.guests[gi].spectator() {
            // handleContainerClick: nothing changes, and sendAllDataToRemote
            // takes back what the client predicted.
            host.hub.resend_menu(conn, p.container, &self.world);
            return true;
        }
        let mut menu = host
            .menus
            .entry(conn)
            .or_default()
            .crafting
            .machine
            .take()
            .filter(|(window, m)| *window == p.container && m.kind() == kind)
            .map_or_else(|| Box::new(Menu::new(kind)), |(_, m)| m);
        menu.set_registry_id(Some(&menu_id));
        for &slot in &hidden {
            menu.hide_slot(slot);
        }
        let Some(mut changed) = self.guest_menu_op(gi, &mut menu, |menu, env| {
            for (k, s) in items.iter().enumerate() {
                menu.set_slot(env, k, s.clone());
            }
            // After the slots: setting a disabled slot (CrafterBlockEntity.setItem) enables it.
            if let Some((disabled, triggered)) = crafter_data {
                for (k, d) in disabled.iter().enumerate() {
                    menu.data[k] = i32::from(*d);
                }
                menu.data[9] = i32::from(triggered);
            }
            let before: Vec<_> = (0..menu.slot_count())
                .map(|s| menu.slot(s).cloned())
                .collect();
            let _ = menu.click(env, i32::from(p.slot), i32::from(p.button), input(p.input));
            (0..menu.slot_count())
                .filter(|&s| menu.slot(s) != before[s].as_ref())
                .collect::<Vec<_>>()
        }) else {
            return true;
        };
        match self.world.block_entities.get_mut(&pos) {
            Some(BlockEntity::Furnace(f)) => {
                for k in 0..3 {
                    f.items[k] = menu.slot(k).cloned();
                }
                // AbstractFurnaceBlockEntity.setItem: only a new input restarts cooking.
                f.input_clicked(&self.loop_context, world_recipes!(self), items[0].clone());
            }
            // BrewingStandBlockEntity.serverTick notices a new ingredient itself.
            Some(BlockEntity::Brewing(b)) => {
                for k in 0..5 {
                    b.items[k] = menu.slot(k).cloned();
                }
            }
            // CrafterBlockEntity.setItem: filling a disabled slot enables it.
            Some(BlockEntity::Crafter(c)) => {
                for k in 0..9 {
                    c.items[k] = menu.slot(k).cloned();
                    c.disabled[k] = menu.data[k] == 1;
                }
            }
            _ => {}
        }
        // FurnaceResultSlot.checkTakeAchievements, each as the click made it.
        let Some(at) = self.lan.as_ref().map(|host| host.hub.guests[gi].position) else {
            return true;
        };
        menu.item_smelted.clear();
        // The guest took from the output.
        let count = |s: Option<&ItemStack>| s.map_or(0, |s| s.count);
        if furnace && count(menu.slot(2)) < count(items[2].as_ref()) {
            self.award_furnace_recipes(pos, at);
        }
        // Slot.setChanged on a changed container slot: BlockEntity.setChanged,
        // so the chunk is saved and comparators read the block again.
        if changed.iter().any(|&s| s < items.len()) {
            self.container_changed(pos);
        }
        if let Some(host) = self.lan.as_mut() {
            *host.menus.entry(conn).or_default().crafting.machine = Some((p.container, menu));
            host.hub.inventory_clicked(conn, p, &self.world, &changed);
        }
        true
    }

    /// `ServerGamePacketListenerImpl.handleContainerSlotStateChanged`: a
    /// guest's crafter slot toggle (`CrafterBlockEntity.setSlotState`) in
    /// their open crafter window.
    pub(super) fn lan_crafter_slot_state(
        &mut self,
        conn: u32,
        window: i32,
        slot: i32,
        enabled: bool,
    ) {
        let Some(host) = self.lan.as_ref() else {
            return;
        };
        let Some(pos) = host
            .hub
            .guest(conn)
            .filter(|g| !g.spectator())
            .and_then(|g| g.menu.as_ref())
            .filter(|m| window != 0 && m.window == window)
            .and_then(|m| match m.slots {
                Slots::Block(pos) => Some(pos),
                _ => None,
            })
        else {
            return;
        };
        if let (Some(BlockEntity::Crafter(c)), Ok(slot)) = (
            self.world.block_entities.get_mut(&pos),
            usize::try_from(slot),
        ) {
            c.set_slot_state(slot, enabled);
            self.world.modified.insert((pos.0 >> 4, pos.2 >> 4));
        }
    }

    /// ServerGamePacketListenerImpl.handlePlaceRecipe: the guest's recipe
    /// book places a known recipe in their inventory grid, crafting table or furnace
    /// (`ServerPlaceRecipe`), and answers with a ghost recipe when the items
    /// are missing.
    pub(super) fn lan_place_recipe(
        &mut self,
        conn: u32,
        p: &horizon_protocol::packets::recipes::PlaceRecipe,
    ) {
        let Some(host) = self.lan.as_ref() else {
            return;
        };
        let Some(gi) = host.hub.guests.iter().position(|g| g.conn == conn) else {
            return;
        };
        let g = &host.hub.guests[gi];
        if g.spectator() {
            return;
        }
        // The server menu uses the world's active data-pack tables, the same
        // tables used by the guest menu below. The base resources can have a
        // different display registry or placement for an overridden recipe.
        let Some(id) = world_stations!(self)
            .recipe_index
            .recipe_of_display(p.recipe)
            .map(str::to_owned)
        else {
            return;
        };
        // An unknown or impossible recipe is refused outright.
        if !g.recipe_book.knows(&id)
            || world_recipes!(self)
                .placement(&id)
                .is_none_or(|placement| placement.impossible())
        {
            return;
        }
        let creative = g.creative();
        // AbstractFurnaceMenu.handlePlacement: the current furnace's slots,
        // rather than the separate crafting-table menu, own cooking inputs.
        let furnace = self.lan.as_ref().unwrap().hub.guests[gi]
            .menu
            .as_ref()
            .filter(|m| p.container != 0 && m.window == p.container)
            .and_then(|m| match m.slots {
                Slots::Block(pos) => Some(pos),
                _ => None,
            })
            .and_then(|pos| match self.world.block_entities.get(&pos) {
                Some(BlockEntity::Furnace(f)) => {
                    let recipe = match f.kind.recipe_type() {
                        "minecraft:blasting" => "blasting",
                        "minecraft:smoking" => "smoking",
                        _ => "smelting",
                    };
                    Some((pos, Kind::Furnace(recipe), f.items.clone()))
                }
                _ => None,
            });
        let placed = if let Some((pos, kind, items)) = furnace {
            // Opening need not have created the click/drag cache yet.
            let mut menu = self
                .lan
                .as_mut()
                .unwrap()
                .menus
                .entry(conn)
                .or_default()
                .crafting
                .machine
                .take()
                .filter(|(window, m)| *window == p.container && m.kind() == kind)
                .map_or_else(|| Box::new(Menu::new(kind)), |(_, m)| m);
            let placed = self.guest_menu_op(gi, &mut menu, |menu, env| {
                for (k, item) in items.iter().enumerate() {
                    menu.set_slot(env, k, item.clone());
                }
                menu.place_recipe(env, &id, p.use_max, creative, None)
            });
            let changed = (0..3).any(|k| menu.slot(k) != items[k].as_ref());
            if let Some(BlockEntity::Furnace(f)) = self.world.block_entities.get_mut(&pos) {
                for k in 0..3 {
                    f.items[k] = menu.slot(k).cloned();
                }
                f.input_clicked(&self.loop_context, world_recipes!(self), items[0].clone());
            }
            if changed {
                self.container_changed(pos);
            }
            *self
                .lan
                .as_mut()
                .unwrap()
                .menus
                .entry(conn)
                .or_default()
                .crafting
                .machine = Some((p.container, menu));
            // Hub::sync_containers broadcasts the actual changed slots.
            placed
        } else {
            self.lan_apply_recipe(gi, p.container, |menu, env| {
                menu.place_recipe(env, &id, p.use_max, creative, None)
            })
        };
        if placed == Some(PostPlace::Ghost)
            && let Some(host) = self.lan.as_mut()
        {
            host.hub.send_ghost_recipe(conn, p.container, &id);
        }
    }

    pub(super) fn guest_recipe_transfer(&mut self, conn: u32, data: &[u8]) {
        use horizon_protocol::buf::Writer;
        let Some(host) = self.lan.as_ref() else {
            return;
        };
        let Some(gi) = host.hub.guests.iter().position(|g| g.conn == conn) else {
            return;
        };
        let g = &host.hub.guests[gi];
        if g.brand != "horizon" || g.spectator() {
            return;
        }
        let Ok((container, id, all)) = myriade_engine::lan::recipes::decode_transfer_request(data)
        else {
            return;
        };
        let blocked = container == 0 && (g.menu.is_some() || g.creative());
        let result = if blocked {
            Err(TransferError::Unavailable)
        } else {
            self.lan_apply_recipe(gi, container, |menu, env| {
                menu.transfer_recipe(env, &id, all)
            })
            .unwrap_or(Err(TransferError::Unavailable))
        };
        let mut reply = Writer::new();
        reply.varint(container);
        reply.u8(recipes::result_byte(recipes::transfer_result(result)));
        if let Some(host) = self.lan.as_ref() {
            host.hub.guests[gi].send(&C::CustomPayload(
                horizon_protocol::packets::configuration::CustomPayload {
                    channel: recipes::TRANSFER_CHANNEL.into(),
                    data: reply.data,
                },
            ));
        }
    }

    /// A viewer's check of a transfer (`horizon:recipe_check`): the matching
    /// and placement `guest_recipe_transfer` makes, the mods' recipe sources
    /// counted, on a copy of the guest's menu (`lan_check_recipe`). Nothing is
    /// written back: no slot, block entity or `container_changed`, and no
    /// packet but the reply. Ten at once, then one a second
    /// (`GuestPlay::recipe_checks`); more go unanswered.
    pub(super) fn guest_recipe_check(&mut self, conn: u32, data: &[u8]) {
        use horizon_protocol::buf::{Reader, Writer};
        let Some(host) = self.lan.as_mut() else {
            return;
        };
        let Some(gi) = host.hub.guests.iter().position(|g| g.conn == conn) else {
            return;
        };
        let g = &host.hub.guests[gi];
        if g.brand != "horizon" || g.spectator() {
            return;
        }
        let unavailable = g.menu.is_some() || g.creative();
        let mut reader = Reader::new(data);
        let (Ok(container), Ok(tag), Ok(id)) =
            (reader.varint(), reader.varint(), reader.string(512))
        else {
            return;
        };
        if reader.remaining() != 0 {
            return;
        }
        let play = host.play.entry(conn).or_default();
        if play.recipe_checks >= RECIPE_CHECK_LIMIT {
            return;
        }
        play.recipe_checks += RECIPE_CHECK_COST;
        let check = if container == 0 && unavailable {
            None
        } else {
            self.lan_check_recipe(gi, container, &id)
        }
        .unwrap_or(TransferCheck {
            result: Err(TransferError::Unavailable),
            missing: Vec::new(),
        });
        let mut reply = Writer::new();
        reply.varint(container);
        reply.varint(tag);
        reply.u8(recipes::result_byte(recipes::transfer_result(check.result)));
        if check.result == Err(TransferError::Missing) {
            let cells: Vec<u8> = (check.missing.iter())
                .filter_map(|&c| u8::try_from(c).ok())
                .collect();
            reply.varint(cells.len() as i32);
            for c in cells {
                reply.u8(c);
            }
        }
        if let Some(host) = self.lan.as_ref() {
            host.hub.guests[gi].send(&C::CustomPayload(
                horizon_protocol::packets::configuration::CustomPayload {
                    channel: recipes::CHECK_CHANNEL.into(),
                    data: reply.data,
                },
            ));
        }
    }

    /// `Menu::check_transfer` of recipe `id` on a copy of guest `gi`'s window
    /// `container`, built as `lan_apply_recipe` builds it: the inventory's
    /// grid, a crafting table's, or a mod crafting menu's over its block
    /// entity with its recipe sources. None when that window is none of them
    /// (the window must be the one the guest has open, as for a transfer).
    fn lan_check_recipe(&mut self, gi: usize, container: i32, id: &str) -> Option<TransferCheck> {
        let conn = self.lan.as_ref()?.hub.guests[gi].conn;
        let menu = if container == 0 {
            let host = self.lan.as_ref()?;
            let env = self.menu_env();
            let mut menu = Menu::new(Kind::Inventory);
            load_player(&mut menu, &host.hub.guests[gi]);
            for (k, s) in host.hub.guests[gi].craft_grid.iter().enumerate() {
                menu.set_slot(&env, 1 + k, s.clone());
            }
            menu
        } else {
            let host = self.lan.as_ref()?;
            let (_, _, table, _) = (host.menus.get(&conn)?.crafting.table.as_ref())
                .filter(|(window, ..)| *window == container)?;
            let mut menu = Menu::clone(table);
            load_player(&mut menu, &host.hub.guests[gi]);
            menu
        };
        let host = self.lan.as_ref()?;
        let env = Env {
            player: Some(host.hub.guests[gi].uuid),
            player_entity_id: Some(host.hub.guests[gi].entity_id),
            loop_context: &self.loop_context,
            limited_crafting: host.hub.limited_crafting,
            can_equip: None,
            known_recipes: Some(&host.hub.guests[gi].recipe_book),
            ..self.menu_env()
        };
        Some(menu.check_transfer(&env, id))
    }

    fn lan_apply_recipe<R>(
        &mut self,
        gi: usize,
        container: i32,
        apply: impl FnOnce(&mut Menu, &Env) -> R,
    ) -> Option<R> {
        let conn = self.lan.as_ref()?.hub.guests[gi].conn;
        let host = self.lan.as_mut()?;
        let placed = if container == 0 {
            let crafting = host.menus.entry(conn).or_default();
            let mut menu = crafting
                .crafting
                .inventory
                .take()
                .unwrap_or_else(|| Box::new(Menu::new(Kind::Inventory)));
            let grid = host.hub.guests[gi].craft_grid.clone();
            let placed = self.guest_menu_op(gi, &mut menu, |menu, env| {
                for (k, s) in grid.into_iter().enumerate() {
                    menu.set_slot(env, 1 + k, s);
                }
                apply(menu, env)
            })?;
            let host = self.lan.as_mut()?;
            host.hub.guests.update(gi, |g| {
                for k in 0..4 {
                    g.craft_grid[k] = menu.slot(1 + k).cloned();
                }
                g.craft_result = menu.slot(0).cloned();
            });
            *host.menus.entry(conn).or_default().crafting.inventory = Some(menu);
            host.hub.send_inventory(gi);
            placed
        } else {
            let (window, state, mut menu, pos) = host
                .menus
                .get_mut(&conn)
                .and_then(|play| play.crafting.table.take_if(|(w, ..)| *w == container))?;
            let placed = self.guest_menu_op(gi, &mut menu, apply)?;
            let host = self.lan.as_mut()?;
            *host.menus.entry(conn).or_default().crafting.table = Some((window, state, menu, pos));
            self.lan_send_table_content(conn);
            placed
        };
        Some(placed)
    }

    /// The guest opened a crafting table (`CraftingTableBlock.getMenuProvider`).
    pub(super) fn lan_open_crafting(&mut self, conn: u32, pos: [i32; 3]) {
        let _ = self.lan_open_crafting_with(conn, pos, Vec::new());
    }

    /// `lan_open_crafting` with the grid starting as `grid` (row by row; a mod's table with no
    /// block, game/modding/native_stations.rs): its window, None when a mod stopped it or no
    /// window opened.
    pub(super) fn lan_open_crafting_with(
        &mut self,
        conn: u32,
        pos: [i32; 3],
        grid: Vec<Option<ItemStack>>,
    ) -> Option<i32> {
        if !self.guest_may_open(conn, "minecraft:crafting", Some(pos)) {
            return None;
        }
        // ServerPlayer.openMenu closes the menu that was open.
        self.guest_close_all(conn);
        let host = self.lan.as_mut()?;
        let title = horizon_protocol::text::translatable("container.crafting", &[]);
        let window = host.hub.open_window(conn, "minecraft:crafting", title)?;
        let mut menu = Box::new(Menu::new(Kind::Crafting));
        menu.set_registry_id(Some("minecraft:crafting"));
        if !grid.is_empty()
            && let Some(gi) = host.hub.guests.iter().position(|g| g.conn == conn)
        {
            self.guest_menu_op(gi, &mut menu, |menu, env| {
                for (slot, stack) in (1..10).zip(grid) {
                    menu.set_slot(env, slot, stack);
                }
            });
            self.lan.as_ref()?;
            let result = menu.slot(0).cloned();
            let env = self.menu_env();
            menu.set_slot(&env, 0, result);
        }
        let host = self.lan.as_mut()?;
        *host.menus.entry(conn).or_default().crafting.table = Some((window, 0, menu, pos));
        self.lan_send_table_content(conn);
        Some(window)
    }

    /// A click on the guest's crafting table window; false for other windows.
    pub(super) fn lan_crafting_click(&mut self, conn: u32, p: &play::ContainerClick) -> bool {
        let Some(host) = self.lan.as_mut() else {
            return false;
        };
        let Some(play) = host.menus.get_mut(&conn) else {
            return false;
        };
        let Some((window, state, mut menu, pos)) = play
            .crafting
            .table
            .take_if(|(window, ..)| *window == p.container)
        else {
            return false;
        };
        // handleContainerClick: broadcastFullState only for a click on an old
        // state, else only what the client got wrong. The whole menu after every
        // click would undo the clicks it has predicted since.
        let mut agreed = false;
        // A table that is no longer valid takes no clicks (handleContainerClick).
        let gi = host.hub.guests.iter().position(|g| g.conn == conn);
        let Some(host) = self.lan.as_ref() else {
            return false;
        };
        if let Some(gi) = gi
            && !host.hub.guests[gi].spectator()
            && self.lan_table_valid(&host.hub.guests[gi], pos)
        {
            let Some(mut changed) = self.guest_menu_op(gi, &mut menu, |menu, env| {
                let before: Vec<_> = (0..menu.slot_count())
                    .map(|s| menu.slot(s).cloned())
                    .collect();
                let _ = menu.click(env, i32::from(p.slot), i32::from(p.button), input(p.input));
                (0..menu.slot_count())
                    .filter(|&s| menu.slot(s) != before[s].as_ref())
                    .collect::<Vec<_>>()
            }) else {
                return true;
            };
            if self.lan.is_none() {
                return true;
            }
            let result = menu.slot(0).cloned();
            let env = self.menu_env();
            menu.set_slot(&env, 0, result);
            let Some(host) = self.lan.as_mut() else {
                return true;
            };
            agreed = p.state == state
                && host.hub.click_agreed(
                    gi,
                    p,
                    menu.slot_count(),
                    &|s| menu.slot(s).cloned(),
                    &changed,
                );
            if agreed {
                // The inventory menu's sync isn't to send what the click settled.
                host.hub.guests.update(gi, |g| {
                    for &s in changed.iter().filter(|&&s| s >= 10) {
                        g.menu_sent[s - 1] = menu.slot(s).cloned();
                    }
                });
            }
        }
        let Some(host) = self.lan.as_mut() else {
            return true;
        };
        *host.menus.entry(conn).or_default().crafting.table = Some((window, state, menu, pos));
        if !agreed {
            self.lan_send_table_content(conn);
        }
        true
    }

    /// `CraftingMenu.stillValid`: the block is still a crafting table and the
    /// guest is within reach of it.
    pub(super) fn lan_table_valid(&self, g: &Guest, pos: [i32; 3]) -> bool {
        let name = &self
            .res
            .facts()
            .get(self.world.block(pos[0], pos[1], pos[2]))
            .name;
        &**name == "minecraft:crafting_table" && g.within_block_range(pos, 4.0)
    }

    /// `ServerPlayer.tick`'s `stillValid`: a guest's crafting table closes
    /// once it is gone or out of reach, giving its grid back.
    pub(super) fn lan_tables_tick(&mut self) {
        let Some(host) = self.lan.as_ref() else {
            return;
        };
        let invalid: Vec<(u32, i32)> = host
            .menus
            .iter()
            .filter_map(|(c, p)| p.crafting.table.as_ref().map(|t| (*c, t.0, t.3)))
            .filter(|&(c, _, pos)| {
                host.hub.guests.iter().find(|g| g.conn == c).is_some_and(|g| {
                    host.hub.here(g) && !self.lan_table_valid(g, pos)
                })
            })
            .map(|(c, window, _)| (c, window))
            .collect();
        for (conn, window) in invalid {
            self.lan_crafting_closed(conn, Some(window));
            let Some(host) = self.lan.as_mut() else {
                return;
            };
            host.hub.send_to(
                conn,
                &C::ContainerClose(play::ContainerClose { container: window }),
            );
            if let Some(gi) = host.hub.guests.iter().position(|g| g.conn == conn) {
                self.guest_carried_back(gi);
            }
        }
    }

    /// The whole crafting table menu for the client (`sendAllDataToRemote`).
    fn lan_send_table_content(&mut self, conn: u32) {
        let Some(host) = self.lan.as_mut() else {
            return;
        };
        let Some(gi) = host.hub.guests.iter().position(|g| g.conn == conn) else {
            return;
        };
        let Some((window, state, menu, _)) = host
            .menus
            .get_mut(&conn)
            .and_then(|p| p.crafting.table.as_mut())
        else {
            return;
        };
        *state = (*state + 1) & 0x7FFF;
        let (window, state) = (*window, *state);
        let mut items: Vec<Option<ItemStack>> = (0..10).map(|s| menu.slot(s).cloned()).collect();
        let g = &host.hub.guests[gi];
        items.extend((9..36).chain(0..9).map(|i| g.inventory[i].clone()));
        let carried = g.carried.clone();
        let items = items.iter().map(|s| host.hub.stack(s)).collect();
        let carried = host.hub.stack(&carried);
        host.hub.send_to(
            conn,
            &C::ContainerSetContent(play::ContainerSetContent {
                container: window,
                state,
                items,
                carried,
            }),
        );
        host.hub.guests.update(gi, |g| g.inventory_told());
    }

    /// `AbstractContainerMenu.removed` of a crafting menu: the closed window
    /// (`window`; None closes the table whichever it is, and 0 the inventory
    /// menu) gives its grid back to the inventory, what doesn't fit is
    /// dropped. False when no such window is open.
    /// The open crafting table's own slots (result and grid) before the
    /// player's inventory, if the guest has one open.
    pub(super) fn lan_crafting_table_own_slots(&self, conn: u32) -> Option<usize> {
        let (_, _, menu, _) = self
            .lan
            .as_ref()?
            .menus
            .get(&conn)?
            .crafting
            .table
            .as_ref()?;
        Some(menu.slot_count() - 36)
    }

    pub(super) fn lan_crafting_closed(&mut self, conn: u32, window: Option<i32>) -> bool {
        let closed = self.lan_crafting_removed(conn, window, MenuRemoval::Live);
        if closed && window != Some(0) {
            if let Some(gi) = self
                .lan
                .as_ref()
                .and_then(|host| host.hub.guests.iter().position(|g| g.conn == conn))
            {
                self.guest_carried_back(gi);
            }
        }
        closed
    }

    /// Remove only input slots; disconnected players drop their inputs.
    pub(super) fn lan_crafting_removed(
        &mut self,
        conn: u32,
        window: Option<i32>,
        removal: MenuRemoval,
    ) -> bool {
        let Some(host) = self.lan.as_mut() else {
            return false;
        };
        let Some(gi) = host.hub.guests.iter().position(|g| g.conn == conn) else {
            return false;
        };
        let crafting = &mut host.menus.entry(conn).or_default().crafting;
        let grid: Vec<ItemStack> = if window == Some(0) {
            *crafting.inventory = None;
            host.hub.guests.update(gi, |g| {
                g.craft_result = None;
                g.craft_grid.iter_mut().filter_map(Option::take).collect()
            })
        } else {
            match crafting
                .table
                .take_if(|(w, ..)| window.is_none_or(|window| *w == window))
            {
                Some((_, _, menu, _)) => (1..10).filter_map(|s| menu.slot(s).cloned()).collect(),
                None => return false,
            }
        };
        if !grid.is_empty() {
            match removal {
                MenuRemoval::Live => self.lan_place_back(gi, grid),
                MenuRemoval::Disconnected => self.drop_guest_items(gi, grid),
            }
        }
        true
    }

    /// `Inventory.placeItemBackInInventory` of `stacks` for guest `gi`, what
    /// doesn't fit dropped (`clearContainer`).
    pub(super) fn lan_place_back(&mut self, gi: usize, stacks: Vec<ItemStack>) {
        let mut menu = Menu::new(Kind::Inventory);
        self.guest_menu_op(gi, &mut menu, |menu, env| place_back(menu, env, stacks));
    }

    /// A guest left: what their crafting grids and workstation held goes back into their
    /// inventory before it is saved (dropped where they stood if full).
    pub(super) fn lan_crafting_left(&mut self, g: &mut Guest) {
        // The guest's play state stays: their statistics are awarded and saved
        // after this (`HostEvent::Left`), which removes it.
        let Some(host) = self.lan.as_mut() else {
            return;
        };
        let play = host.menus.get_mut(&g.conn);
        let mut grid: Vec<ItemStack> = g.craft_grid.iter_mut().filter_map(Option::take).collect();
        g.craft_result = None;
        if let Some(play) = play {
            if let Some((_, _, menu, _)) = play.crafting.table.take() {
                grid.extend((1..10).filter_map(|s| menu.slot(s).cloned()));
            }
            // A workstation's inputs too.
            if let Some(station) = play.station.take() {
                grid.extend(super::lan_stations::station_inputs(&station));
            }
        }
        if grid.is_empty() {
            return;
        }
        let mut menu = Menu::new(Kind::Inventory);
        load_player(&mut menu, g);
        place_back(&mut menu, &self.menu_env(), grid);
        let dropped = std::mem::take(&mut menu.dropped);
        store_player(&mut menu, g);
        for stack in dropped {
            self.spawn_item(stack, g.position, [0.0, 0.1, 0.0], 40);
        }
    }
}

#[cfg(test)]
mod machine_recipe_placement_tests {
    use super::*;
    use myriade_engine::lan::{Joined, NetConfig, PlayerSender, guest::Guest, hub::HubConfig};
    use horizon_core::position::ChunkPos;
    use horizon_protocol::packets::{
        configuration::ClientInformation,
        play::{Serverbound, UseItemOn},
        recipes::PlaceRecipe,
    };
    use std::sync::{Arc, atomic::AtomicI32, mpsc};

    #[test]
    #[ignore = "daily"]
    fn h234_registered_place_recipe_reaches_actual_furnace_and_preserves_controls() {
        crate::game::native_entity_refs::tests::with_game(|game| {
            game.build_registries();
            game.set_allow_commands(true);
            game.world.chunks.insert(
                (0, 0),
                std::sync::Arc::new(myriade_engine::chunk::Chunk::empty(0, 0)),
            );
            game.block_ticks
                .add_empty_chunk(ChunkPos::new(0, 0))
                .unwrap();
            assert!(game.run_command("fill 0 199 0 15 199 15 stone"));
            assert!(game.world.loaded(8, 8));
            game.lan_host(
                crate::game::lan::LanSettings {
                    port: 0,
                    guest_commands: true,
                    force_game_mode: false,
                },
                NetConfig {
                    announce: false,
                    ..NetConfig::loopback(0, "machine-place-test".into(), 2)
                },
                HubConfig {
                    default_game_mode: 0,
                    guest_commands: true,
                    force_game_mode: false,
                    hardcore: false,
                    view_distance: 2,
                    simulation_distance: 2,
                    dimension: "minecraft:overworld".into(),
                    allow_flight: false,
                    seed: 0,
                    max_players: 2,
                    levels: Default::default(),
                    level_seeds: Default::default(),
                },
            )
            .unwrap();
            let (tx, _rx) = mpsc::channel();
            let mut guest = Guest::new(
                Joined {
                    conn: 2,
                    uuid: 234,
                    name: "RecipeGuest".into(),
                    properties: Vec::new(),
                    info: ClientInformation::new(2),
                    brand: "test".into(),
                    sender: PlayerSender::new(tx, Arc::default()),
                    latency: Arc::new(AtomicI32::new(0)),
                    address: "127.0.0.1:25565".parse().unwrap(),
                    expected: false,
                },
                2,
                [8.5, 200.0, 8.5],
                2,
            );
            guest.dimension = "minecraft:overworld".into();
            guest.placed = true;
            guest.loaded = true;
            guest.selected = 1;
            guest.permission = 2;
            game.lan.as_mut().unwrap().hub.guests.push(guest);
            let clock = game.world.game_time;
            let planks = game
                .stations()
                .recipe_index
                .first_display("minecraft:oak_planks")
                .unwrap();
            let charcoal = game
                .stations()
                .recipe_index
                .first_display("minecraft:charcoal")
                .unwrap();
            let command = |game: &mut Game<'_>, text: &str| {
                assert!(game.as_guest(2, |g| g.run_command(text)).unwrap(), "{text}");
            };
            let place = |game: &mut Game<'_>, container, recipe| {
                game.lan_guest_packet(
                    2,
                    Serverbound::PlaceRecipe(PlaceRecipe {
                        container,
                        recipe,
                        use_max: false,
                    }),
                );
            };
            let open = |game: &mut Game<'_>, pos| {
                game.lan_guest_packet(
                    2,
                    Serverbound::UseItemOn(UseItemOn {
                        hand: 0,
                        pos,
                        face: 1,
                        cursor: [0.5, 1.0, 0.5],
                        inside: false,
                        world_border: false,
                        sequence: 234,
                    }),
                );
                assert!(
                    game.lan
                        .as_ref()
                        .unwrap()
                        .hub
                        .guest(2)
                        .unwrap()
                        .ack_up_to
                        .get()
                        >= 234
                );
                game.lan
                    .as_ref()
                    .unwrap()
                    .menus
                    .get(&2)
                    .and_then(|p| p.crafting.table.as_ref())
                    .map(|(window, ..)| *window)
                    .or_else(|| {
                        game.lan
                            .as_ref()
                            .unwrap()
                            .hub
                            .guest(2)
                            .unwrap()
                            .menu
                            .as_ref()
                            .map(|m| m.window)
                    })
                    .expect("actual table or machine window opened")
            };
            command(game, "recipe give @s minecraft:oak_planks");
            command(game, "recipe give @s minecraft:charcoal");
            assert!(
                game.lan
                    .as_ref()
                    .unwrap()
                    .hub
                    .guest(2)
                    .unwrap()
                    .recipe_book
                    .knows("minecraft:charcoal")
            );
            command(game, "item replace entity @s hotbar.0 with oak_log 3");
            place(game, 0, planks);
            assert_eq!(
                game.lan.as_ref().unwrap().hub.guest(2).unwrap().craft_grid[0],
                Some(ItemStack::new("minecraft:oak_log", 1))
            );
            assert_eq!(
                game.lan
                    .as_ref()
                    .unwrap()
                    .hub
                    .guest(2)
                    .unwrap()
                    .craft_result,
                Some(ItemStack::new("minecraft:oak_planks", 4))
            );
            assert_eq!(
                game.lan.as_ref().unwrap().hub.guest(2).unwrap().inventory[0]
                    .as_ref()
                    .unwrap()
                    .count,
                2
            );
            game.lan_crafting_closed(2, Some(0));
            assert!(game.run_command("setblock 10 200 8 crafting_table"));
            let window = open(game, [10, 200, 8]);
            place(game, window, planks);
            let (_, _, table, _) = game.lan.as_ref().unwrap().menus[&2]
                .crafting
                .table
                .as_ref()
                .unwrap();
            assert_eq!(
                table.slot(0),
                Some(&ItemStack::new("minecraft:oak_planks", 4))
            );
            game.guest_close_all(2);
            let mut failures = Vec::new();
            for (label, count, known) in [
                ("place", 3, true),
                ("ghost", 0, true),
                ("forgotten", 3, false),
            ] {
                game.as_guest(2, |g| {
                    g.run_command("clear @s");
                })
                .unwrap();
                if count > 0 {
                    command(game, "item replace entity @s hotbar.0 with oak_log 3");
                }
                command(
                    game,
                    "item replace entity @s hotbar.8 with stick[minecraft:custom_data={h234:1}] 7",
                );
                assert!(game.run_command("setblock 10 200 8 air"));
                assert!(game.run_command("setblock 10 200 8 furnace"));
                assert!(game.run_command("item replace block 10 200 8 container.1 with coal[minecraft:custom_data={h234:2}] 2"));
                let window = open(game, [10, 200, 8]);
                if !known {
                    command(game, "recipe take @s minecraft:charcoal");
                }
                assert_eq!(
                    game.lan
                        .as_ref()
                        .unwrap()
                        .hub
                        .guest(2)
                        .unwrap()
                        .recipe_book
                        .knows("minecraft:charcoal"),
                    known
                );
                let before = game
                    .lan
                    .as_ref()
                    .unwrap()
                    .hub
                    .guest(2)
                    .unwrap()
                    .inventory
                    .clone();
                let pos = (10, 200, 8);
                let Some(BlockEntity::Furnace(f)) = game.world.block_entities.get(&pos) else {
                    panic!("actual furnace missing")
                };
                assert!(f.items[0].is_none() && f.items[2].is_none());
                let fuel = f.items[1].clone();
                assert_eq!(fuel.as_ref().unwrap().count, 2);
                place(game, window, charcoal);
                let Some(BlockEntity::Furnace(f)) = game.world.block_entities.get(&pos) else {
                    panic!("furnace removed")
                };
                assert_eq!(f.items[1], fuel);
                assert!(f.items[2].is_none());
                let mut expected = before.clone();
                let input = if label == "place" {
                    expected[0].as_mut().unwrap().count = 2;
                    Some(ItemStack::new("minecraft:oak_log", 1))
                } else {
                    None
                };
                if f.items[0] != input
                    || game.lan.as_ref().unwrap().hub.guest(2).unwrap().inventory != expected
                {
                    failures.push((
                        label,
                        f.items.clone(),
                        game.lan
                            .as_ref()
                            .unwrap()
                            .hub
                            .guest(2)
                            .unwrap()
                            .inventory
                            .clone(),
                    ));
                }
                assert_eq!(game.lan.as_ref().unwrap().hub.guest(2).unwrap().uuid, 234);
                assert_eq!(
                    game.lan.as_ref().unwrap().hub.guest(2).unwrap().health,
                    20.0
                );
                assert_eq!(
                    game.lan.as_ref().unwrap().hub.guest(2).unwrap().position,
                    [8.5, 200.0, 8.5]
                );
                assert_eq!(game.world.game_time, clock);
                game.guest_close_all(2);
            }
            assert!(
                failures.is_empty(),
                "actual registered PlaceRecipe furnace failures: {failures:?}"
            );
        });
    }
}

#[cfg(test)]
mod recipe_check_tests {
    use super::*;
    use myriade_engine::{
        containers::{ModBlockEntity, ModBlockEntityFields, ModCrafting},
        lan::{
            Joined, NetConfig, Out, PlayerSender, guest::Guest, hub::HubConfig,
            menu::Menu as HubMenu,
        },
    };
    use horizon_protocol::packets::configuration::ClientInformation;
    use std::sync::{Arc, atomic::AtomicI32, mpsc};

    const STATION: (i32, i32, i32) = (4, 200, 4);
    const WINDOW: i32 = 7;

    /// What a check must leave alone: the station's block entity, the guest's
    /// inventory, grid and cursor, its menus' slots, and the station's chunk
    /// unmarked as changed (`container_changed` marks it; this takes the mark).
    fn state(game: &mut Game<'_>) -> String {
        let changed = game
            .world
            .modified
            .remove(&(STATION.0 >> 4, STATION.2 >> 4));
        let host = game.lan.as_ref().unwrap();
        let g = &host.hub.guests[0];
        format!(
            "{:?} {:?} {:?} {:?} {:?} {:?} {:?}",
            game.world.block_entities.get(&STATION),
            g.inventory,
            g.craft_grid,
            g.carried,
            g.menu.as_ref().map(|m| (&m.sent, m.state, &m.craft)),
            host.menus
                .get(&2)
                .is_some_and(|p| p.crafting.machine.is_some()),
            changed,
        )
    }

    /// The packets sent to the guest since the last call.
    fn sent(rx: &mpsc::Receiver<Out>) -> Vec<C> {
        let mut bodies = Vec::new();
        for out in rx.try_iter() {
            match out {
                Out::Packet(body) => bodies.push(body),
                Out::Packets(more) => bodies.extend(more),
                Out::Disconnect(_) => panic!("unexpected disconnect"),
            }
        }
        bodies
            .iter()
            .map(|body| {
                let mut r = horizon_protocol::buf::Reader::new(body);
                let id = r.varint().unwrap();
                C::decode(id, r.rest()).unwrap()
            })
            .collect()
    }

    /// A check's reply: the window, the tag, the result byte and the missing cells.
    fn reply(packets: &[C]) -> (i32, i32, u8, Vec<u8>) {
        let [C::CustomPayload(p)] = packets else {
            panic!("expected only the reply, got {packets:?}");
        };
        assert_eq!(p.channel, recipes::CHECK_CHANNEL);
        let mut r = horizon_protocol::buf::Reader::new(&p.data);
        let (window, tag, result) = (r.varint().unwrap(), r.varint().unwrap(), r.u8().unwrap());
        let mut cells = Vec::new();
        if result == 1 {
            for _ in 0..r.varint().unwrap() {
                cells.push(r.u8().unwrap());
            }
        }
        assert_eq!(r.remaining(), 0);
        (window, tag, result, cells)
    }

    fn request(window: i32, tag: i32, id: &str) -> Vec<u8> {
        let mut w = horizon_protocol::buf::Writer::new();
        w.varint(window);
        w.varint(tag);
        w.string(id);
        w.data
    }

    /// A check on a guest's mod crafting station answers as the transfer
    /// would and changes nothing: no slot, no block entity, no chunk marked
    /// changed, no packet but its reply. The transfer after it does move.
    #[test]
    #[ignore = "daily"]
    fn a_check_answers_and_leaves_the_world_and_the_guest_alone() {
        crate::game::native_entity_refs::tests::with_game(|game| {
            game.build_registries();
            game.lan_host(
                crate::game::lan::LanSettings {
                    port: 0,
                    guest_commands: false,
                    force_game_mode: false,
                },
                NetConfig {
                    announce: false,
                    ..NetConfig::loopback(0, "recipe-check-test".into(), 2)
                },
                HubConfig {
                    default_game_mode: 0,
                    guest_commands: false,
                    force_game_mode: false,
                    hardcore: false,
                    view_distance: 2,
                    simulation_distance: 2,
                    dimension: "minecraft:overworld".into(),
                    allow_flight: false,
                    seed: 0,
                    max_players: 2,
                    levels: Default::default(),
                    level_seeds: Default::default(),
                },
            )
            .unwrap();
            let station: &'static ModMenuType = Box::leak(Box::new(ModMenuType {
                id: "test:station",
                slots: 10,
                data_slots: 0,
                results: vec![],
                links: None,
                crafting: Some(ModCrafting {
                    grid: 1,
                    size: 3,
                    result: 0,
                }),
            }));
            let mut items = vec![None; 10];
            items[5] = Some(ItemStack::new("minecraft:cobblestone", 1));
            game.world.block_entities.insert(
                STATION,
                BlockEntity::Mod(Box::new(ModBlockEntity::new(
                    "test:station".into(),
                    ModBlockEntityFields {
                        items,
                        ..Default::default()
                    },
                ))),
            );
            let (tx, rx) = mpsc::channel();
            let mut guest = Guest::new(
                Joined {
                    conn: 2,
                    uuid: 2640,
                    name: "CheckGuest".into(),
                    properties: Vec::new(),
                    info: ClientInformation::new(2),
                    brand: "horizon".into(),
                    sender: PlayerSender::new(tx, Arc::default()),
                    latency: Arc::new(AtomicI32::new(0)),
                    address: "127.0.0.1:25565".parse().unwrap(),
                    expected: false,
                },
                2,
                [4.5, 201.0, 4.5],
                2,
            );
            guest.placed = true;
            guest.loaded = true;
            guest.inventory[0] = Some(ItemStack::new("minecraft:oak_planks", 2));
            guest.menu = Some(HubMenu {
                window: WINDOW,
                kind: 0,
                registry_id: "test:station".into(),
                pos: Some(STATION),
                slots: Slots::Block(STATION),
                size: 10,
                state: 0,
                sent: vec![None; 46],
                drag: None,
                sent_data: Vec::new(),
                hidden: Vec::new(),
                mod_type: Some(station),
                craft: None,
                links: Vec::new(),
            });
            game.lan.as_mut().unwrap().hub.guests.push(guest);
            sent(&rx);
            game.world.modified.clear();
            let before = state(game);
            let check = |game: &mut Game<'_>, window, tag, id| {
                game.guest_recipe_check(2, &request(window, tag, id));
                assert_eq!(state(game), before, "a check of {id} changed something");
                reply(&sent(&rx))
            };
            // Two planks make a stick; the grid's cobblestone has room to go.
            assert_eq!(
                check(game, WINDOW, 1, "minecraft:stick"),
                (WINDOW, 1, 0, vec![])
            );
            // A crafting table's last two planks lack.
            assert_eq!(
                check(game, WINDOW, 2, "minecraft:crafting_table"),
                (WINDOW, 2, 1, vec![2, 3])
            );
            // An unknown recipe, and a window the guest doesn't have open:
            // not into this menu.
            assert_eq!(
                check(game, WINDOW, 3, "minecraft:no_such_recipe"),
                (WINDOW, 3, 3, vec![])
            );
            assert_eq!(
                check(game, WINDOW + 1, 4, "minecraft:stick"),
                (WINDOW + 1, 4, 3, vec![])
            );
            // The transfer the first check answered for does move them.
            game.guest_recipe_transfer(2, &{
                let mut w = horizon_protocol::buf::Writer::new();
                w.varint(WINDOW);
                w.string("minecraft:stick");
                w.data
            });
            assert_ne!(state(game), before, "the transfer moved nothing");
        });
    }

    /// Ten checks at once are answered, the next is dropped, and one more
    /// goes through once the throttle has drained.
    #[test]
    #[ignore = "daily"]
    fn checks_past_the_limit_go_unanswered() {
        crate::game::native_guest_turn_tests::with_guests(|game| {
            let (tx, rx) = mpsc::channel();
            let host = game.lan.as_mut().unwrap();
            host.hub.guests.update(0, |g| {
                g.brand = "horizon".into();
                g.sender = PlayerSender::new(tx, Arc::default());
            });
            let conn = host.hub.guests[0].conn;
            for tag in 0..11 {
                game.guest_recipe_check(conn, &request(0, tag, "minecraft:stick"));
            }
            let replies = sent(&rx);
            assert_eq!(replies.len(), 10);
            for _ in 0..RECIPE_CHECK_COST {
                let play = game.lan.as_mut().unwrap().play.entry(conn).or_default();
                play.recipe_checks = play.recipe_checks.saturating_sub(1);
            }
            game.guest_recipe_check(conn, &request(0, 11, "minecraft:stick"));
            assert_eq!(reply(&sent(&rx)).1, 11);
        });
    }
}


#[cfg(test)]
#[path = "lan_crafting/stonecutter_stat_tests.rs"]
mod stonecutter_stat_tests;
