//! `Interaction`: its box and answer are `passive::Interact`, its last attack and use too. `attack` and `click` write them when the input
//! makes them; no tick reads them. An attack answers as `skipAttackInteraction` does: the blow is skipped unless the interaction responds.
use crate::passive::{self, Act, Interact};
use crate::species::lists::INTERACTION;
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;
use horizon_core::nbt::Tag;

/// An interaction at `at` with its box and answer: `passive::spawn` reads them as the save does.
pub fn spawn(w: &mut World, at: Vec3, width: f32, height: f32, response: bool) -> Entity {
    let tag = Tag::compound([("width", Tag::Float(width.to_bits())), ("height", Tag::Float(height.to_bits())), ("response", Tag::Byte(i8::from(response)))]);
    passive::spawn(w, INTERACTION, at, tag)
}

/// An attack by `player` at `tick`, recorded. True when the blow is skipped (the interaction does not respond).
pub fn attack(w: &mut World, e: Entity, player: [i32; 4], tick: i64) -> bool {
    let Some(mut i) = w.get_mut::<Interact>(e) else { return false };
    i.attack = Some(Box::new(Act { player, timestamp: tick }));
    !i.response
}

/// A use (right click) by `player` at `tick`, recorded. The server's answer is always CONSUME (`interact`), so nothing is returned.
pub fn click(w: &mut World, e: Entity, player: [i32; 4], tick: i64) {
    if let Some(mut i) = w.get_mut::<Interact>(e) {
        i.interaction = Some(Box::new(Act { player, timestamp: tick }));
    }
}
