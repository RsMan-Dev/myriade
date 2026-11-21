//! The jump of a mount with a rider (`LocalPlayer.aiStep`'s jump bar, `AbstractHorse.handleStartJump`): the charge is there only while the
//! jump key is held, and the jump capability carries the sound of the jump and its volume (`AbstractHorse.playJumpSound`).
use bevy_ecs::prelude::*;

/// The ticks the jump key has been held on a mount (`Player.jumpRidingTicks`). Present only during a charge.
#[derive(Component)]
#[component(storage = "SparseSet")]
pub struct JumpCharge {
    pub ticks: u32,
}

/// The sound and volume a jump plays, set by the species that jumps. The default is the server jar's `SoundEvents.HORSE_JUMP`
/// (`entity.horse.jump`) at `playJumpSound`'s volume 0.4.
#[derive(Component, Clone, Copy)]
pub struct JumpSound {
    pub sound: &'static str,
    pub volume: f32,
}

impl Default for JumpSound {
    fn default() -> Self {
        Self { sound: "entity.horse.jump", volume: 0.4 }
    }
}

/// Gives the entity the `JumpSound` of the rule's parameters: the sound of `handleStartJump` of a mount that dashes
/// (`Camel.getDashingSound`, `CamelHusk.getDashingSound`).
pub static JUMP_SOUND: crate::rules::Rule = crate::rules::Rule {
    name: "jump_sound",
    equip: |w, e, p| {
        w.entity_mut(e).insert(*crate::rules::params::<JumpSound>(p));
    },
    attrs: crate::rules::no_attrs,
    interact: crate::rules::no_interact,
};
