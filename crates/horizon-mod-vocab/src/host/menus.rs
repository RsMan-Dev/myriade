//! Host method declarations; access defaults and game forwarders are generated.
#![allow(unused_variables)]
use crate::*;
/// The server host's methods of the `menus` area, implemented by the game and reached through [`ServerHost`].
#[horizon_mod_api_macros::host_area]
pub trait MenusServerHost {
    /// Side: server.
    /// Opens a chest menu of `rows` rows (1 to 6) titled `title` for the
    /// player, showing `items` (by slot, padded with empty ones), in place of
    /// the menu they had open: its window id, a new one each time
    /// (`ServerPlayer.openMenu`). The old menu closes as a close does, its
    /// cursor stack going back to the player's inventory; `replace` says
    /// whether the client is first sent a close for it
    /// (`MenuProvider.shouldTriggerClientSideContainerClosingOnOpen`).
    /// `MenuReplace::InPlace` changes the title and rows without the client
    /// centring the mouse again. Packets for the old window are ignored. The
    /// items of an open menu change through its slots (`menu`), not by
    /// opening it again.
    #[access(WorldOnly)]
    fn open_menu(
        &mut self,
        uuid: u128,
        rows: u8,
        title: Text,
        items: Vec<Option<ItemStack>>,
        replace: MenuReplace,
    ) -> Result<Option<i32>, StackWriteError>;
    /// Side: server.
    /// The menu `open_menu` opened that the player has open as window
    /// `window` (the window id `open_menu`, `menu_clicked` and `menu_closed`
    /// give). Its slots are written as `ItemPlace::MenuSlot` in a
    /// transaction, and the client is sent each changed slot with the next
    /// tick's container sync (`AbstractContainerMenu.broadcastChanges`), as
    /// for any open menu. None for another window, a menu the game opened, or
    /// a player who isn't here.
    #[access(WorldOnly)]
    fn menu(&self, uuid: u128, window: i32) -> Option<MenuRef>;
    /// Side: server.
    /// Closes the player's native menu (a `menu_closed` follows).
    #[access(WorldOnly)]
    fn close_menu(&mut self, uuid: u128);
    /// Side: server.
    /// Replaces everything the player's open `open_menu` menu `window` shows with `items` (by
    /// slot, padded with empty ones, extra ones dropped, as `open_menu` takes them) and sends
    /// it at once as one `ClientboundContainerSetContentPacket`
    /// (`AbstractContainerMenu.sendAllDataToRemote`), without reopening the menu: the title,
    /// the cursor and the screen stay. Writing slots through `menu` sends each changed slot with
    /// the next tick's sync instead; this is the whole-contents form, for a page turned or a
    /// list rebuilt. Each refusal sends nothing and changes nothing, and is named by
    /// `MenuItemsRefusal`: `Client`, `NoPlayer` (no placed player of that UUID), `NotOpen`
    /// (`window` is not their open `open_menu` menu) and `Stack` (a stack the game's item
    /// conversion refuses).
    #[access(WorldOnly)]
    fn menu_set_items(
        &mut self,
        uuid: u128,
        window: i32,
        items: Vec<Option<ItemStack>>,
    ) -> Result<(), MenuItemsRefusal>;
}
/// The handles' reads of the `menus` area, implemented by the game and reached through [`Backend`].
// hidden: a handle's backend, reached through `Backend`; mods call the handle, never this trait
#[doc(hidden)]
#[horizon_mod_api_macros::host_area]
pub trait MenusBackend {
    /// Side: server.
    /// Container id (0 for the player's inventory menu).
    #[access(WorldOnly)]
    fn menu_container_id(&self, menu: MenuRef) -> Option<i32>;
    /// Side: server.
    /// Registry menu type; the inventory menu has no MenuType in vanilla.
    /// The outer None means an unavailable menu.
    #[access(WorldOnly)]
    fn menu_kind(&self, menu: MenuRef) -> Option<Option<String>>;
    /// Side: both.
    /// Number of slots in AbstractContainerMenu.slots.
    #[access(WorldOnly)]
    fn menu_slot_count(&self, menu: MenuRef) -> Option<usize>;
    /// Side: both.
    /// Full stack at a menu index. Outer None means unavailable or out of
    /// range; Some(None) means an empty slot. `slot` is the slot's position in the menu's slot
    /// list, vanilla's `Slot.index`, the number container click packets carry; vanilla has no other
    /// player-visible slot number. `menu_slot_backing` gives the index inside the backing container.
    #[access(WorldOnly)]
    fn menu_slot(&self, menu: MenuRef, slot: usize) -> Option<Option<ItemStack>>;
    /// Side: both.
    /// Carried (cursor) stack; outer None means unavailable.
    #[access(WorldOnly)]
    fn menu_carried(&self, menu: MenuRef) -> Option<Option<ItemStack>>;
    /// Side: server.
    /// The storage menu slot `slot` shows (`Slot.container` and `getContainerSlot`): the
    /// player's inventory by `ENTITY_ITEMS` index (`InventoryMenu`'s armour slots 5..9 go head
    /// to feet), a block entity's or an entity's slot, the ender chest's, the crafting grid's
    /// or a mod's chest menu's.
    #[access(WorldOnly)]
    fn menu_slot_backing(&self, menu: MenuRef, slot: usize) -> Option<MenuSlotBacking>;
    /// Side: server.
    /// How many of `stack` menu slot `slot` holds (`Slot.mayPlace`, then `Slot.getMaxStackSize`):
    /// a furnace's fuel slot one bucket, an armour slot one piece that fits it, a crafting result
    /// none.
    #[access(WorldOnly)]
    fn menu_slot_holds(&self, menu: MenuRef, slot: usize, stack: &ItemStack) -> Option<u32>;
    /// Side: server.
    /// Whether menu slot `slot` gives up what it holds (`Slot.mayPickup`): bound armour stays
    /// on outside creative.
    #[access(WorldOnly)]
    fn menu_slot_gives(&self, menu: MenuRef, slot: usize) -> Option<bool>;
}
/// The handles' client reads of the `menus` area, implemented by the game and reached through [`ClientBackend`].
// hidden: a handle's backend, reached through `ClientBackend`; mods call the handle, never this trait
/// The handles' client writes of the `menus` area, implemented by the game and reached through [`ClientBackendMut`].
// hidden: a handle's backend, reached through `Backend`; mods call the handle, never this trait
/// The handles' server writes of the `menus` area, implemented by the game and reached through [`Backend`].
// hidden: a handle's backend, reached through `Backend`; mods call the handle, never this trait
#[doc(hidden)]
#[horizon_mod_api_macros::host_area]
pub trait MenusBackendMut {
    /// Side: server.
    /// Close a player's open menu as `ServerPlayer.closeContainer` does, running the normal
    /// lifecycle once: the container's lid, a vehicle's or a trade's stand-in, the crafting,
    /// station and mount menus, the carried stack back to the inventory,
    /// `player_container_close`, then the client's window closed (`ContainerClose`). Refuses
    /// stale menus, clients, a player whose state is swapped in for a turn, and the player's own
    /// inventory menu, which is never closed. A client closes its own screen with `close`.
    #[access(WorldOnly)]
    fn menu_close_container(&mut self, menu: MenuRef) -> bool;
}
