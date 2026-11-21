//! `AnvilScreen`'s name: the server keeps the name the player typed, up to 50 characters.
use super::Station;

/// `AnvilScreen`: `name.setMaxLength(50)`.
pub(super) const NAME_CHARS: usize = 50;

/// `AnvilScreen.slotChanged`: the name shows the input's name.
pub(super) fn input_changed(station: &mut Station, name: String) {
    station.name = name;
}

#[cfg(test)]
mod tests {
    use super::super::{Kind, Station};
    use super::input_changed;

    #[test]
    fn the_name_follows_the_input_slot() {
        let mut station = Station::new_kind(Kind::Anvil, [0; 3]);
        station.inputs[0] = Some(crate::items::ItemStack::new("minecraft:diamond_sword", 1));
        input_changed(&mut station, "Diamond Sword".into());
        assert_eq!(station.name, "Diamond Sword");

        station.inputs[0] = None;
        input_changed(&mut station, String::new());
        assert_eq!(station.name, "");
    }
}
