use crate::*;


/// A map decoration type a mod registers once at start: the id a `minecraft:map_decorations`
/// entry names it by, and how it is drawn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapDecorationType {
    /// `<namespace>:<path>` in a namespace other than vanilla's, like `mymod:treasure`.
    pub id: String,
    /// The GUI atlas sprite that draws it (`MapDecorationType.assetId`): a pack's texture or a
    /// mod's runtime sprite (`register_runtime_sprites`), such as `mymod:gui/treasure`.
    pub sprite: String,
    /// Whether it shows on a map in an item frame and on the cartography table's screen
    /// (`showOnItemFrame`); a held map shows every decoration.
    pub show_on_item_frame: bool,
}
