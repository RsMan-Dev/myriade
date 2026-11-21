//! The player's attribute base values, read by the server's attribute math.
use super::Game;

impl Game<'_> {
    /// `Player.getAttributeBaseValue`: the attribute's default where the player has none.
    fn player_attribute_base(&self, name: &str, default: f64) -> f64 {
        self.player
            .as_ref()
            .and_then(|player| {
                let key = player.attributes.key(name)?;
                let handle = player.attributes.lookup(Some(&key))?;
                player
                    .attributes
                    .instance(&handle)
                    .ok()
                    .map(|value| value.base_value())
            })
            .unwrap_or(default)
    }
}
