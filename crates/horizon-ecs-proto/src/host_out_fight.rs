//! The End fight's outputs toward the host, queued by `dragon::fight_tick::drain_out` once per tick and moved into `HostEvent::Fight` by the
//! channel (`lots!`): the statistics, the advancements and the boss bar are the host's.
use crate::cow::Cause;
use bevy_ecs::prelude::Resource;

#[derive(Clone, Debug, PartialEq)]
pub enum FightEvent {
    /// A spike crystal was destroyed at `at`; `by` is the player behind it.
    CrystalDestroyed { at: [f64; 3], by: Option<u32> },
    /// The fight's players came in or went out of the arena's reach.
    PlayersChanged { present: bool },
    /// The fight's dragon died: `by` and `last_blow` as the kill had them; `silent` mutes the death.
    Killed { by: Option<u32>, last_blow: Option<Cause>, silent: bool },
    /// The kill is credited to `by`, the last player to hurt the dragon.
    Credit { by: u32, last_blow: Cause, by_player: bool },
    /// The boss bar of the fight goes.
    BarGone,
    /// The fight changed and must be saved.
    Dirty,
}

/// The queue of the fight's outputs, drained by the channel at each step.
#[derive(Resource, Default)]
pub struct FightOut(pub Vec<FightEvent>);
