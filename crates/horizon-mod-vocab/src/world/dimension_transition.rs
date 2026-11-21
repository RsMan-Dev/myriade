//! Client loading-screen registrations and their presentation data.

/// A screen registered by `Hooks::register_dimension_transition_screen`.
#[derive(Clone, Debug, PartialEq, Eq)]
// exhaustive: mods construct registrations and the game reads both fields
#[derive(horizon_mod_api_macros::Wire)]
pub struct DimensionTransitionScreen {
    /// Namespaced screen id, drawn by `Hooks::dimension_transition_screen_draw`.
    pub screen: String,
    /// Which travel selects this screen.
    pub transition: DimensionTransition,
}

/// NeoForge's incoming, outgoing and conditional registration variants.
#[derive(Clone, Debug, PartialEq, Eq)]
// exhaustive: the three registration variants
#[derive(horizon_mod_api_macros::Wire)]
pub enum DimensionTransition {
    /// Travel to this dimension (`registerIncomingEffect`).
    Incoming(String),
    /// Travel from this dimension (`registerOutgoingEffect`).
    Outgoing(String),
    /// Travel to `to` from `from` (`registerConditionalEffect`), highest priority.
    Conditional {
        /// Destination dimension id.
        to: String,
        /// Source dimension id.
        from: String,
    },
}

/// Why the client is displaying its level-loading screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
// exhaustive: LevelLoadingScreen.Reason's three variants
#[derive(horizon_mod_api_macros::Wire)]
pub enum DimensionTransitionReason {
    /// Travel involving the Nether, except a death respawn.
    NetherPortal,
    /// Travel involving the End, except a death respawn or Nether travel.
    EndPortal,
    /// Login, death respawn, or other travel.
    Other,
}
