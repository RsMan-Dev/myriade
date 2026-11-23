//! `Dolphin.playAttackSound` (`SoundEvents.DOLPHIN_ATTACK`) overrides the empty `Mob.playAttackSound`: the blow that lands plays the bite
//! at volume 1 and pitch 1, unless the attacker is silent. The rule poses the sound name, read by `hostile::landed`.
use bevy_ecs::prelude::*;

/// The bite sound of an attacker (`entity.dolphin.attack`).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct BiteSound(pub &'static str);

/// The rule that poses `BiteSound` from its params (the sound event name).
pub static BITE: crate::rules::Rule = crate::rules::Rule {
    name: "bite_sound",
    equip: |w, e, p| {
        let p: &dyn std::any::Any = p;
        if let Some(s) = p.downcast_ref::<&'static str>() {
            w.entity_mut(e).insert(BiteSound(s));
        }
    },
    attrs: crate::rules::no_attrs,
    interact: crate::rules::no_interact,
};
