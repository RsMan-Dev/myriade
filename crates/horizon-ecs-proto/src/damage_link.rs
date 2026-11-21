//! Damage sent to a link: an entity that carries `DamageLink` takes no damage at all. A blow by an attacker is handed to the link instead,
//! once per window; the host answers it (`MiscEvent::LinkedHurt`). Read at the damage event (`health::apply_damage`); nothing counts down,
//! the window is a deadline on the entity's own time.
use crate::cow::Cause;
use crate::host_out_misc::MiscEvent;
use crate::targeting::Who;
use bevy_ecs::prelude::*;

/// `Creaking.INVULNERABILITY_ANIMATION_DURATION`: the ticks after a handed-over blow during which the next ones are refused.
pub const WINDOW: u64 = 8;
/// The client's entity event of a handed-over blow (`Creaking.handleEntityEvent(66)`: the animation and the hurt sound).
pub const EVENT: u8 = 66;

/// Its damage goes to the link. `until` is the end of the window, in the entity's own time (`u64::MAX`: nothing goes through any more).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[component(storage = "SparseSet")]
pub struct DamageLink {
    pub until: u64,
}

/// What the link does with a hit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// `#bypasses_invulnerability`: the entity takes the hit as any other.
    Takes,
    /// No damage, no reaction (window open, or the blow has no attacker).
    Refused,
    /// No damage; the link is hit.
    Handed,
}

/// `Creaking.hurtServer` of a linked creaking, in order: the bypassing type goes through; an open window or a blow without attacker is
/// refused; else it is handed over.
pub fn verdict(cause: Cause, window_open: bool, attacker: bool) -> Verdict {
    if cause == Cause::OutOfWorld {
        Verdict::Takes
    } else if window_open || !attacker {
        Verdict::Refused
    } else {
        Verdict::Handed
    }
}

/// A blow was handed over: the client plays the animation and the host is told, with whether a player answers for the blow.
pub fn handed(w: &mut World, e: Entity, by: Option<Who>) {
    let by_player = match by {
        Some(Who::Player(_)) => true,
        Some(Who::Entity(a)) => crate::credit::player_of(w, a).is_some(),
        None => false,
    };
    let at = w.get::<crate::cow::Pos>(e).map(|p| [p.0.x, p.0.y, p.0.z]);
    let host = &mut w.resource_mut::<crate::goals::Events>().host;
    host.entity_event(e, EVENT);
    // `Creaking.hurtServer`: `gameEvent(ENTITY_ACTION)` right after the entity event. The demolition (`tearDown`) emits none.
    if let Some(at) = at {
        host.game_event_by(e, "minecraft:entity_action", at, false, false, false);
    }
    w.get_resource_or_insert_with(crate::colony_register::MiscOut::default).0.push(MiscEvent::LinkedHurt { entity: e, by_player });
}
