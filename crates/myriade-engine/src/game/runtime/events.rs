//! What happened in the game, for agents (`events`, `wait-for`; docs/agent-control.md): the
//! state is compared once per event loop turn, after the game ran, so watching never changes
//! what the game does.

#[derive(Default)]
pub(super) struct Events {}
