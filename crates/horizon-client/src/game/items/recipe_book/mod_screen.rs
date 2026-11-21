//! A mod menu's book uses the shared painter, but never vanilla storage ordinals.
use super::*;
use horizon_client::recipe_book::mod_presentation::ModPresentation;
#[path = "mod_screen/layout.rs"]
mod layout;
#[path = "mod_screen/placement.rs"]
mod placement;
#[path = "mod_screen/resolve.rs"]
mod resolve;

#[derive(Default)]
pub(super) struct ModRecipeUi {
    presentation: Option<ModPresentation>,
    prepared: Option<horizon_client::recipe_book::mod_presentation::PreparedBook>,
    stocks: Vec<ItemStack>,
    filtering: bool,
    refused: bool,
}

impl Game<'_> {
    fn mod_book_panel(&self) -> Option<horizon_client::recipe_book::ModBookPanel<'_>> {
        let key = self.native_screen_book()?;
        let menu = self.native_screen_menu()?;
        if mod_api::RecipeBookLayout::declaring_mod(menu).is_some() {
            let view = self.native_net.placement.view.as_ref()?;
            if !self.native_book_placement_catalog_matches(&view.identity) {
                return None;
            }
            return view.index.panel(menu, key);
        }
        self.recipe_tables
            .books
            .as_ref()?
            .as_ref()
            .ok()?
            .panel(menu, key)
    }
    fn mod_book_layout(&self, width: f32, open: bool) -> Option<layout::BookLayout> {
        let batch = self.batch_for_size()?;
        layout::layout(
            [width, batch.height],
            [self.native_screen_width()?, self.native_screen_height()?],
            self.native_screen_toggle(),
            open,
        )
    }
    pub(super) fn mod_book_flags(&self) -> Option<(bool, bool)> {
        self.mod_book_panel()?;
        let batch = self.batch_for_size()?;
        self.mod_book_layout(batch.width, false)?;
        self.mod_book_layout(batch.width, true)?;
        let flags = self.custom_book_settings(self.native_screen_book()?)?;
        Some((flags.open, flags.filtering))
    }
    pub(super) fn mod_book_open(&self) -> bool {
        self.mod_book_flags().is_some_and(|flags| flags.0)
    }
}
