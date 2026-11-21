//! The choice of a locomotion mode: the mediums the modes move in, and the mode an entity takes when it enters or leaves a fluid.
/// The medium a mode moves in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Medium {
    Ground,
    Water,
    Air,
    Lava,
}
/// The mode an entity goes back to on leaving a fluid: its ground, else its air, else none.
pub fn back_mode(has_ground: bool, has_air: bool) -> Option<Medium> {
    if has_ground { Some(Medium::Ground) } else if has_air { Some(Medium::Air) } else { None }
}
/// The mode a fluid change gives: water, then lava, else the mode it goes back to.
pub fn fluid_mode(water: bool, lava: bool, back: Option<Medium>) -> Option<Medium> {
    if water { Some(Medium::Water) } else if lava { Some(Medium::Lava) } else { back }
}
/// The active mode, from its markers (ground, water, air, lava in that order).
pub fn current_mode(walk: bool, swim: bool, fly: bool, lava: bool) -> Option<Medium> {
    if walk { Some(Medium::Ground) } else if swim { Some(Medium::Water) } else if fly { Some(Medium::Air) } else if lava { Some(Medium::Lava) } else { None }
}
