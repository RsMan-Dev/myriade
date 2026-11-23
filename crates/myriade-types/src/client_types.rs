use crate::*;


/// A note block's instrument (`NoteBlockInstrument`), for `note_block_play`,
/// in the `instrument` property's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NoteInstrument {
    /// The harp, over most blocks.
    Harp,
    /// The bass drum, over stone.
    Basedrum,
    /// The snare drum, over sand and gravel.
    Snare,
    /// The clicks, over glass.
    Hat,
    /// The double bass, over wood.
    Bass,
    /// The flute, over clay.
    Flute,
    /// The bell, over a gold block.
    Bell,
    /// The guitar, over wool.
    Guitar,
    /// The chime, over packed ice.
    Chime,
    /// The xylophone, over a bone block.
    Xylophone,
    /// The iron xylophone, over an iron block.
    IronXylophone,
    /// The cow bell, over soul sand.
    CowBell,
    /// The didgeridoo, over a pumpkin.
    Didgeridoo,
    /// The bit, over an emerald block.
    Bit,
    /// The banjo, over hay.
    Banjo,
    /// The pling, over glowstone.
    Pling,
    /// The trumpet, over a copper block.
    Trumpet,
    /// The trumpet, over an exposed copper block.
    TrumpetExposed,
    /// The trumpet, over an oxidized copper block.
    TrumpetOxidized,
    /// The trumpet, over a weathered copper block.
    TrumpetWeathered,
    /// A zombie's sound, over a zombie head.
    Zombie,
    /// A skeleton's sound, over a skeleton skull.
    Skeleton,
    /// A creeper's sound, over a creeper head.
    Creeper,
    /// The ender dragon's sound, over a dragon head.
    Dragon,
    /// A wither skeleton's sound, over its skull.
    WitherSkeleton,
    /// A piglin's sound, over a piglin head.
    Piglin,
    /// The sound set by a player head's own note block sound.
    CustomHead,
}

impl NoteInstrument {
    /// Every instrument, in the `instrument` property's order.
    pub const ALL: [Self; 27] = {
        use NoteInstrument::*;
        [
            Harp,
            Basedrum,
            Snare,
            Hat,
            Bass,
            Flute,
            Bell,
            Guitar,
            Chime,
            Xylophone,
            IronXylophone,
            CowBell,
            Didgeridoo,
            Bit,
            Banjo,
            Pling,
            Trumpet,
            TrumpetExposed,
            TrumpetOxidized,
            TrumpetWeathered,
            Zombie,
            Skeleton,
            Creeper,
            Dragon,
            WitherSkeleton,
            Piglin,
            CustomHead,
        ]
    };
    /// The `instrument` property's value (`harp`, `cow_bell`).
    pub fn name(self) -> &'static str {
        match self {
            Self::Harp => "harp",
            Self::Basedrum => "basedrum",
            Self::Snare => "snare",
            Self::Hat => "hat",
            Self::Bass => "bass",
            Self::Flute => "flute",
            Self::Bell => "bell",
            Self::Guitar => "guitar",
            Self::Chime => "chime",
            Self::Xylophone => "xylophone",
            Self::IronXylophone => "iron_xylophone",
            Self::CowBell => "cow_bell",
            Self::Didgeridoo => "didgeridoo",
            Self::Bit => "bit",
            Self::Banjo => "banjo",
            Self::Pling => "pling",
            Self::Trumpet => "trumpet",
            Self::TrumpetExposed => "trumpet_exposed",
            Self::TrumpetOxidized => "trumpet_oxidized",
            Self::TrumpetWeathered => "trumpet_weathered",
            Self::Zombie => "zombie",
            Self::Skeleton => "skeleton",
            Self::Creeper => "creeper",
            Self::Dragon => "dragon",
            Self::WitherSkeleton => "wither_skeleton",
            Self::Piglin => "piglin",
            Self::CustomHead => "custom_head",
        }
    }
}

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

/// A mod block's screen over its menu (`screen`): its size, where
/// each of the container's slots is (`Slot.x`, `Slot.y`; none hides the
/// slot, and so do slots past the list; the server's menu does not have a
/// hidden slot either, so clicks and shift-clicks skip it), and the player inventory's first
/// row (vanilla's is 84; the hotbar is 58 below it) and left column, from
/// its top left.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Screen {
    /// Vanilla's container screens are 176 wide.
    pub width: f32,
    /// The screen's height in GUI pixels; vanilla's is 166.
    pub height: f32,
    /// Each menu slot's top-left (x, y), or `None` to hide it.
    pub slots: Vec<Option<[f32; 2]>>,
    /// The y of the player inventory's first row; vanilla's is 84.
    pub inventory_y: f32,
    /// The inventory's left column (vanilla's is 8).
    pub inventory_x: f32,
    /// The mod draws the title and the "Inventory" label itself
    /// (`screen_draw`); Horizon leaves them out.
    pub own_labels: bool,
    /// The mod draws its own background (`screen_draw`), so Horizon
    /// leaves out the plain panel and the slot frames.
    pub own_background: bool,
    /// The registered recipe book hosted beside this menu. Its declarations must
    /// target this menu; no panel is enabled until authoritative settings arrive.
    /// Leaving this unset keeps the screen without a recipe book.
    pub recipe_book: Option<crate::RecipeBookTypeKey>,
    /// Optional 20 × 18 recipe-book toggle position relative to this menu.
    /// The screen author reserves this area, as vanilla screen templates do.
    /// Without an anchor the engine uses an external gutter; if none fits,
    /// the book is refused with a diagnostic while the menu remains usable.
    pub recipe_book_toggle: Option<[f32; 2]>,
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
