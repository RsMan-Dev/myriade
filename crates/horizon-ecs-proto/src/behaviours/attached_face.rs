//! `attached_face`: a body fixed to a face of a block: its face (0..6, the first is down), its dye colour (`Sounds`' config: `none` is a
//! colour the species names), the raw peek set by the goals and the eased lid. The lid eases `LID_SPEED` a tick toward raw peek / 100, and
//! the lid's sweep pushes what it opens into (`cow::push_lid`). Covered armour: a +20 modifier while raw peek is 0 (`attributes`, socle).
use crate::cow::Pos;
use super::*;
use crate::goals::Events;
use crate::rules::Rule;

/// The lid's rate of easing per tick, the same for every lid.
pub const LID_SPEED: f32 = 0.05;

#[derive(Component, Clone, Copy, Debug)]
pub struct Attached {
    pub face: u8,
    pub color: u8,
    pub raw: i32,
    pub eased: f32,
}

/// The sounds of a lid's raw change, named by the species.
#[derive(Clone, Copy, Debug)]
pub struct Sounds {
    pub open: &'static str,
    pub close: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The colour a body starts with (the species' idle value).
    pub colour: u8,
    /// Its armour modifier while shut (`minecraft:covered`, +20 AddValue).
    pub covered: bool,
}

pub static RULE: Rule = Rule { name: "attached_face", equip, attrs: crate::rules::no_attrs, interact: crate::rules::no_interact };

fn equip(w: &mut World, e: Entity, p: &'static (dyn std::any::Any + Sync)) {
    let colour = params::<Params>(p).colour;
    w.entity_mut(e).insert(Attached { face: 0, color: colour, raw: 0, eased: 0.0 });
}

/// The raw peek moves to `amount`, with the lid's sound (every call, as in vanilla), and the covered modifier follows a shut lid.
pub fn set_peek(w: &mut World, e: Entity, amount: i32, sounds: &Sounds, covered: bool) {
    let Some(mut a) = w.get::<Attached>(e).copied() else { return };
    a.raw = amount;
    w.entity_mut(e).insert(a);
    if covered {
        crate::attributes::set_armor_modifier(w, e, "minecraft:covered", (amount == 0).then_some(20.0));
    }
    let at = w.get::<Pos>(e).map(|p| [p.0.x, p.0.y, p.0.z]).unwrap_or([0.0; 3]);
    let name = if amount == 0 { sounds.close } else { sounds.open };
    w.resource_mut::<Events>().host.sound(name, at, 1.0, 1.0);
}

/// The lid eases toward its raw peek; the sweep pushes what it opens into.
pub fn ease(mut lids: Query<(Entity, &mut Attached)>, mut commands: Commands) {
    for (e, mut a) in &mut lids {
        let target = a.raw as f32 * 0.01;
        let old = a.eased;
        let new = if old > target { (old - LID_SPEED).max(target) } else { (old + LID_SPEED).min(target).max(0.0) };
        if new != old {
            a.eased = new;
            commands.queue(move |w: &mut World| crate::cow::push_lid(w, e, old, new));
        }
    }
}

/// `getPhysicalPeek`: how far the lid stands from the body's wall.
pub fn physical_peek(eased: f32) -> f32 {
    0.5 - ((0.5 + eased) * std::f32::consts::PI).sin() * 0.5
}

