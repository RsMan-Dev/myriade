//! The recipe book of the inventory, crafting table and furnace screens
//! (`RecipeBookComponent`): the green book button, the tabs by category, the
//! search box, the craftable filter, the pages of recipe buttons with the
//! alternatives overlay, ghost recipes, and placing a recipe in the grid
//! (`ServerPlaceRecipe`). Which recipes are known comes from the advancement
//! rewards; the open and filter settings are saved with the player.
#[path = "recipe_book/mod_screen.rs"]
mod mod_screen;

use super::Game;
use horizon_client::{
    containers::{BlockEntity, FurnaceKind},
    hud::HudBatch,
    items::ItemStack,
    menu::place::PostPlace,
    recipe_book::{Book, BookType, ClientBook, Entry, GhostTarget},
    text::Text,
    ui::EditBox,
};
use mod_api::{RecipeTransferCheck, RecipeTransferResult};
use std::{
    collections::BTreeSet,
    time::{Duration, Instant},
};

/// A ghost recipe slot: the items it cycles through.
struct Ghost {
    slot: usize,
    items: Vec<ItemStack>,
    result: bool,
}
#[derive(Default)]
pub(super) struct RecipeUi {
    /// Explicit viewport for renderer-less resource-backed fixtures.
    #[cfg(test)]
    pub(super) test_viewport: Option<[u32; 2]>,
    custom: mod_screen::ModRecipeUi,
    /// The last authoritative property-set and stonecutter generation.
    pub(super) server_properties: crate::recipe_book::ServerRecipeProperties,
    book: Option<Book>,
    pub(super) search: Option<EditBox>,
    ghost: Vec<Ghost>,
    /// The client's copy of the known recipes (`ClientRecipeBook`), its
    /// order deciding the buttons', and the recipes it was last synced with.
    client: ClientBook,
    client_known: Option<BTreeSet<String>>,
    client_version: u64,
    /// The entries a server's packets carried, by display id: the book is
    /// made with them over its own (`Book::use_server_entries`).
    server_entries:
        std::collections::BTreeMap<i32, horizon_protocol::packets::recipes::RecipeBookEntry>,
    /// The server's checks of viewer transfers (`transfer_recipe_check`).
    checks: TransferChecks,
}

/// The server's answers to `horizon:recipe_check`, for what the client last
/// saw of the open menu: its window, its state id, and the inventory and
/// grid. Any change there forgets them.
#[derive(Default)]
struct TransferChecks {
    seen: Option<(i32, i32, Vec<Option<ItemStack>>)>,
    answers: Vec<CheckAnswer>,
    /// The one request out: its display id and when it went.
    asked: Option<(i32, i32, Instant)>,
}

impl TransferChecks {
    fn ask(&mut self, display_id: i32, now: Instant) -> Option<i32> {
        static NEXT_TAG: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);
        let tag = NEXT_TAG
            .try_update(
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
                |tag| tag.checked_add(1),
            )
            .ok()?;
        self.asked = Some((tag, display_id, now));
        Some(tag)
    }
}

/// One answer of `TransferChecks`.
struct CheckAnswer {
    display_id: i32,
    /// When it came.
    at: Instant,
    /// How many times it was asked again for this view of the menu.
    refreshes: u32,
    check: RecipeTransferCheck,
}

/// How old an answer is asked again while it is wanted (chests beside a
/// station change without the client seeing it), and how many times for one
/// view of the menu.
const CHECK_REFRESH: Duration = Duration::from_secs(2);
const CHECK_REFRESHES: u32 = 15;
/// A request with no answer this long is given up, and another may go.
const CHECK_WAIT: Duration = Duration::from_millis(500);

impl RecipeUi {
    /// A new screen starts without a ghost recipe, overlay or search focus.
    pub(super) fn reset_screen(&mut self) {
        self.ghost.clear();
        if let Some(search) = self.search.as_mut() {
            search.focused = false;
            search.set_value("");
        }
        self.custom = mod_screen::ModRecipeUi::default();
    }
}

impl Game<'_> {
    /// A viewer's transfer of a recipe by id, into a mod menu or a grid.
    pub(in crate::game) fn transfer_recipe_id(&mut self, recipe: &str, max: bool) {
        self.transfer_viewer_recipe_id(recipe, max);
    }

    /// The vanilla book the open screen has, if any.
    pub(super) fn active_book(&self) -> Option<BookType> {
        if self.station.is_some() {
            return None;
        }
        // Explicitly assigned mod panels use their own identity and flags.
        if self.mount_active() || self.native_screen_height().is_some() {
            return None;
        }
        match self.container_kind() {
            // The inventory's grid, or a crafting table's (`CraftingScreen`);
            // a server's other menus without a container have no book.
            None if self.container.is_none() => Some(BookType::Crafting),
            Some(BlockEntity::Furnace(f)) => Some(match f.kind {
                FurnaceKind::Furnace => BookType::Furnace,
                FurnaceKind::BlastFurnace => BookType::BlastFurnace,
                FurnaceKind::Smoker => BookType::Smoker,
            }),
            _ => None,
        }
    }
    fn book_open(&self, book: BookType) -> bool {
        self.level.recipe_book.open[book.index()]
    }
    /// Whether typed keys go to the search box.
    pub(super) fn recipe_search_focused(&self) -> bool {
        (self.mod_book_open() || self.active_book().is_some_and(|b| self.book_open(b)))
            && self.recipe_ui.search.as_ref().is_some_and(|s| s.focused)
    }
    /// Text typed (or one backspace) into the search box.
    pub(super) fn recipe_text(&mut self, text: Option<&str>, backspace: bool) {
        let Some(search) = self.recipe_ui.search.as_mut() else {
            return;
        };
        if backspace {
            search.delete_chars(-1);
        } else if let Some(text) = text {
            search.insert_text(text);
        }
    }
    // ---- recipes and what can be crafted ----

    /// Sends the client book what changed in the server's: the first time a
    /// replacing batch (vanilla's server iterates an identity hash set; this
    /// one goes in id order), then each unlock and removal.
    fn sync_client_book(&mut self) {
        // On a server the book is the server's packets' (remote_recipe_book_*).
        let known = &self.level.recipe_book.recipes;
        let index = &world_stations!(self).recipe_index;
        let ui = &mut self.recipe_ui;
        match ui.client_known.as_ref() {
            Some(previous) if previous == known => return,
            Some(previous) => {
                for id in previous.difference(known) {
                    for d in index.display_ids(id) {
                        ui.client.remove(d);
                    }
                }
                for id in known.difference(previous) {
                    for d in index.display_ids(id) {
                        ui.client.add(d);
                    }
                }
            }
            None => {
                ui.client.clear();
                for id in known {
                    for d in index.display_ids(id) {
                        ui.client.add(d);
                    }
                }
            }
        }
        ui.client_known = Some(known.clone());
        ui.client_version += 1;
    }
    /// Every entry of the book, in its order (`native_recipes`).
    pub(super) fn recipe_book_entries(&mut self) -> Vec<Entry> {
        self.ensure_book();
        self.recipe_ui
            .book
            .as_ref()
            .map_or_else(Vec::new, |b| b.entries.clone())
    }
    /// The display ids the player's book holds, and a number that changes
    /// whenever they do (`known_recipes`).
    pub(super) fn recipe_book_displays(&mut self) -> (u64, Vec<i32>) {
        self.sync_client_book();
        (
            self.recipe_ui.client_version,
            self.recipe_ui.client.displays(),
        )
    }
    /// The book's model is made again from `Game::recipes` when next needed:
    /// the server's recipes arrived, or a new session began.
    pub(super) fn forget_book(&mut self) {
        self.recipe_ui.book = None;
        self.recipe_ui.custom = mod_screen::ModRecipeUi::default();
        self.recipe_tables.books = None;
        self.invalidate_mod_book_placement_catalogues();
    }
    fn ensure_book(&mut self) {
        if self.recipe_ui.book.is_none() {
            let mut book = Book::build(self.recipes(), self.items());
            book.set_display_ids(self.display_index());
            self.recipe_ui.book = Some(book);
        }
        self.ensure_recipe_search();
    }
    fn ensure_recipe_search(&mut self) {
        if self.recipe_ui.search.is_none() {
            let mut search = EditBox::new(0.0, 0.0, 81.0, 14.0);
            search.message = super::screen_info::tr("itemGroup.search", &[]);
            search.max_length = 50;
            search.focused = false;
            search.text_color = u32::MAX;
            self.recipe_ui.search = Some(search);
        }
    }
    // ---- geometry ----

    fn batch_for_size(&self) -> Option<HudBatch> {
        #[cfg(test)]
        if let Some(size) = self.recipe_ui.test_viewport {
            return Some(HudBatch::new(size, self.settings.gui_scale_inputs()));
        }
        let r = self.renderer.as_ref()?;
        Some(HudBatch::new(
            [r.config.width, r.config.height],
            self.settings.gui_scale_inputs(),
        ))
    }

    /// Whether the open screen takes `native_place_recipe`: a crafting grid's
    /// book (the inventory's or a crafting table's), or a mod menu type's
    /// crafting grid.
    pub(super) fn mod_recipe_screen(&self) -> bool {
        (self.active_book() == Some(BookType::Crafting) || self.mod_crafting_open())
            && !self.spectator()
    }
    /// Whether the open menu is a mod menu type with a crafting grid.
    fn mod_crafting_open(&self) -> bool {
        self.remote
            .as_ref()
            .and_then(|r| r.menu.as_ref())
            .and_then(|(_, menu)| horizon_client::containers::mod_menu_type(menu))
            .is_some_and(|t| t.crafting.is_some())
    }
    /// `native_place_recipe`: the book's click on the recipe with this display
    /// id, refused as `handlePlaceRecipe` refuses it (no crafting screen,
    /// not in the player's book; an impossible one in `place_recipe`).
    pub(super) fn mod_place_recipe(&mut self, display_id: i32, all: bool) {
        if !self.mod_recipe_screen() || !self.recipe_ui.client.displays().contains(&display_id) {
            return;
        }
        self.ensure_book();
        let Some(index) = self
            .recipe_ui
            .book
            .as_ref()
            .and_then(|b| b.entries.iter().position(|e| e.display_id == display_id))
        else {
            return;
        };
        self.recipe_ui.ghost.clear();
        self.place_book_recipe(BookType::Crafting, index, all);
    }
    /// Whether the open menu is a content mod's menu type (a machine's, with or without a grid):
    /// the server's menu on a server, the block's own in a solo world or on a LAN host.
    fn mod_menu_open(&self) -> bool {
        self.remote_menu_type()
            .and_then(horizon_client::containers::mod_menu_type)
            .is_some()
            || self.local_mod_menu().is_some()
    }
    /// The content mod menu type and block of the menu this player has open when the game is
    /// its own server (a solo world, a LAN host's own player): the block container's menu type.
    fn local_mod_menu(&self) -> Option<(&'static str, [i32; 3])> {
        let k = self.container?;
        Some((self.mod_menu_at(k)?, [k.0, k.1, k.2]))
    }
    /// `menu_transfer` for this player's own mod menu when the game is its own server: the same
    /// server-side ask a guest's transfer gets (`guest_menu_transfer`), on the same thread.
    /// None when no part listens (vanilla menus never ask) or no mod menu is open here.
    fn local_menu_transfer(
        &mut self,
        recipe: &str,
        max: bool,
        dry_run: bool,
    ) -> Option<RecipeTransferResult> {
        if !mod_api::__private::has_part_hook(mod_api::HookId::MenuTransfer) {
            return None;
        }
        let (menu, pos) = self.local_mod_menu()?;
        if !mod_api::__private::menu_listens(mod_api::HookId::MenuTransfer, menu) {
            return None;
        }
        let mut result = None;
        self.native_menu_transfer(mod_api::__private::MenuTransferCall {
            player: self.own_uuid(),
            menu,
            pos: Some(pos),
            recipe,
            max,
            dry_run,
            result: &mut result,
        });
        result
    }
    /// A viewer's transfer of a recipe by id (`transfer_recipe_id`): sent to the server, whose
    /// `menu_transfer` parts on a mod menu answer first, then its grid transfer. Only a server
    /// that takes viewer transfers is asked: elsewhere it shows as not into this menu.
    pub(super) fn transfer_viewer_recipe_id(&mut self, recipe: &str, max: bool) {
        let open = !self.spectator() && (self.mod_menu_open() || self.mod_recipe_screen());
        if !open {
            self.show_transfer_result(RecipeTransferResult::NotIntoMenu);
            return;
        }
        {
            // The game is its own server: the mod menu's part answers first, as on a server,
            // then the grid transfer.
            if let Some(answer) = self.local_menu_transfer(recipe, max, false) {
                self.show_transfer_result(answer);
                return;
            }
            self.local_grid_transfer(recipe, max);
            return;
        }
        if !self
            .remote
            .as_ref()
            .is_some_and(|r| r.recipe_viewer.transfer)
        {
            self.show_transfer_result(RecipeTransferResult::NotIntoMenu);
            return;
        }
        let mut request = horizon_protocol::buf::Writer::new();
        request.varint(self.remote_menu_id());
        request.string(recipe);
        request.u8(u8::from(max));
        self.send(horizon_protocol::packets::play::Serverbound::CustomPayload(
            horizon_protocol::packets::configuration::CustomPayload {
                channel: horizon_client::lan::recipes::TRANSFER_CHANNEL.into(),
                data: request.data,
            },
        ));
    }
    /// The display ids of `recipe_id` that the server sent and this book
    /// numbers the same way (`Book::confirmed_ids`): the entries
    /// `transfer_viewer_recipe` and `transfer_recipe_check` find. A server
    /// numbers the recipes it sent itself, so the display index's ids are not
    /// evidence of its own.
    pub(in crate::game) fn viewer_book_ids(&mut self, recipe_id: &str) -> std::sync::Arc<[i32]> {
        self.ensure_book();
        self.recipe_ui
            .book
            .as_ref()
            .map_or_else(Default::default, |book| book.confirmed_ids(recipe_id))
    }
    /// A viewer's transfer of one set of ingredients.
    pub(super) fn transfer_viewer_recipe(&mut self, display_id: i32) {
        if !self.mod_recipe_screen()
            || (self.creative && self.craft_size != 3 && !self.mod_crafting_open())
        {
            self.show_transfer_result(RecipeTransferResult::NotIntoMenu);
            return;
        }
        if self
            .remote
            .as_ref()
            .is_some_and(|r| !r.recipe_viewer.transfer)
        {
            // Older Horizon and vanilla servers still accept book clicks.
            let _ = self.recipe_place(display_id, false);
            return;
        }
        self.ensure_book();
        let Some(id) = self
            .recipe_ui
            .book
            .as_ref()
            .and_then(|book| {
                book.entries
                    .iter()
                    .find(|entry| entry.display_id == display_id)
            })
            .map(|entry| entry.id.clone())
        else {
            self.show_transfer_result(RecipeTransferResult::NotIntoMenu);
            return;
        };
        // The game is its own server: a mod menu's part answers first, as on a server.
        if let Some(answer) = self.local_menu_transfer(&id, false, false) {
            self.show_transfer_result(answer);
            return;
        }
        self.local_grid_transfer(&id, false);
    }
    /// The game's own grid transfer of `id` into this player's menu when the game is its own
    /// server (what a server runs when no `menu_transfer` part answers).
    fn local_grid_transfer(&mut self, id: &str, max: bool) {
        use horizon_client::menu::place::TransferError;
        let mut result = Err(TransferError::Unavailable);
        self.menu_session(|menu, env, _| {
            result = menu.transfer_recipe(env, id, max);
            Some(result.is_ok())
        });
        self.show_transfer_result(horizon_client::lan::recipes::transfer_result(result));
        if result.is_ok() {
            self.recipe_book_changed();
        }
    }



    fn show_transfer_result(&mut self, result: RecipeTransferResult) {
        if result == RecipeTransferResult::Moves {
            self.close_transferred_recipe();
        }
    }

    /// `RecipesClientHost::transfer_recipe_check`: the server's latest check of a viewer
    /// transfer of `display_id` into the open crafting grid. Asked for when
    /// there is no answer for what the client sees now (the window, its state
    /// id, the inventory and the grid), and again every `CHECK_REFRESH` while
    /// it is wanted, at most `CHECK_REFRESHES` times for that view: chests a
    /// station takes from change where the client can't see them. One request
    /// out at a time. None until the answer comes, outside a screen that takes
    /// recipes, and on a server that doesn't advertise `horizon:recipe_check`.
    pub(in crate::game) fn transfer_recipe_check(
        &mut self,
        display_id: i32,
    ) -> Option<RecipeTransferCheck> {
        // The game is its own server: a mod menu's part answers the dry run, as
        // `horizon:recipe_check` asks it on a server; with none the check is as before.
        if self.mod_menu_open() {
            self.ensure_book();
            let id = (self.recipe_ui.book.as_ref())
                .and_then(|book| book.entries.iter().find(|e| e.display_id == display_id))
                .map(|e| e.id.clone())?;
            let result = self.local_menu_transfer(&id, false, true)?;
            return Some(RecipeTransferCheck {
                result,
                missing_cells: Vec::new(),
            });
        }
        let eligible_menu =
            self.mod_recipe_screen() || (!self.spectator() && self.mod_menu_open());
        if !eligible_menu || !self.remote.as_ref().is_some_and(|r| r.recipe_viewer.check) {
            return None;
        }
        let now = Instant::now();
        let seen = (
            self.remote_menu_id(),
            self.remote.as_ref().map_or(0, |r| r.state_id),
            (self.inventory.iter().take(36))
                .chain(&self.craft_grid)
                .cloned()
                .collect::<Vec<_>>(),
        );
        let checks = &mut self.recipe_ui.checks;
        if checks.seen.as_ref() != Some(&seen) {
            *checks = TransferChecks {
                seen: Some(seen),
                ..Default::default()
            };
        }
        let answer = (checks.answers.iter()).find(|a| a.display_id == display_id);
        let ask = match answer {
            None => true,
            Some(a) => a.refreshes < CHECK_REFRESHES && now.duration_since(a.at) >= CHECK_REFRESH,
        };
        let answer = answer.map(|a| a.check.clone());
        let waiting = (checks.asked).is_some_and(|(_, _, at)| now.duration_since(at) < CHECK_WAIT);
        if ask && !waiting {
            self.ensure_book();
            let id = (self.recipe_ui.book.as_ref())
                .and_then(|book| book.entries.iter().find(|e| e.display_id == display_id))
                .map(|e| e.id.clone());
            if let Some(id) = id {
                let mut request = horizon_protocol::buf::Writer::new();
                request.varint(self.remote_menu_id());
                let Some(tag) = self.recipe_ui.checks.ask(display_id, now) else {
                    return answer;
                };
                request.varint(tag);
                request.string(&id);
                self.send(horizon_protocol::packets::play::Serverbound::CustomPayload(
                    horizon_protocol::packets::configuration::CustomPayload {
                        channel: horizon_client::lan::recipes::CHECK_CHANNEL.into(),
                        data: request.data,
                    },
                ));
            }
        }
        answer
    }


    /// `ServerboundPlaceRecipePacket`: the server places the recipe
    /// (`ServerPlaceRecipe` on the menu engine) and answers with a ghost
    /// recipe when the items are missing.
    fn place_book_recipe(&mut self, book_type: BookType, entry_index: usize, use_max: bool) {
        let Some(entry) = self
            .recipe_ui
            .book
            .as_ref()
            .and_then(|b| b.entries.get(entry_index))
            .cloned()
        else {
            return;
        };
        let creative = self.creative;
        let mut post = None;
        let answer = &mut post;
        self.menu_session(|menu, env, _| {
            // handlePlaceRecipe: an impossible recipe is refused outright.
            if env
                .recipes
                .placement(&entry.id)
                .is_none_or(|p| p.impossible())
            {
                return Some(false);
            }
            *answer = Some(menu.place_recipe(env, &entry.id, use_max, creative, None));
            Some(true)
        });
        if post == Some(PostPlace::Ghost) {
            self.fill_ghost(book_type, &entry);
        }
        self.recipe_book_changed();
    }
    /// The container changed under the book: comparators and saving.
    fn recipe_book_changed(&mut self) {
        if let Some(key) = self.container {
            self.world.modified.insert((key.0 >> 4, key.2 >> 4));
            self.container_changed(key);
        }
    }

    // ---- a server's recipe book ----


    // ---- ghost recipe ----

    /// `fillGhostRecipe`: the recipe's ingredients faded into the slots.
    fn fill_ghost(&mut self, book_type: BookType, entry: &Entry) {
        let grid = self.craft_size;
        let fuel_empty = self.slot_mut(301).is_none();
        let Some(book) = self.recipe_ui.book.as_ref() else {
            return;
        };
        let ghost = book
            .ghost(entry, book_type, grid, fuel_empty, self.items())
            .into_iter()
            .map(|g| Ghost {
                slot: match (g.target, book_type) {
                    (GhostTarget::Result, BookType::Crafting) => 200,
                    (GhostTarget::Result, _) => 302,
                    (GhostTarget::Grid(i), _) => 100 + i,
                    (GhostTarget::Input, _) => 300,
                    (GhostTarget::Fuel, _) => 301,
                },
                result: g.target == GhostTarget::Result,
                items: g.items,
            })
            .collect();
        self.recipe_ui.ghost = ghost;
    }

    // ---- drawing ----

}

#[cfg(test)]
mod request_generation_tests {
    use super::*;
    #[test]
    fn delayed_answers_cannot_match_inventory_changes_or_retries() {
        let mut checks = TransferChecks::default();
        let first = checks.ask(42, Instant::now()).unwrap();
        checks = TransferChecks::default(); // Inventory change clears the cache.
        let second = checks.ask(42, Instant::now()).unwrap();
        assert_ne!(first, second);
        let retry = checks.ask(42, Instant::now()).unwrap();
        assert_ne!(second, retry);
        assert_eq!(
            checks.asked.map(|(tag, id, _)| (tag, id)),
            Some((retry, 42))
        );
    }
}

#[cfg(test)]
mod tests {
    /// `CraftingScreen` has the book; a server's menu without a grid has none.
    #[test]
    #[ignore = "daily"]
    fn a_servers_crafting_table_has_the_book() {
        use super::*;
        crate::game::native_entity_refs::tests::with_client_game(|game| {
            let (pipe, _server) = horizon_protocol::connection::Pipe::pair();
            game.connect_local(pipe, "Crafter").unwrap();
            game.container = None;
            let open = |game: &mut Game<'_>, menu: &str| {
                game.remote
                    .as_mut()
                    .unwrap()
                    .menu
                    .set(Some((3, menu.into())));
                game.active_book()
            };
            assert_eq!(open(game, "minecraft:crafting"), Some(BookType::Crafting));
            assert_eq!(open(game, "minecraft:cartography_table"), None);
        });
    }
}
