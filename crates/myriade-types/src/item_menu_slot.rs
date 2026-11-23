use crate::*;


impl MenuRef {
    /// The stack on this menu's cursor, as a place (`ItemAccess.forPlayerCursor`): nothing once
    /// the menu is gone.
    pub fn cursor(self) -> ItemPlace {
        ItemPlace::PlayerCursor { menu: self }
    }
}
