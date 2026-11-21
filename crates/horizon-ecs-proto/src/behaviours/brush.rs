//! A brush on a creature that can be brushed (`Armadillo.brushOffScute`, from `mobInteract`): an adult lets fall what `BRUSH_TABLE` rolls,
//! plays the brush sound, sends `entity_interact`, and the brush wears by 16. The rule marks the creature `Brushable` (what the dispenser's
//! brush reads, `dispense::brush_drop`); nothing here names a species.
use crate::behaviours::shear::{adult, drop_stacks};
use crate::cow::Pos;
use crate::dispense::{brush_drop, Brushable};
use crate::goals::Events;
use crate::rules::{no_attrs, Rule};
use bevy_ecs::prelude::*;

const BRUSH: &str = "minecraft:brush";
const WEAR: u32 = 16;

pub static RULE: Rule = Rule {
    name: "brush",
    equip: |w, e, _| {
        w.entity_mut(e).insert(Brushable);
    },
    attrs: no_attrs,
    interact,
};

fn interact(w: &mut World, e: Entity, _: &'static (dyn std::any::Any + Sync), i: &crate::behaviours::Interaction) -> bool {
    if i.item == 0 || !crate::items::is(BRUSH, i.item) || !adult(w, e) {
        return false;
    }
    let Some(at) = w.get::<Pos>(e).map(|p| [p.0.x, p.0.y, p.0.z]) else { return false };
    let stacks = brush_drop(w, e);
    drop_stacks(w, stacks, at);
    crate::behaviours::act_phase::sound(w, e, "minecraft:entity.armadillo.brush", 1.0, 1.0);
    let host = &mut w.resource_mut::<Events>().host;
    host.game_event_by(e, "minecraft:entity_interact", at, false, false, false);
    host.held_worn(i.player, i.item, WEAR);
    true
}
