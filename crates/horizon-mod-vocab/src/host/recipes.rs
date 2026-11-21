//! Host method declarations; access defaults and game forwarders are generated.
#![allow(unused_variables)]
use crate::*;
/// The host's methods both sides share of the `recipes` area, implemented by the game and reached through [`Host`].
#[horizon_mod_api_macros::host_area]
pub trait RecipesHost {
    /// Side: both.
    /// Published asset/data read: available in Setup, Reloading and World.
    /// The recipes of registered recipe type `kind` (`minecraft:crafting`, `mymod:crusher`), vanilla's
    /// types and mods' own alike, from `data/<namespace>/recipe/*.json` of vanilla,
    /// the content mods (`.hmod`) and the world's data packs, in file order: each
    /// one typed (`Recipe`: its ingredients, results, group, cooking time and
    /// experience, or its stonecutting, smithing or special crafting rule),
    /// with its file as JSON for a mod's own type. The file's type field names its
    /// serializer; this can differ from the reported recipe kind. Dynamic rules are not fixed
    /// outputs; use `LevelRef::recipe_for` for engine-matchable recipe types.
    /// Registered crafting kinds match through LevelRef::recipe_for. Cache other mod-owned
    /// types once per load/reload and match those in mod memory; those return
    /// RecipeLookupError::NotMatchable. These are the
    /// recipes in force (`RecipeManager.getAllRecipesFor`): data packs merged and
    /// load conditions held, so a pack's override or removal shows. A data pack
    /// changes them on `/reload`, so look them up again when you need them. Empty
    /// for a type nobody uses and outside a world. These are the server's table on
    /// the server thread, a dedicated server or a LAN host; a client, the
    /// single-player window included, sees vanilla's and the content mods' recipes,
    /// not the world's data packs', unless a mod had the server send it the type
    /// (`native_datapack_sync`).
    #[access(PublishedResources)]
    fn recipes_of_type(&self, kind: &str) -> Arc<[Recipe]>;
    /// Side: both.
    /// Published asset/data read: available in Setup, Reloading and World.
    /// An ingredient from recipe-file JSON text: an item, a `#tag`, or a list
    /// of them, a bare id as `minecraft:` (as vanilla reads it); `test` says
    /// whether a stack fits. Items the game doesn't know are left out; None
    /// for missing tags, invalid ingredients or no known named items, and
    /// outside a world. An existing-empty tag returns an empty ingredient.
    /// Invalid JSON is refused with `RecipeJsonError::InvalidJson`.
    #[access(PublishedResources)]
    fn recipe_ingredient(&self, json: &str) -> Result<Option<Ingredient>, RecipeJsonError>;
    /// Side: both.
    /// Published asset/data read: available in Setup, Reloading and World.
    /// A stack from recipe-file `result` JSON text: `id` (a bare one as
    /// `minecraft:`), `count` (1 by default, up to 99) and `components`. None
    /// without an `id`, for an item the game doesn't know or a count outside
    /// that, and outside a world.
    /// Invalid JSON is refused with `RecipeJsonError::InvalidJson`.
    #[access(PublishedResources)]
    fn recipe_result(&self, json: &str) -> Result<Option<ItemStack>, RecipeJsonError>;
    /// Side: both.
    /// The `book_id`s of the recipes the player's recipe book holds now
    /// (the server's book on a server); empty outside a world.
    #[access(WorldOnly)]
    fn known_recipes(&mut self) -> Arc<[i32]>;
    /// Side: both.
    /// Published asset/data read: available in Setup, Reloading and World.
    /// The validated category and icon of a custom recipe type, or None for
    /// an undeclared/invalid type, before resources load, or outside a world.
    /// Returns the first category in category-id order when a type has several.
    /// Any invalid declaration refuses the complete generation.
    /// Rebuilt when the recipe table changes. Does not unlock any recipes.
    #[access(PublishedResources)]
    fn recipe_book_category(&mut self, kind: &str) -> Option<RecipeBookCategory>;
}
/// The handles' reads of the `recipes` area, implemented by the game and reached through [`Backend`].
// hidden: a handle's backend, reached through `Backend`; mods call the handle, never this trait
#[doc(hidden)]
#[horizon_mod_api_macros::host_area]
pub trait RecipesBackend {
    /// Side: both. Exact load-time membership for component-independent ingredients.
    /// Custom/component predicates anywhere in the tree are NotEnumerable.
    #[access(PublishedResources)]
    fn recipe_ingredient_item_keys(
        &self,
        ingredient: &Ingredient,
    ) -> Result<Arc<[ItemKey]>, IngredientKeysError>;
    /// Side: both.
    /// Whether `stack` fits `ingredient` (`Ingredient.test`), matched with this
    /// game's loop context.
    #[access(WorldOnly)]
    fn recipe_ingredient_test(&self, ingredient: &Ingredient, stack: &ItemStack) -> bool;
}
