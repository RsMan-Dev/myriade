//! The general entities' outputs of the ECS (`GeneralEvent`), applied by the host's own functions: a mod's tick that was asked for.
//! A click on a general interaction has no host handler (the host's interaction entities are client-side boxes), so it is dropped;
//! a payload change is resent by the tracker's `drain_modified` (game/ecs/tracking.rs), so it needs no arm here.
use super::super::Game;
use horizon_ecs_proto::host_out_general::GeneralEvent;

impl Game<'_> {
    /// Takes the general entities' outputs the ECS queued and applies them, one arm per variant.
    pub(in crate::game) fn ecs_general_events(&mut self, evs: impl IntoIterator<Item = GeneralEvent>) {
        for ev in evs {
            match ev {
                GeneralEvent::ModTick { .. } | GeneralEvent::Clicked { .. } | GeneralEvent::Changed { .. } => {}
            }
        }
    }
}
