//! The recipe book of the inventory, crafting table and furnace screens
//! (`RecipeBookComponent`): the green book button, the tabs by category, the
//! search box, the craftable filter, the pages of recipe buttons with the
//! alternatives overlay, ghost recipes, and placing a recipe in the grid
//! (`ServerPlaceRecipe`). Which recipes are known comes from the advancement
//! rewards; the open and filter settings are saved with the player.
use super::Game;
use myriade_engine::{
    containers::{BlockEntity, FurnaceKind},
    items::ItemStack,
    menu::place::PostPlace,
    recipe_book::{Book, BookType, ClientBook, Entry, GhostTarget},
};
use myriade_types::RecipeTransferCheck;
use myriade_types::RecipeTransferResult;
use std::collections::BTreeSet;

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
    /// The last authoritative property-set and stonecutter generation.
    pub(super) server_properties: crate::recipe_book::ServerRecipeProperties,
    book: Option<Book>,
    ghost: Vec<Ghost>,
    /// The client's copy of the known recipes (`ClientRecipeBook`), its
    /// order deciding the buttons', and the recipes it was last synced with.
    client: ClientBook,
    client_known: Option<BTreeSet<String>>,
    client_version: u64,
}

impl RecipeUi {
    /// A new screen starts without a ghost recipe.
    pub(super) fn reset_screen(&mut self) {
        self.ghost.clear();
    }
}

impl Game<'_> {
    /// The vanilla book the open screen has, if any.
    pub(super) fn active_book(&self) -> Option<BookType> {
        if self.station.is_some() {
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
    }
    fn ensure_book(&mut self) {
        if self.recipe_ui.book.is_none() {
            let mut book = Book::build(self.recipes(), self.items());
            book.set_display_ids(&self.stations().recipe_index);
            self.recipe_ui.book = Some(book);
        }
    }
    /// Whether the open screen takes a book click: a crafting grid's book (the
    /// inventory's or a crafting table's).
    pub(super) fn mod_recipe_screen(&self) -> bool {
        self.active_book() == Some(BookType::Crafting) && !self.spectator()
    }
    /// The book's click on the recipe with this display
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
            || (self.creative && self.craft_size != 3)
        {
            self.show_transfer_result(RecipeTransferResult::NotIntoMenu);
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
        self.local_grid_transfer(&id, false);
    }
    /// The game's own grid transfer of `id` into this player's menu when the game is its own
    /// server (what a server runs when no `menu_transfer` part answers).
    fn local_grid_transfer(&mut self, id: &str, max: bool) {
        use myriade_engine::menu::place::TransferError;
        let mut result = Err(TransferError::Unavailable);
        self.menu_session(|menu, env, _| {
            result = menu.transfer_recipe(env, id, max);
            Some(result.is_ok())
        });
        self.show_transfer_result(myriade_engine::lan::recipes::transfer_result(result));
        if result.is_ok() {
            self.recipe_book_changed();
        }
    }



    fn show_transfer_result(&mut self, result: RecipeTransferResult) {
        if result == RecipeTransferResult::Moves {
        }
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
#[path = "recipe_book/resolve.rs"]
mod resolve;
impl Game<'_> {
    /// The stacks a mod recipe book's slot display stands for, with no stock in hand.
    pub(in crate::game) fn resolve_mod_book_declaration_slot(
        &self,
        slot: &myriade_types::RecipeSlotDisplay,
    ) -> Vec<ItemStack> {
        resolve::Resolver::new(self).resolve(slot)
    }
}
