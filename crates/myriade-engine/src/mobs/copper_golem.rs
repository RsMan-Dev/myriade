use horizon_core::random::RandomSource;
use std::collections::HashMap;

// Copper golem state: `variant` the weathering (0 unaffected .. 3 oxidized),
// `variant3` the `CopperGolemState`, `main_hand` with `ints[1]` the carried
// stack, the saddle slot the antenna (`EQUIPMENT_SLOT_ANTENNA`), `vector[0]`
// the next weathering tick (-1 unset, -2 waxed), `ints[0]` the idle animation
// start tick, `opened_chest` the `openedChestPos`. AnimationState slots: idle
// (0), getting an item (1), getting nothing (2), dropping an item (3),
// dropping nothing (4).
pub const IDLE: u8 = 0;
pub const GETTING_ITEM: u8 = 1;
pub const GETTING_NO_ITEM: u8 = 2;
pub const DROPPING_ITEM: u8 = 3;
pub const DROPPING_NO_ITEM: u8 = 4;
/// `WeatheringCopper.WeatherState` names.
pub const WEATHER: [&str; 4] = ["unaffected", "exposed", "weathered", "oxidized"];

/// `WeatheringCopper.WeatherState.next`/`previous` on the variant.
pub fn weather_next(v: u8) -> u8 {
    (v + 1).min(3)
}
pub fn weather_previous(v: u8) -> u8 {
    v.saturating_sub(1)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn weathering_stops_at_both_ends() {
        assert_eq!(weather_next(3), 3);
        assert_eq!(weather_previous(0), 0);
        assert_eq!(weather_next(1), 2);
        assert_eq!(weather_previous(2), 1);
    }
}
