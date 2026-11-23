use crate::*;


/// A mod's book category and displays for one custom recipe type.
#[derive(Clone, Debug, PartialEq)]
pub struct RecipeBookCategory {
    /// A namespaced category id outside `minecraft:`.
    pub id: String,
    /// A custom type present in the current recipe table.
    pub recipe_type: String,
    /// A registered item id used as the category's icon.
    pub icon: String,
    /// The menu whose screen owns this category.
    pub target_menu: String,
    /// Registered book identity, separate from category identity and settings.
    /// Declaring a category does not itself register a book or change its flags.
    pub book: RecipeBookTypeKey,
    /// Search category aggregating exactly this book's categories.
    pub search_group: String,
    /// Recipe-type supplied displays, rebuilt with this declaration on reload.
    pub displays: Vec<RecipeBookDisplayEntry>,
}

/// What a recipe viewer's transfer into the open crafting grid does or would
/// do (`RecipesClientHost::transfer_recipe`, `transfer_recipe_check`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecipeTransferResult {
    /// It moves a set of the recipe's ingredients into the grid.
    Moves,
    /// The items there are don't make a set.
    Missing,
    /// The grid's items have no room in the inventory.
    NoRoom,
    /// Not into the open menu: the recipe doesn't fit its grid, or the
    /// server doesn't know it.
    NotIntoMenu,
}

/// The server's answer to `transfer_recipe_check`: what the transfer would
/// do, and with `Missing`, the recipe's cells nothing fills.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecipeTransferCheck {
    /// What the transfer would do.
    pub result: RecipeTransferResult,
    /// With `Missing`, the recipe's cells no item fills: indices into its
    /// slots (width by height for a shaped recipe, else its ingredients in
    /// order). Empty otherwise.
    pub missing_cells: Vec<u32>,
}

/// A namespaced custom recipe book identity returned during registration.
/// This owns no settings; player settings and synchronization are separate.
#[derive(Clone, Debug, PartialEq, Eq, Hash, )]
pub struct RecipeBookTypeKey {
    pub id: String,
}

impl RecipeBookTypeKey {


    /// The stable namespaced identity used in saves and negotiated payloads.
    pub fn as_str(&self) -> &str {
        &self.id
    }
}
