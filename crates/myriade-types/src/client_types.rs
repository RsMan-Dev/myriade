use crate::*;


/// The mixer group a sound plays on (`SoundSource`), in vanilla's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SoundSource {
    /// The master volume, which scales every other source.
    Master,
    /// Music discs aside, the game's music.
    Music,
    /// Jukeboxes and music discs.
    Records,
    /// Rain and thunder.
    Weather,
    /// Sounds blocks make.
    Blocks,
    /// Hostile creatures.
    Hostile,
    /// Friendly and neutral creatures.
    Neutral,
    /// Players.
    Players,
    /// Ambient and cave sounds.
    Ambient,
    /// Voice sounds.
    Voice,
    /// Interface sounds, such as a button click.
    Ui,
}

impl SoundSource {
    /// Its position in vanilla's `SoundSource` order, as the network sends it.
    pub fn index(self) -> usize {
        self as usize
    }
    /// Every source, in vanilla's order (`SoundSource.values()`).
    pub const ALL: [SoundSource; 11] = [
        Self::Master,
        Self::Music,
        Self::Records,
        Self::Weather,
        Self::Blocks,
        Self::Hostile,
        Self::Neutral,
        Self::Players,
        Self::Ambient,
        Self::Voice,
        Self::Ui,
    ];
}

/// `ParticleStatus`: how many particles a client shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParticleStatus {
    /// All particles.
    All,
    /// Fewer particles.
    Decreased,
    /// Only the essential particles.
    Minimal,
}

/// One player's open menu, invalidated on close, replacement or reconnect.
/// Client menu handles are also invalidated on respawn.
///
/// Clicks require a mutable client host, not a server or a read-only hook:
/// ```compile_fail
/// use mod_api::{MenuRef, ServerHost};
/// fn wrong(menu: MenuRef, host: &mut dyn ServerHost) {
///     menu.click(host, &[]);
/// }
/// ```
/// ```compile_fail
/// use mod_api::{MenuRef, ClientHost};
/// fn wrong(menu: MenuRef, host: &dyn ClientHost) {
///     menu.click(host, &[]);
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MenuRef {
    pub player: PlayerRef,
    pub window: i32,
    pub generation: u64,
}
