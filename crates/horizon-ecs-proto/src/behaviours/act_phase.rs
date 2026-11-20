//! The timed state of a species that alternates between the surface and a burrow: the phase, the end of the timed act, the cooldowns
//! (own-time deadlines, `cow::now_of`: nothing counts down each tick), and the state written rarely (`RareState`), kept apart so
//! that the common path reads one small component. `deadline` ends a timed act at its wheel deadline.
use crate::{cow::*, goals::*, targeting::Who};
use bevy_ecs::prelude::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    #[default]
    Standing,
    /// Coming out of the ground (the `emerging` act).
    Emerging,
    /// In the ground, or going in (`digging` act).
    Burrowing,
    Roaring,
    Sniffing,
    /// The beam of the area attack.
    Booming,
}

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct ActPhase {
    pub phase: Phase,
    /// The end of the timed act running: exclusive activities share one value.
    pub until: u64,
    pub dig_cooldown_until: u64,
    pub sniff_cooldown_until: u64,
    pub sonic_cooldown_until: u64,
    pub vibration_cooldown_until: u64,
    pub recent_projectile_until: u64,
    /// The boom of the area attack has been drawn for this act.
    pub boomed: bool,
}

/// Written by one event each and read by one activity: a pending emerge, the roar's target, the place of the last vibration kept.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct RareState {
    pub emerge_pending: bool,
    pub roar_target: Option<Who>,
    pub disturbance: Option<[i32; 3]>,
    pub disturbance_until: u64,
}

/// A sound at the entity's place, through the host queue.
pub fn sound(w: &mut World, e: Entity, name: &'static str, volume: f32, pitch: f32) {
    let p = w.get::<Pos>(e).unwrap().0;
    w.resource_mut::<Events>().host.sound(name, [p.x, p.y, p.z], volume, pitch);
}

/// The deadline of a timed act came up: a goal that runs is ended (false: nothing starts now), one that does not may start (true).
pub fn deadline(w: &mut World, e: Entity, m: &'static crate::behaviours::Module) -> bool {
    if w.get::<GoalSet>(e).is_some_and(|g| g.is_running(m)) {
        end(w, e, m);
        false
    } else {
        true
    }
}
