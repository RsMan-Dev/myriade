//! The playable game loop: streaming world, player simulation at 20 ticks/s,
//! block interaction, inventory and HUD.
use key_codes::{
    container_key_clones, drop_held_behind_dialog, follow_named_keymap, gameplay_has_keyboard,
    glfw_button, glfw_modifiers, keyboard_button,
};
use loading_screen::{
    App, Loading, leave_fullscreen_after, missing_resources, run_event_loop, unclean_start_recovery,
};
use motion_ids::{motion_at, motion_block_entity};
use resource_load::{
    Progress, launch_world, load_resources, named_scope, owns_device_loss_marker,
    synchronous_startup,
};
/// The content tables the world plays with, as field-level borrows: the
/// world's data packs' over the game's own. The `Game::items`/`recipes`/
/// `stations` methods read the same tables, but taking `&self` blocks a
/// `&mut self.…` in the same scope; these expand to a match on the fields,
/// which borrows stay disjoint from.
macro_rules! world_items {
    ($s:ident) => {
        match (&$s.server_items, &$s.world_tables) {
            (Some(items), _) => &**items,
            (None, Some(t)) => &t.items,
            (None, None) => $s.res.items(),
        }
    };
}
macro_rules! world_recipes {
    ($s:ident) => {
        match &$s.world_tables {
            Some(t) => &t.recipes,
            None => $s.res.recipes(),
        }
    };
}
macro_rules! world_stations {
    ($s:ident) => {
        match &$s.world_tables {
            Some(t) => &t.stations,
            None => $s.res.stations(),
        }
    };
}
/// The biome table the world plays with (`Game::biomes`), as a field-level
/// borrow: the world's packs' over the game's own.
macro_rules! world_biomes {
    ($s:expr) => {
        match &$s.world_biomes {
            Some(b) => b,
            None => $s.res.biomes(),
        }
    };
}
/// The world's packs' content files, as a field-level borrow (see the macros).
fn pack_files(world_tables: &Option<WorldTables>) -> horizon_world::loot::PackOverlay<'_> {
    match world_tables {
        Some(t) => horizon_world::loot::PackOverlay {
            conditions: &t.conditions,
            files: &t.files,
        },
        None => horizon_world::loot::PackOverlay::NONE,
    }
}
include!(concat!(env!("OUT_DIR"), "/game_modules.rs"));
/// Installs the dispatcher of each injection point a mod listens at as its function's entry,
/// once the points are sealed. The prebuilt game's staging pass writes the call; a build without
/// points has none to arm.
macro_rules! arm_points {
    () => {};
}

/// The game's side of the injection points (arismoko/horizon#589), whose slots and listeners live above the engine.
#[doc(hidden)]
pub mod points {
    /// A `side = "both"` point's host, out of line: each point's listener call stays a few bytes.
    /// Called only by the `#[point]`s the stage adds to its copy.
    #[inline(never)]
    #[allow(dead_code, reason = "used by the staged copy's points")]
    pub(super) fn host_side<'a>(game: &'a mut super::Game<'_>) -> mod_api::HostSide<'a> {
        super::execution_role::host_side(game)
    }

    /// An engine value's `mod_api` mirror for a point record, through the boundary's conversion.
    #[allow(dead_code, reason = "used by the staged copy's points")]
    pub(super) fn mirror_in<T: super::api_values::TryIntoApi + ?Sized>(
        value: &T,
    ) -> Option<T::Api> {
        value.try_into_api().ok()
    }

    /// A listener's `mod_api` value as the engine's, validated as every mod value is; a value the
    /// engine refuses leaves the engine's own in place.
    #[allow(dead_code, reason = "used by the staged copy's points")]
    pub(super) fn mirror_out<A: super::api_values::TryIntoEngine<()>>(
        value: &A,
        point: &str,
    ) -> Option<A::Engine> {
        let engine = value.try_into_engine(&()).ok();
        if engine.is_none() {
            log::warn!(
                "point {point}: a listener's value has no engine form; the engine's is kept"
            );
        }
        engine
    }

    /// An engine value with no `mod_api` form at this call: its listeners are skipped this once.
    #[cold]
    #[allow(dead_code, reason = "used by the staged copy's points")]
    pub(super) fn unconverted(point: &str) {
        log::warn!("point {point}: a value has no API form; its listeners were skipped");
    }
}
mod ecs;
mod host;
mod native_hooks;
use event_loop::EventLoopCtl;
use hand::interact_location;
pub(crate) use hit::Hit;
use horizon_client::random_source;
use loading_window::{
    build_renderer, loading_frame, loading_renderer, prepare_renderer_textures, start_game,
};
use resource_loader::{
    EntityModels, GameCpuResources, Loaded, LoaderMsg, PendingWorldEntry, WorldRestLoader,
    run_loading, write_report,
};
use window_handler::ClientQuit;
// `Action` here is the title screen's buttons (`title::Action`).
use self::api_values::TryIntoApi;
pub(crate) use api_values::{block_jump_factor, block_speed_factor, is_ladder};
use horizon_client::actions::Action as Mapping;
use horizon_client::spans::{startup, time as span};
use horizon_client::{
    Result,
    assets::{Assets, RenderAssets},
    audio::Audio,
    biome::Biomes,
    block_entity_render::{
        self, BlockEntityRenderer, CacheOptions, Scene as BlockEntityScene, StaticCacheFrameStats,
        StaticSectionSnapshot,
    },
    blocks::{BlockFacts, Fluid},
    clock::FixedStep,
    collisions,
    containers::{BlockEntity, Providers},
    crafting::Recipes,
    creative::{Creative, Names},
    entities::{Experience, ExperienceOrb, ItemEntity},
    environment::Environment,
    first_person::{Hand, SwingAnimation},
    furnace::FurnaceCooking,
    gfx::{Frame, Renderer},
    graphics::EntityCullingMode,
    hud::{HudAtlas, HudBatch, rgba},
    hud_layout, hud_overlays,
    items::{ItemStack, Items},
    keymapping::{KeyAction, SNEAK, SPRINT},
    light::LightTables,
    mesher::{MeshTables, Vertex},
    mobs::{Mob, MobType, MobTypes},
    models,
    motion::{MotionResources, PlayerMotion},
    mouse::{LookOptions, MouseLook},
    particles::Particle,
    save::{Level, Persistent, SaveDir},
    settings::Settings,
    spans,
    sprint::Keys,
    streaming::{Streaming, WorkerContext},
    testgen::TestTerrain,
    title::{Action, Title},
    vanillagen::VanillaTerrain,
    world::World,
};
use horizon_client::{fast_hash::FxHashMap, held::Source};
use horizon_core::{
    biome::{BiomeZoom, obfuscate_seed},
    geometry::{Aabb, Direction},
    position::BlockPos,
};
use horizon_core::{geometry::Vec3, random::RandomSource};
use horizon_mod_package::selection::{Argument, LaunchArguments};
pub(crate) use launch_options::Options;
pub use native_block_entity_draw::BlockEntityDraw;
pub use native_claimed_items::{HeldItemSlot, ModEvent};
pub use native_components::NativeComponentError;
pub use native_configs::ConfigFile;
pub use native_data_maps::DataMapType;
pub use native_hooks::{PhantomSpawnResult, Shake, SleepProblem};
pub use native_mob_draw::{MobRender, PlayerRender};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
        mpsc::{Receiver, TryRecvError},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    error::EventLoopError,
    event::{DeviceEvent, DeviceId, ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{CursorGrabMode, Window, WindowId},
};
use {
    horizon_core::dimension::Dimension as LevelDimension,
    horizon_world::{
        block_collision::BlockCollisionRegistry,
        collision_context::{CollisionContext, CollisionEntity, CollisionItem},
        entity_move::MoverType,
        entity_obstruction::{EntityIdentity, EntityObstructionWorld, ObstructionEntity},
        fluid::FluidState,
        level::{
            Behaviours, BucketContent, EntityQuery, Landing, LevelAccess, LevelEvent, PlaceContext,
            PlaceOutcome, PlacementContext, PlayerContext, PlayerEffect, RandomTickMaskUpdate,
            Ticks, flags as level_flags,
        },
        registry::BlockStateId,
    },
};

/// A frame's finished meshes up to this many upload at once, unqueued.
const AUTO_UPLOADS_AT_ONCE: usize = 8;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Screen {
    Playing,
    GameModeSwitcher,
    /// Horizon's emote wheel (`game/cosmetics/emote_wheel.rs`): over the world, not pausing.
    EmoteWheel,
    Paused,
    Options,
    Chat,
    Creative,
    Inventory,
    Dead,
    /// The Statistics screen.
    Stats,
    /// The Advancements screen.
    Advancements,
    /// WorldOptionsScreen (game/network/lan_screen.rs).
    WorldOptions,
    /// Play with Friends, from the pause menu's Friends button
    /// (`friends_screen`, game/network/friends.rs).
    Friends,
    /// `SocialInteractionsScreen` (`social_screen`, game/network/social.rs).
    Social,
    /// CommandBlockEditScreen (game/administration/command_blocks.rs).
    CommandBlock,
    /// A game master block's edit screen (game/interface/block_editor.rs).
    BlockEditor,
    /// ServerLinksDialogScreen: a server's links (game/network/server_links.rs).
    ServerLinks,
    /// ConfirmLinkScreen for a server link (game/network/server_links.rs).
    ConfirmLink,
    /// DialogScreen.WarningScreen: the note about custom screens.
    DialogWarning,
    /// WinScreen: the end poem and the credits (game/interface/win.rs).
    Win,
    /// DebugOptionsScreen, F3+F6 (game/interface/debug_options.rs).
    DebugOptions,
    /// A screen of a native mod's own (`mod_screen_open`): it draws and
    /// takes its input through the screen hooks.
    Mod,
    /// PvP & HUD's widget editor (game/interface/hud_editor.rs).
    HudEditor,
    /// Horizon Studio script screen (`studio_screen_open`): canvas draws
    /// itself, takes input through the screen hooks, does not draw chat.
    Studio,
}
impl Screen {
    /// The id native mods see for this screen (`ScreenView.id`, the
    /// `screen_render_post` hook): one per variant, and a hook's contract, so
    /// it stays the same from release to release. A new variant must add one.
    fn id(self) -> &'static str {
        match self {
            Screen::Playing => "playing",
            Screen::GameModeSwitcher => "game_mode_switcher",
            Screen::EmoteWheel => "emote_wheel",
            Screen::Paused => "paused",
            Screen::Options => "options",
            Screen::Chat => "chat",
            Screen::Creative => "creative",
            Screen::Inventory => "inventory",
            Screen::Dead => "dead",
            Screen::Stats => "stats",
            Screen::Advancements => "advancements",
            Screen::WorldOptions => "world_options",
            Screen::Friends => "friends",
            Screen::Social => "social",
            Screen::CommandBlock => "command_block",
            Screen::BlockEditor => "block_editor",
            Screen::ServerLinks => "server_links",
            Screen::ConfirmLink => "confirm_link",
            Screen::DialogWarning => "dialog_warning",
            Screen::Win => "win",
            Screen::DebugOptions => "debug_options",
            Screen::Mod => "mod",
            Screen::HudEditor => "hud_editor",
            Screen::Studio => "studio",
        }
    }
}
/// The vanilla `state` a `transformer` table entry offers to `ability`: hoe and
/// shovel tables have one action each; the axe's strip, scrape and wax-off lists
/// are one untagged list in `BlockTransformers.bootstrap`, so its particle names it.
fn vanilla_tool_state(
    transformer: &str,
    particle: &str,
    ability: mod_api::ToolAbility,
    state: BlockStateId,
) -> Option<BlockStateId> {
    use mod_api::ToolAbility::*;
    let action = match transformer {
        "minecraft:axe" => match particle {
            "scrape" => AxeScrape,
            "wax_off" => AxeWaxOff,
            _ => AxeStrip,
        },
        "minecraft:shovel" => ShovelFlatten,
        "minecraft:hoe" => HoeTill,
        _ => return None,
    };
    (action == ability).then_some(state)
}

struct Mining {
    pos: [i32; 3],
    progress: f32,
    /// The latest destroy-speed query, reused for server crack feedback.
    per_tick: f32,
    ticks: u32,
}
impl Mining {
    fn stage(&self) -> Option<u8> {
        (self.progress > 0.0).then(|| ((self.progress * 10.0) as u8).min(9))
    }
}
#[derive(Clone, Copy)]
struct TransformContext<'a> {
    world: &'a World,
    player: Option<u32>,
    hand: mod_api::Hand,
    blocking: bool,
    simulated: bool,
}

/// What a falling block's tick did (`FallingBlockEntity.tick`).
enum FallStep {
    Falling,
    Gone,
    /// Gave up falling: drops itself as an item here.
    Dropped(Vec3),
    /// Hit the ground in this block.
    Landed([i32; 3]),
}
/// A guest's right click comes as one packet per hand and part
/// (`Minecraft.startUseItem`): `ServerGamePacketListenerImpl.handleUseItemOn`
/// does the block's and `ItemStack.useOn`'s part, `handleUseItem` the
/// item's own (`Item.use`). Running both for either would use the item twice.
#[derive(Clone, Copy)]
enum GuestUse {
    OnBlock(Hand),
    Item(Hand),
    /// `handleInteract`: `Player.interactOn` with the hand, nothing else (the
    /// client sends UseItem itself when the entity passed), on the entity the
    /// packet named: `ServerGamePacketListenerImpl.handleInteract` resolves
    /// `packet.entityId()` once and range checks it, so the click never picks
    /// an entity again from the view.
    Entity(Hand, InteractTarget),
}
/// The entity an interact packet names and where its client clicked it (the
/// packet's location, measured from the entity's position): what
/// `ServerGamePacketListenerImpl.handleInteract` takes from
/// `ServerboundInteractPacket` and hands to `Player.interactOn`, which passes
/// the point on to `interactAt`.
#[derive(Clone, Copy)]
struct InteractTarget {
    id: i32,
    location: [f64; 3],
}

/// The client world as seen by the vanilla block-behaviour library.
struct GameLevel<'w> {
    cake_probe: bool,
    cake_requested: std::cell::Cell<bool>,
    cake_answer: Option<bool>,
    api_level: Option<mod_api::LevelRef>,
    positions: piston_carry_state::PositionState<'w>,
    border: &'w horizon_core::geometry::border::BorderGeometry,
    world: &'w mut World,
    command_block_transition: Option<command_blocks::CommandBlockTransition>,
    redstone_experiments: bool,
    facts: &'w BlockFacts,
    collision: &'w BlockCollisionRegistry<'w>,
    events: &'w mut Vec<LevelEvent>,
    dimension: LevelDimension,
    darken: i32,
    /// The environment of the level's dimension (`SUN_ANGLE`).
    environment: &'w Environment,
    raining: bool,
    thundering: bool,
    /// The world's enabled feature flags as this side knows them (`LevelReader.enabledFeatures`).
    enabled_features: &'w std::sync::Arc<[String]>,
    player_box: Option<([f64; 3], [f64; 3])>,
    /// The LAN guests in this level who trigger blocks (not spectators).
    guest_boxes: Vec<([f64; 3], [f64; 3])>,
    /// Other players that block placement during a guest turn, including live
    /// ancestor bodies; None outside one, where `guest_boxes` are those players.
    building_player_boxes: Option<Vec<([f64; 3], [f64; 3])>>,
    player_collision: Option<PlayerCollision>,
    /// The level's ECS: the minecarts, arrows, hooks and item frames the block
    /// triggers and comparators read, by zone; inert on a client.
    ecs: &'w mut horizon_client::ecs_bridge::EcsLevel,
    /// Every entity on the walk; see `entity::for_each_entity`.
    other_entities: entity::EntityLists<'w>,
    /// End crystals, which aren't on the walk yet.
    crystals: &'w [dragon::Crystal],
    /// The lightning bolts here: none ignores block triggers
    /// (`Entity.isIgnoringBlockTriggers`).
    trigger_extras: Vec<Aabb>,
    biomes: &'w Biomes,
    seed: i64,
    behaviours: &'w Behaviours,
    items: &'w Items,
    /// Cooking recipes' experience, which a removed furnace pays out.
    recipes: &'w horizon_client::crafting::Recipes,
    /// Container loot tables (`unpackLootTable`).
    loot: Option<std::sync::Arc<horizon_world::loot::LootData>>,
    registry: &'static horizon_world::registry::BlockRegistry,
    block_registries: &'w horizon_client::command::args::Registries,
    rules: horizon_world::level::GameRules,
    /// `Difficulty.getId()` (peaceful 0 .. hard 3).
    difficulty: i32,
    /// The feet of the players who keep fire spreading around them: the local
    /// player and the LAN guests in this level, spectators aside.
    fire_players: Vec<[f64; 3]>,
    /// Block loot random sequences (`ServerLevel.getRandomSequence`).
    rand_sequences: &'w mut horizon_client::random_sequences::RandomSequences,
    pending: &'w mut spill::Pending,
    spill: &'w mut spill::Spill,
    /// Where the chunk tickets let blocks tick.
    tickets: horizon_client::ticket_levels::Levels,
    random_tick_sections: &'w mut horizon_client::random_tick_sections::RandomTickSections,
    /// Players with each chest open (a trapped chest's signal), also what a
    /// recount of the openers replaces (`ContainerOpenersCounter.recheckOpeners`).
    lid_openers: &'w mut HashMap<(i32, i32, i32), i32>,
    /// The containers players have open at this moment, by position: the local
    /// player's and the guests' (`getEntitiesWithContainerOpen`). A double
    /// chest is in it as both halves.
    open_containers: &'w [(i32, i32, i32)],
    /// The chests copper golems have open, a double chest as both halves. They
    /// keep their own count (`chest_openers`), so they stay out of `open_containers`.
    golem_chests: &'w [(i32, i32, i32)],
    /// `Level.isClientSide`: a client's copy of the level (a guest of a LAN
    /// world, a window on its server thread, a dedicated server's client).
    client_side: bool,
    /// Only the integrated or dedicated server dispatches native observations.
    native_authoritative: bool,
    native_player: Option<u128>,
    /// The acting player's entity id, beside `native_player`.
    native_player_entity: Option<i32>,
    /// `Level.isDebug`: `setBlock` changes nothing.
    debug: bool,
    native_sounds: &'w mut Vec<creatures::NativeSound>,
    loop_context: &'w horizon_world::native_load_context::LoopContext,
    native_particles: &'w mut Vec<lan_particles::QueuedParticle>,
    sound_seeds: Option<&'w horizon_client::lan::hub::Hub>,
    drop_entity: Option<mod_api::EntityRef>,
    /// The block a mod's machine breaks and the tool it breaks it with
    /// (`native_machine_destroy_block`): `Block.dropResources` rolls that
    /// block's loot with the tool instead of with none. Neighbours that pop
    /// off because of the break drop as with no tool, as in vanilla.
    drop_tool: Option<((i32, i32, i32), Option<ItemStack>)>,
    /// Only this destroyed block has an entity as its loot `this` parameter.
    /// Neighbour/default/machine drops keep the no-entity context.
    drop_breaker_at: Option<BlockPos>,
    /// Where `Block.dropResources` puts the drops instead of popping them,
    /// while a mod's machine breaks a block (`native_machine_destroy_block`).
    drop_into: Option<Vec<ItemStack>>,
}
struct PlayerCollision {
    y: f64,
    descending: bool,
    fall_distance: f64,
    main_hand_item: Arc<str>,
    main_hand_count: i32,
    leather_boots: bool,
}
/// The boxes of the entities that block building, as obstruction candidates.
struct BuildingObstruction(Vec<Aabb>);
impl EntityObstructionWorld for BuildingObstruction {
    type Error = std::convert::Infallible;
    fn obstruction_candidates(
        &mut self,
        _excluded: Option<EntityIdentity>,
        bounds: Aabb,
    ) -> std::result::Result<Vec<ObstructionEntity>, Self::Error> {
        Ok(self
            .0
            .iter()
            .filter(|b| b.intersects(bounds))
            .enumerate()
            .map(|(i, &bounds)| ObstructionEntity {
                identity: EntityIdentity(i as u64),
                root_vehicle: EntityIdentity(i as u64),
                bounds,
                removed: false,
                spectator: false,
                blocks_building: true,
            })
            .collect())
    }
}
/// Whether a block's successful use is `InteractionResult.SUCCESS_SERVER`,
/// which a client does not predict: `useWithoutItem` of a bed, a light
/// block, a charged respawn anchor or a sign. Every other block use the
/// level answers with SUCCESS is `SUCCESS` (`SwingSource.PREDICTED`).
fn server_only_block_use(
    block: &horizon_items::blocks::StateFacts,
    branch: horizon_world::level::UseBranch,
) -> bool {
    branch == horizon_world::level::UseBranch::WithoutItem
        && [
            "AbstractBedBlock",
            "LightBlock",
            "RespawnAnchorBlock",
            "SignBlock",
        ]
        .iter()
        .any(|class| block.has_class(class))
}
impl GameLevel<'_> {
    /// How many entities in `area` press plates and tripwires, or with
    /// `LIVING` only the living ones; a constant so a list of other kinds
    /// compiles to no loop at all.
    fn count_triggers<const LIVING: bool>(&self, area: Aabb) -> usize {
        use entity::{Entity as _, for_each_entity};
        let mut count = 0;
        for_each_entity!(self.entities(), entity => {
            if (!LIVING || entity.is_living())
                && entity.triggers_blocks()
                && area.intersects(entity.bounds())
            {
                count += 1;
            }
        });
        count
    }
    /// Every entity box that blocks building: the players who aren't
    /// spectators, the minecarts and the `Blockers` (a removed mob or a marker
    /// armor stand, whose box is empty, doesn't).
    fn building_blocker_boxes(&self, zone: Aabb) -> Vec<Aabb> {
        let boxed = |(lo, hi): &([f64; 3], [f64; 3])| {
            Aabb::new(
                Vec3::new(lo[0], lo[1], lo[2]),
                Vec3::new(hi[0], hi[1], hi[2]),
            )
        };
        let b = self.entities();
        self.player_box
            .iter()
            .chain(
                self.building_player_boxes
                    .as_ref()
                    .unwrap_or(&self.guest_boxes),
            )
            .map(boxed)
            .chain(self.ecs.cart_bounds_in(zone))
            .chain(b.mobs.iter().filter(|m| !m.removed).map(Mob::bounds))
            .chain(
                b.armor_stands
                    .iter()
                    .filter(|s| !s.marker)
                    .map(|s| s.bounds()),
            )
            .chain(b.boats.iter().map(boats::Boat::bounds))
            .chain(self.crystals.iter().map(dragon::Crystal::bounds))
            .chain(b.tnts.iter().map(tnt::PrimedTnt::bounds))
            .chain(
                b.falling
                    .iter()
                    .map(|f| horizon_client::falling_block::bounds(f.position)),
            )
            .collect()
    }
}
/// A landing's block game event, emitted after its world write is applied.
struct LandingVibration {
    event: &'static str,
    position: BlockPos,
    state: BlockStateId,
    from_entity: bool,
}

/// FallingBlockEntity carrying a block state.
#[derive(Clone, Debug)]
struct FallingBlock {
    attachment_lifetime: horizon_mod_runtime::attachments::AttachmentLifetime,
    /// `Entity.yRot` and `xRot` (`/tp`, `/data get ... Rotation`).
    pub rotation: [f32; 2],
    /// When it joined its list (`horizon_client::join_seq`); set by the list.
    pub(super) join: horizon_client::join_seq::JoinSeq,
    /// Its entity id: from the entity counter on the server, the server's on a client.
    id: u32,
    state: BlockStateId,
    /// `DATA_START_POS`: spawn position used to select weighted block models.
    start_pos: BlockPos,
    position: Vec3,
    previous: Vec3,
    velocity: Vec3,
    /// `FallingBlockEntity.time`: signed, a save can start it below zero.
    age: i32,
    /// `Entity.isNoGravity`.
    no_gravity: bool,
    /// `FallingBlockEntity.tick` handles portals after moving, before landing.
    portal: horizon_client::nether_portal::PortalState,
    /// `FallingBlockEntity.dropItem`.
    drop_item: bool,
    /// `Entity.fallDistance` (powder snow holds a block that fell far at 0.9).
    fall_distance: f64,
    /// `Entity.onGround`: whether its last move was stopped going down.
    on_ground: bool,
    /// `Entity.stuckSpeedMultiplier`: a cobweb it is in scales its next move.
    stuck: Vec3,
    damage: horizon_client::falling_block::Damage,
    /// The UUID and compound it was loaded with, kept while it moves.
    saved: Option<Box<([i32; 4], horizon_core::nbt::Tag)>>,
}
horizon_mod_runtime::impl_attachment_lifetime!(FallingBlock);

impl FallingBlock {
    /// Where `FallingBlockRenderer` draws the block's model corner, `alpha`
    /// into the tick: centred on the entity, then moved by `offset`, the
    /// block's random model offset (`ModelBlockRenderer.tesselateBlock`:
    /// `BlockState.getOffset` of `BlockPos.containing(x, maxY, z)`), so a
    /// pointed dripstone keeps the place it hung at while it falls.
    fn model_origin(
        &self,
        alpha: f64,
        offset: impl Fn(BlockStateId, i32, i32) -> Option<[f64; 3]>,
    ) -> glam::DVec3 {
        let [dx, dy, dz] = offset(
            self.state,
            self.position.x.floor() as i32,
            self.position.z.floor() as i32,
        )
        .unwrap_or([0.0; 3]);
        glam::DVec3::new(
            self.previous.x + (self.position.x - self.previous.x) * alpha - 0.5 + dx,
            self.previous.y + (self.position.y - self.previous.y) * alpha + dy,
            self.previous.z + (self.position.z - self.previous.z) * alpha - 0.5 + dz,
        )
    }
}

/// `FallingBlockRenderer.shouldRender` drops a falling block whose own state is
/// the state of the cell it is in (`level.getBlockState(entity.blockPosition())`):
/// a `NoGravity` block, or one that has just landed, sits inside an identical
/// block, and vanilla draws no model for it.
fn falling_block_visible(state: BlockStateId, cell: BlockStateId) -> bool {
    state != cell
}

/// The data loaded once at startup and never changed: blocks, items,
/// recipes, biomes, mobs, collision and motion. Read-only and `Send + Sync`,
/// so the client and an integrated server thread can share one copy.
pub struct Resources {
    core: WorldCore,
    rest: std::sync::OnceLock<WorldRest>,
    /// Immutable reach flags share the lifetime of these facts, including across a resource reload.
    block_entity_reach: std::sync::OnceLock<Arc<[u8]>>,
}

struct WorldCore {
    item_data: Arc<horizon_world::item_defaults::ItemData>,
    facts: BlockFacts,
    biomes: Arc<Biomes>,
    environment: Environment,
    collision: Arc<BlockCollisionRegistry<'static>>,
    motion: MotionResources,
    suffocation: horizon_world::suffocation::SuffocationRegistry<'static>,
    spawn_registries: horizon_client::mobs::finalize::SpawnRegistries,
    threads: usize,
}

struct WorldRest {
    items: Items,
    recipes: Recipes,
    transformers: horizon_client::block_transform::Transformers,
    mob_types: HashMap<String, Arc<MobType>>,
    mob_data: horizon_client::mobs::MobData,
    providers: Providers,
    block_entity_render: BlockEntityRenderer,
    animate: horizon_client::animate_tick::AnimateTables,
    fog: horizon_client::fog::FogTables,
    music: horizon_client::music::MusicTables,
    ambient: horizon_client::ambience::AmbientTables,
    nether_environment: Environment,
    end_environment: Environment,
    behaviours: Behaviours,
    /// The player model's right and left arms.
    player_arm: [Option<horizon_client::mobs::Part>; 2],
    /// The slim model's right and left arms (3 pixels wide).
    player_arm_slim: [Option<horizon_client::mobs::Part>; 2],
    player_type: Option<Arc<horizon_client::mobs::MobType>>,
    /// `SpinAttackEffectModel` (Riptide's spinning boxes).
    spin_attack: Option<horizon_client::mobs::Part>,
    stations: horizon_client::stations::StationData,
}

/// The content tables a world's data packs change, loaded when the world
/// opens and on `/reload`: `ReloadableServerResources` loads a server's
/// recipes, loot tables and tags from the world's pack stack, over the
/// resource root's own. Worlds without packs use `Resources.rest` directly.
struct WorldTables {
    conditions: horizon_world::conditions::LoadConditions,
    suffocation: horizon_world::suffocation::SuffocationRegistry<'static>,
    /// The pack files the tables were built from (a native mod's among
    /// them), for the advancement tree reload and the loot data.
    files: Vec<(String, Vec<Vec<u8>>)>,
    items: Arc<Items>,
    recipes: Recipes,
    stations: horizon_client::stations::StationData,
    reload_sources: Option<Arc<horizon_client::crafting::ResolvedSources>>,
    /// The values of the data map types native mods registered (`native_data_map_types`).
    data_maps: native_data_maps::DataMaps,
    /// The entity type, biome, structure and damage type tags with the packs'
    /// over the game's own, by registry (`TagLoader.loadTagsForExistingRegistries`):
    /// what the predicates' `#tag`s of those registries read.
    pack_tags: HashMap<&'static str, HashMap<String, HashSet<String>>>,
}

struct PreparedParts {
    block_states: Arc<api_values::block_state::Index>,
    tags: HashMap<String, HashSet<mod_api::BlockKey>>,
    parts: Option<mod_api::Parts>,
    cauldrons: Arc<cauldron_interactions::Table>,
}
use condition_reload_state::PreparedWorldReload;

impl WorldTables {
    /// The registries [`WorldTables::pack_tags`] holds.
    const PACK_TAG_REGISTRIES: [&'static str; 4] = [
        "entity_type",
        "worldgen/biome",
        "worldgen/structure",
        "damage_type",
    ];
}

/// The equipment slot of `armor[index]`: head, chest, legs, feet.
/// `doHurtEquipment`'s `durabilityDamage` before the cast to int: `max(1, damage / 4)`.
pub(super) fn durability_damage(damage: f32) -> f32 {
    (damage / 4.0).max(1.0)
}
/// The durability damage a mod left for `slot` (`mod_api::ArmorHurt::new_damage`, cast to int
/// as vanilla casts it); an entry it removed keeps vanilla's `fallback`.
pub(super) fn answered_wear(
    entries: &[mod_api::ArmorHurt],
    slot: horizon_core::equipment_slot::EquipmentSlot,
    fallback: u32,
) -> u32 {
    entries
        .iter()
        .find(|e| e.slot == slot)
        .map_or(fallback, |e| e.new_damage as u32)
}

pub(super) fn armor_slot(index: usize) -> horizon_core::equipment_slot::EquipmentSlot {
    use horizon_core::equipment_slot::EquipmentSlot;
    match index {
        0 => EquipmentSlot::Head,
        1 => EquipmentSlot::Chest,
        2 => EquipmentSlot::Legs,
        _ => EquipmentSlot::Feet,
    }
}

pub use resource_tables::UiResources;

/// What a resource pack reload replaces (`Minecraft.reloadResourcePacks`):
/// block models and sprites with their meshing tables, item models,
/// translations and the textures read on the CPU. Each game holds its own;
/// cloning shares the loaded data.
#[derive(Clone)]
pub struct PackResources {
    source: horizon_assets::resource_source::ResourceSource,
    data_source: horizon_assets::resource_source::ResourceSource,
    data_changes: horizon_assets::resource_source::ResourceSource,
    additional_data: Arc<[Arc<horizon_client::datapack::DataPack>]>,
    assets: Arc<Assets>,
    block_registries: Arc<horizon_client::command::args::Registries>,
    render: Arc<RenderAssets>,
    context: Arc<WorkerContext>,
    layer_names: Arc<HashMap<u32, String>>,
    /// Item definitions, baked item models and special renderers.
    item_renderer: Arc<horizon_client::item_render::ItemRenderer>,
    names: Arc<Names>,
    /// clouds.png: (width, height, filled cells).
    clouds: Arc<(u32, u32, Vec<bool>)>,
    /// misc/forcefield.png: (width, height, RGBA) for the world border wall.
    forcefield: Arc<(u32, u32, Vec<u8>)>,
    /// Custom sky layers (OptiFine's `optifine/sky`) with their textures; none without them.
    custom_sky: Arc<dyn horizon_client::custom_sky::SkyLayers>,
    /// The overworld's, Nether's and End's attribute stacks with the packs' Polytone biome and
    /// dimension modifiers (#11486), when a pack has some: what the frame draws.
    environments: Option<Arc<[Environment; 3]>>,
    /// The packs' Polytone custom particles and block emitters (#11486), when a pack has some.
    polytone_particles: Option<Arc<horizon_client::particles::polytone::Registry>>,
    /// The packs' Polytone post chains (#11486), when a pack has some.
    polytone_post: Option<Arc<horizon_client::polytone_post::PolytonePost>>,
    /// The packs' Polytone custom colours (`polytone/colors.json`).
    polytone_colors: Arc<horizon_client::pack_palette::PackColors>,
}

/// A quick craft drag as the screen draws it: each dragged slot's count and
/// whether the slot caps it (None: hidden), and what stays in hand.
struct DragView {
    slots: HashMap<usize, Option<(u32, bool)>>,
    left: Option<u32>,
}
fn equipment_modifier_applies(group: &str, slot: &str) -> bool {
    group == slot
        || group == "any"
        || (group == "armor" && slot != "mainhand" && slot != "offhand")
        || (group == "hand" && (slot == "mainhand" || slot == "offhand"))
}
/// Ctrl+Q reads the physical keyboard, independently of sprint key mappings.
/// A chunk whose ticking future can complete (`ChunkMap.prepareTickingChunk`):
/// lit, with its neighbours loaded, as `postprocess_chunks` waits for.
fn ticking_chunk(world: &World, x: i32, z: i32) -> bool {
    let Some(center) = world.chunks.get(&(x, z)) else {
        return false;
    };
    center.lit && world.chunks.surrounded((x, z))
}
/// `TicketStorage.canTicketExpire` for a `PORTAL` ticket: its chunk's holder is
/// ready for saving once the futures of its level are done, the chunk lit and
/// its neighbours loaded out to one chunk at a ticking level
/// (`prepareTickingChunk`) and two at an entity-ticking one
/// (`prepareEntityTickingChunk`). The ticket's own level stands for the
/// holder's: the area it waits for is the one its own streaming loads.
fn portal_ticket_can_expire(
    world: &World,
    ticket: &horizon_client::ticket_levels::PortalTicket,
) -> bool {
    use horizon_client::ticket_levels::{BLOCK_TICKING, ENTITY_TICKING};
    let r = match ticket.level {
        l if l <= i32::from(ENTITY_TICKING) => 2,
        l if l == i32::from(BLOCK_TICKING) => 1,
        _ => 0,
    };
    let (x, z) = ticket.chunk;
    world.chunks.get(&(x, z)).is_some_and(|c| c.lit)
        && (-r..=r).all(|dz| (-r..=r).all(|dx| world.chunks.contains_key(&(x + dx, z + dz))))
}

fn select_creative_tab(
    creative: &mut Creative,
    clicks: &mut horizon_client::container_screen::Clicks,
    tab: usize,
) {
    creative.select(tab);
    *clicks = Default::default();
    // Tabs are outside the panel; their release must not drop a carried stack.
    clicks.skip_release();
}

/// The icon the Select World list shows for the world in play
/// (`GameRenderer.tryTakeScreenshotIfNeeded`): its path in the save, and when
/// the last frame grab was asked for.
struct WorldIcon {
    path: PathBuf,
    last_attempt: Option<Instant>,
}

#[derive(Debug, PartialEq)]
struct QueuedLevelEvent {
    kind: i32,
    pos: [i32; 3],
    data: i32,
    source: Option<horizon_world::level::LevelEventPlayer>,
}

/// Physical and scaled GUI dimensions last reported by `window_resize`.
#[derive(Clone, Copy, PartialEq)]
struct ResizeState {
    size: winit::dpi::PhysicalSize<u32>,
    gui_size: [u32; 2],
}

impl ResizeState {
    /// Records `self` as the last report and says whether it differs from the previous one.
    fn differs_from(self, last: &mut Option<Self>) -> bool {
        last.replace(self) != Some(self)
    }
}

/// Temporary block-contact collections rebuilt on every server tick. Keeping
/// their backing storage on `Game` avoids reallocating them while the world is
/// ticking; the wrapper around `tick_blocks` restores the scratch even when a
/// phase returns an error.
#[derive(Default)]
struct TickScratch {
    ticking: Vec<(i32, i32)>,
    inside: Vec<BlockPos>,
    standing: Vec<(BlockPos, bool, f64)>,
    boat_cells: Vec<BlockPos>,
    touched: Vec<BlockPos>,
    sensing: Vec<BlockPos>,
    rest: Vec<BlockPos>,
    later: Vec<BlockPos>,
}

struct Game<'a> {
    /// Shared with an integrated server; `collision` and `motion` borrow from it.
    res: Arc<Resources>,
    rest_loader: Option<WorldRestLoader>,
    pending_entry: Option<PendingWorldEntry>,
    pending_pack_reload: bool,
    /// When `resourcepacks/` was last looked at for the open pack screen, and what it held
    /// (`watch_pack_folder`).
    pack_folder: Option<(
        std::time::Instant,
        Vec<(String, u64, Option<std::time::SystemTime>)>,
    )>,
    /// A pack reload staged on a worker (`LoadingOverlay`): the frame draws
    /// its progress until `drive_pack_reload` applies it.
    pack_reload: Option<packs::PackReload>,
    /// A server's resource packs (game/world/server_packs.rs).
    server_packs: server_packs::ServerPackState,
    /// The link a chat click asks to open (`ConfirmLinkScreen`).
    link_prompt: Option<String>,
    /// A command the player didn't type, waiting for their answer
    /// (game/network/remote_chat.rs).
    command_prompt: Option<remote_chat::CommandPrompt>,
    /// A server's Code of Conduct to accept (game/interface/code_of_conduct.rs).
    code_of_conduct: Option<code_of_conduct::Prompt>,
    /// Single player's server while it loads its world (`Minecraft.doWorldLoad`).
    integrated_starting: Option<integrated::Starting>,
    collision: &'a BlockCollisionRegistry<'a>,
    motion: &'a MotionResources,
    /// This world's frozen damage definitions and reloadable tag snapshot.
    pub(super) health_resources: horizon_client::health::HealthResources,
    damage_tag_generation: u64,
    /// The registries native mods registered data map types for, as of the
    /// server tables' last build: what `data_maps_updated` fires for.
    data_map_registries: Vec<String>,
    /// This game's resource packs (`reload_packs` replaces them).
    pack: PackResources,
    /// The title and menu bundle (GUI atlas, font and panorama).
    gui: UiResources,
    instance_groups: std::cell::RefCell<native_instance_groups::InstanceGroupStorage>,
    standalone_geometry: native_block_entity_draw::StandaloneGeometry,
    /// `FogRenderer`'s evolving state.
    fog: horizon_client::fog::FogState,
    /// `LocalPlayer.itemActivation`: the totem flying up to the screen.
    item_activation: horizon_client::item_activation::ItemActivation,
    weather_parameters: horizon_client::weather::ParameterCache,
    world: World,
    /// The biome table the world plays with: the game's own with the world's
    /// packs over it (`Biomes::with_packs`), `None` before a world opens.
    world_biomes: Option<Arc<Biomes>>,
    /// `world_biomes` as the biome files made it, before `native_biome_colors`.
    world_biomes_base: Option<Arc<Biomes>>,
    streaming: Streaming,
    lod: Option<horizon_client::lod::Lod>,
    lod_source_attempted: bool,
    lod_observe: horizon_client::fast_hash::FxHashSet<(i32, i32)>,
    /// The server level far terrain keeps the chunks of.
    lod_level: Option<String>,
    /// Beacon beam sections kept after their chunk unloads, so far terrain
    /// LOD still draws their beams (`beacon_geometry` refreshes them).
    far_beacons: HashMap<(i32, i32, i32), Vec<(u32, i32)>>,
    /// The detail revision, centre and radius `Lod::update` last had covered for.
    lod_covered_at: Option<(u64, (i32, i32), i32)>,
    /// Whether the view's columns have all arrived, and how new it is: large
    /// translucent pages wait for both before they shrink (`set_terrain_loading`).
    view_arrival: horizon_client::streaming::ViewArrival,
    options: Options,
    player: Option<PlayerMotion>,
    spawn: Option<Vec3>,
    clock: FixedStep,
    previous_position: Vec3,
    /// `yRotO` / `xRotO` of the local player: the view at the start of the tick,
    /// moved by `Entity.turn` and a boat's `clampRotation` as the view is
    /// (`LocalPlayer.getRopeHoldPosition` eases the held lead from it).
    rope_rot_old: (f32, f32),
    eye: f32,
    previous_eye: f32,
    yaw: f32,
    pitch: f32,
    inventory: Vec<Option<ItemStack>>,
    selected: usize,
    carried: Option<ItemStack>,
    craft_grid: Vec<Option<ItemStack>>,
    craft_size: usize,
    /// The crafting table the 3x3 grid belongs to (`CraftingMenu.access`).
    craft_table: Option<[i32; 3]>,
    /// The Mouse Tweaks extra's drag and wheel (`tweaks`).
    tweaks: extras::mouse_tweaks::Tweaks,
    /// `lastQuickMoved`: what the last Shift click moved, which a Shift
    /// double click moves every stack of.
    last_quick_moved: Option<ItemStack>,
    /// Open container block entity, if any.
    container: Option<(i32, i32, i32)>,
    /// The open container and the slots it had when it opened:
    /// `Container.stillValid` holds only while every block entity the menu
    /// took is still there, so a half of a double chest (or the container)
    /// going away closes it.
    open_container_slots: Option<((i32, i32, i32), usize)>,
    /// Open workstation menu (enchanting table, anvil, ...).
    station: Option<stations::Station>,
    /// Status effects and thrown potions (game/items/potions.rs).
    potions: potions::PotionState,
    scratch_slot: Option<ItemStack>,
    mining: Option<Mining>,
    /// The other players' cracks (`LevelRenderer.destroyingBlocks`): breaker id to the block,
    /// the stage (0..=9) and the game time it last changed.
    destroying: HashMap<i32, ([i32; 3], u8, i64)>,
    destroy_delay: u32,
    use_delay: u32,
    creative: bool,
    /// The game mode; `creative` mirrors `mode == Creative`.
    mode: horizon_client::command::args::GameMode,
    keys: horizon_client::held::HeldKeys,
    // Physical input is separate from remapped and toggled movement keys.
    physical_keys: HashSet<KeyCode>,
    physical_buttons: HashSet<MouseButton>,
    left: bool,
    right: bool,
    cursor: (f32, f32),
    captured: bool,
    /// Native grab mode used by winit. Linux window managers can drop the
    /// grab without a focus event, leaving `captured` true but stale.
    capture_mode: Option<CursorGrabMode>,
    /// The window held the pointer when it lost focus: the system lets a
    /// grabbed pointer go with the focus, and a window that comes back still
    /// wanting it takes it again (`MouseHandler.grabMouse`, whose grab
    /// outlives the window's focus).
    regrab_on_refocus: bool,
    /// This view's corner in its shared window; cursor warps include the offset.
    view_at: [u32; 2],
    /// Splitscreen: this player's own Microsoft account session (one of the launcher's other
    /// accounts), in place of the process's (P1's) for its connections and skin.
    split_auth: Option<std::sync::Arc<horizon_client::auth::AuthSession>>,
    focused: bool,
    /// The modifiers the person last held at it, applied at the claim (`held_modifiers`).
    unclaimed_modifiers: Option<winit::event::Modifiers>,
    /// The window's last focus change during a scripted run, applied once the run ends.
    focus_after_run: Option<bool>,
    /// When the window lost focus, while it is still without
    /// (`Minecraft.lastActiveTime`: pausing waits 500 ms).
    focus_lost: Option<Instant>,
    /// The keyboard's modifier state (`KeyEvent.modifiers`).
    modifiers: winit::keyboard::ModifiersState,
    /// The window's fullscreen request and reported state.
    fullscreen: horizon_client::window::FullscreenState,
    /// The monitor whose video modes `settings.video_modes` holds.
    video_modes: horizon_client::window::VideoModeList,
    /// MouseHandler's accumulated look and wheel state.
    mouse: MouseLook,
    /// An agent moved the mouse (`mouse-move`) since the pointer was last
    /// grabbed or released: its motion turns the view like a grabbed pointer.
    agent_pointer: bool,
    /// A world screen with the pointer captured was open last frame.
    was_playing: bool,
    /// Minecraft.pause: a single-player screen that pauses the game is open.
    pause: bool,
    debug: bool,
    /// `horizon-ctl gpu-passes`: time Horizon's own passes on the GPU with F3 shut.
    gpu_passes: bool,
    /// `horizon-ctl gpu-passes detail`: also dispatch times and counters.
    gpu_pass_detail: bool,
    screen: Screen,
    window: Option<Arc<Window>>,
    /// What `window_resize` last reported: the physical size and the GUI settings behind the scaled size.
    resize_state: Option<ResizeState>,
    renderer: Option<Renderer>,
    /// Avoid an endless rebuild loop if the new device is lost as well.
    last_device_recovery: Option<Instant>,
    /// CPU geometry storage retained across frames, including empty/title frames.
    entity_vertices: Vec<Vertex>,
    /// Horizon Extras' dynamic lights this frame (`dynamic_lights.rs`), and how brightly each
    /// item glows (built the first time the extra is on).
    dynamic_lights: Vec<[f32; 4]>,
    /// Name tags drawn over everything (`RenderPipelines.TEXT_SEE_THROUGH`).
    see_through_vertices: Vec<Vertex>,
    /// The first-person player's model, which only a native pack's shadow pass draws.
    shadow_only_vertices: std::cell::RefCell<Vec<Vertex>>,
    /// Which entity this frame's entity vertices belong to (`Frame::entity_motion`).
    entity_motion: std::cell::RefCell<Vec<(u32, u64)>>,
    /// The GPU-posed vertex count at each of `entity_motion`'s marks (`Frame::entity_motion_posed`).
    entity_motion_posed: std::cell::RefCell<Vec<u32>>,
    /// Mob models posed on the GPU (`Frame::skin`, `mobs::skin`): the models stored there,
    /// and this frame's draws.
    skin_models: std::cell::RefCell<horizon_client::mobs::skin::SkinModels>,
    skin_frame: std::cell::RefCell<horizon_client::gfx::entity_skin::SkinFrame>,
    /// The glowing entities' silhouettes this frame (game/entities/glowing.rs).
    glowing_vertices: std::cell::RefCell<Vec<Vertex>>,
    /// The shader pack's entity and block entity ids, while one draws entities.
    pack_ids: Option<shader_packs::PackIds>,
    started: Instant,
    previous: Instant,
    /// FramerateLimitTracker.latestInputTime.
    last_input: Instant,
    /// Controllers (Horizon's own).
    pad: pad::Pads,
    frames: u64,
    /// Mod block entities' kept geometry (game/modding/native_block_entity_draw.rs).
    kept_block_entities: std::cell::RefCell<native_block_entity_draw::KeptGeometry>,
    /// Mod screens' living scenes, per mod and clock id (game/modding/native_scene_live.rs).
    live_scenes: std::cell::RefCell<native_scene_live::LiveScenes>,
    /// Glyphs from `native_draw_text` (`&self`): uploaded to the font atlas with the frame.
    text_patches: std::cell::RefCell<Vec<horizon_client::font::Patch>>,
    /// The mod block entities' view distances, asked of the parts once (`native_block_entity_draw`).
    mod_view_distances: std::cell::OnceCell<native_block_entity_draw::ViewDistances>,
    /// `item_level` for one frame, game time, day time and dimension.
    item_level_cache: std::cell::RefCell<
        Option<(
            (u64, i64, i64, &'static str),
            horizon_assets::item_model::LevelState,
        )>,
    >,
    /// Frames drawn with the player in the world, for `--world-frames`.
    world_frames: u64,
    ticks: u64,
    /// What ends the game with an error; kept as it is, so main can tell a debug crash.
    error: Option<Box<dyn std::error::Error>>,
    message: Option<(String, Instant)>,
    save: Arc<SaveDir>,
    level: Level,
    /// Must precede session_lock: dropping the writer drains writes while the world is locked.
    player_autosaves: horizon_client::lan::playerdata::Autosaves,
    session_lock: Option<horizon_client::save::SessionLock>,
    /// The prepared world is installed. The session lock is taken earlier so tag
    /// reads see ready tables, and is not this marker: a final save before the
    /// prepared `Level` is installed would replace the world with placeholder state.
    world_adopted: bool,
    audio: Audio,
    /// The side, registries and mod settings of this game's resource generation, which
    /// its menus lend to the hooks they fire inside a loop; rebuilt by `refresh_loop_context`.
    loop_context: Arc<horizon_world::native_load_context::LoopContext>,
    setting_overrides: std::cell::RefCell<std::collections::BTreeMap<(String, String), String>>,
    /// The native damage hooks every level of this game lends to its hits.
    damage_hooks: horizon_client::health::DamageHooks,
    music: horizon_client::music::MusicManager,
    ambience: horizon_client::ambience::Ambience,
    /// Minecarts whose `MinecartSoundInstance` is playing.
    cart_sounds: HashSet<u32>,
    /// The minecart the riding sounds were started for (`LocalPlayer.startRiding`).
    riding_cart_sounds: Option<u32>,
    /// `LocalPlayer.wasFallFlying` for the elytra's flying sound.
    was_fall_flying: bool,
    /// Remote guardian eye targets reused between ticks.
    guardian_look_scratch: Vec<(usize, [f64; 3])>,
    dropped: horizon_client::join_seq::JoinList<ItemEntity>,
    next_entity: u32,
    /// What the native mods made of the effect swirls of the ECS entities, by network id, kept while one listens (`ecs_state`).
    ecs_effect_particles: HashMap<u32, horizon_client::effects::ModifiedEffectParticles>,
    /// The last entity id when this level's entity pass began: an item with a
    /// later one was added during that pass and first ticks on the next
    /// (`EntityTickList`). `u32::MAX` until a tick has run.
    ticked_entities_through: u32,
    /// The items `tick_blocks` dropped while applying the entities' block
    /// contacts (a boat breaking a lily pad). Vanilla runs those in each
    /// entity's own tick (`applyEffectsFromBlocks`), inside the entity pass,
    /// so they too first tick on the next tick.
    contact_drops: std::ops::Range<u32>,
    particles: horizon_client::particles::ParticleEngine,
    /// `ItemPickupParticle`s: taken items, orbs and arrows flying to their collector.
    pickups: pickups::Pickups,
    /// Sign text being edited (`SignEditScreen`).
    sign_edit: Option<sign_edit::SignEditor>,
    /// An open book, book and quill or lectern screen.
    book: Option<books::BookScreen>,
    /// Lecterns whose page-turn pulse is running (ticks left).
    lectern_pulses: Vec<((i32, i32, i32), i32)>,
    /// LevelLoadingScreen.smoothedProgress while the player is not placed yet.
    loading_progress: f32,
    /// `CreateWorldScreen.openCreateWorldScreen`'s data read, on a worker,
    /// while the title says `createWorld.preparing`.
    creation_job: Option<std::thread::JoinHandle<std::result::Result<(), String>>>,
    /// When `loading_progress` last eased (the server thread's load, 50 ms a step).
    loading_progress_at: Option<std::time::Instant>,
    /// A received packet started or ended a piston's move: `poll_remote`
    /// syncs the moving pistons once its packets are handled, not once per packet.
    moving_pistons_stale: bool,
    /// The world spawn search's origin chunk and fallback height, once known.
    spawn_origin: Option<((i32, i32), i32)>,
    /// `native_create_spawn_position` was asked, and a mod chose the spawn.
    spawn_hook_fired: bool,
    spawn_hook_cancelled: bool,
    spawn_spiral: horizon_client::world_spawn::ChunkSpiral,
    /// A new player's spawn search, while its candidates' chunks load.
    player_spawn: Option<horizon_client::world_spawn::PlayerSpawnFinder>,
    /// Chunks a respawn search waits for this tick, kept loading beside the
    /// player's own (taken by `update_world`).
    spawn_wait: Vec<(i32, i32)>,
    /// The dimension's ECS world (`ecs_bridge::EcsLevel`); inert on a client.
    ecs: horizon_client::ecs_bridge::EcsLevel,
    remote_entities: horizon_client::join_seq::JoinList<Mob>,
    /// `remote_entities` by network id, for the packets that name one (`game/mobs/mob_index.rs`).
    remote_index: mob_index::MobIndex,
    creatures: creatures::Creatures,
    sculk: sculk::SculkState,
    raiding: raids::Raiding,
    attack_ticks: u32,
    /// The main hand's item at the last player tick, whose attack damage and
    /// speed modifiers apply (`LivingEntity.detectEquipmentUpdates`): a hotbar
    /// swap just before an attack keeps them, with the new item's enchantments
    /// (attribute swapping).
    attack_item: Option<Arc<str>>,
    /// Mobs a player hurt (`LivingEntity.lastHurtByPlayer`), by player UUID.
    last_hurt_by_player: HashMap<u32, combat::PlayerCredit>,
    /// Leashed mobs a player killed, with the killer's UUID: the lead drops
    /// on the corpse's next tick (`Leashable.tickLeash`), for the mods. With
    /// the corpse's dimension: each level's tick prunes only its own.
    lead_killers: HashMap<u32, (u128, Dim)>,
    /// Animals a player fed into love (`Animal.loveCause`), with the guest it was by
    /// connection (None: the host). Breeding credits them, wherever they are.
    love_cause: HashMap<u32, Option<u32>>,
    /// Mobs whose death (`LivingEntity.die`) has been handled.
    /// Looked up per mob each tick: a fast hasher (never iterated).
    died: horizon_client::fast_hash::FxHashSet<u32>,
    /// Client-ticked block-entity animation state (`game/blocks/block_entity_anims.rs`).
    be_anims: horizon_client::block_entity_anim::BlockEntityAnims,
    /// Item being used (eaten): hotbar slot and ticks used.
    using: Option<(usize, u32)>,
    /// `getUseDuration` at the use's start, as a mod left it (`game/items/item_use.rs`).
    use_duration: i32,
    /// What that use began with (`useItem`) and whether a mod set its ticks left
    /// (`game/items/item_use.rs`).
    use_start: using_item::UseStart,
    /// The crossbow charge's start and middle loading sounds played (`game/items/crossbow.rs`):
    /// each charge plays each once, however far a mod's tick moved the charge at a time.
    crossbow_sounds: [bool; 2],
    /// Camera.oldFovModifier / fovModifier.
    /// `Camera` (eye height and field-of-view easing, view placement).
    view: horizon_client::camera::Camera,
    /// Hud.scopeScale (spyglass overlay size).
    scope_scale: f32,
    title: Option<Title>,
    /// The save a native mod asked the title to open (`request_world`), taken by the title's next frame, which
    /// asks the player first.
    native_world_request: Option<native_world_request::Request>,
    /// Armor slots: head, chest, legs, feet.
    armor: [Option<ItemStack>; 4],
    /// The off hand (Inventory slot 40, saved as slot 150; menu slot 504 here).
    offhand: Option<ItemStack>,
    /// The body and saddle slots (`EquipmentSlot.BODY`, `SADDLE`; Inventory
    /// slots 41 and 42, saved as 151 and 152). The hands and armor stay in
    /// the fields above. Nothing in the game reads these yet; they are kept
    /// for mods and saves.
    equipment: horizon_items::equipment::Equipment,
    /// The hand an item use acts with (`InteractionHand`): the off hand only
    /// for its turn in `use_item` and while its item is being used.
    hand: Hand,
    /// The shield's item cooldown after an axe disabled it (ticks).
    shield_cooldown: u32,
    /// `Player.getCooldowns()`: the `use_cooldown` component's entries, what
    /// `itemDecorations` shades a slot by. A server's players have their own
    /// (`GuestPlay::cooldowns`), swapped in for their turn.
    cooldowns: horizon_client::cooldowns::Cooldowns,
    /// The source of a command run through /execute (the player otherwise).
    command_source: Option<commands::execute::Source>,
    native_command_filter: bool,
    titles: titles::Titles,
    /// This game's world is autosaving this tick (`MinecraftServer.isSaving`).
    autosaving: bool,
    /// `MinecraftServer.ticksUntilAutosave`: the ticks left before the next
    /// autosave, counted down a tick at a time.
    ticks_until_autosave: i64,
    applied_modifiers: Vec<(String, String)>,
    experience: Experience,
    orbs: horizon_client::join_seq::JoinList<ExperienceOrb>,
    xp_delay: u32,
    ordered: ordered_tick::OrderedTicks,
    level_up_sound: horizon_client::entities::LevelUpSound,
    /// The arm swing (`LivingEntity.SwingState`) and how many swings started.
    swing: horizon_client::first_person::SwingState,
    swings_started: u32,
    /// Swings of uses whose client predicts them (`SwingSource.PREDICTED`):
    /// a server does not send these back to the player who swung.
    predicted_swings: u32,
    /// The first-person equip heights and the items the hands show (with
    /// the slot each came from).
    heights: horizon_client::first_person::HandHeights,
    /// The hands' lag behind the view (`LocalPlayer.xBob`, `yBob`); a new
    /// LocalPlayer (joining, respawning, changing dimension) starts at 0.
    sway: horizon_client::first_person::HandSway,
    visible_hands: [(usize, Option<ItemStack>); 2],
    /// Whether the main and off hand's slots were set to a new stack that
    /// the hand doesn't show yet (`Game::hand_stack_replaced`).
    hands_replaced: [bool; 2],
    /// `Minecraft`'s attack handling, attack clicks since the last tick and
    /// the attack strength tickers the hand reads.
    clicks: horizon_client::first_person::Clicks<Option<ItemStack>>,
    hand_strength: horizon_client::attack_strength::AttackStrength,
    settings: Settings,
    dragging: bool,
    /// `ClientAvatarState`: walked distance and view bob.
    avatar: horizon_client::camera::AvatarBob,
    flash: u32,
    /// OptiFine custom sky layers' eased brightness (`custom_sky.rs`).
    custom_sky: custom_sky::CustomSkyView,
    /// Bolts that struck here and sky flashes they made, in total (`status`).
    lightning_seen: [u32; 2],
    /// The head of the bed while sleeping; the visual timer also survives waking.
    sleeping: Option<[i32; 3]>,
    sleep_timer: u32,
    /// The beds players sleep in that this game marked `occupied`, by level
    /// (game/blocks/beds.rs).
    player_beds: Vec<(Dim, [i32; 3])>,
    /// `SleepStatus`'s active and sleeping players at the last announcement.
    sleep_seen: (i64, i64),
    dim: Dim,
    world_root: PathBuf,
    carrier_retention_snapshot: Vec<(u128, u32)>,
    /// Offline-to-account UUID migration: loaded entities retain the old UUID until saved.
    owner_migration: Option<(u128, u128)>,
    /// The world's data packs' content tables, when it has packs (`None`
    /// keeps `Resources.rest`'s own in use).
    world_tables: Option<WorldTables>,
    /// Server-owned loot and item snapshots, independent of client preloads.
    server_loot: horizon_client::structure_blocks::LootSnapshot,
    server_items: Option<Arc<Items>>,
    /// The world is superflat (`ServerLevel.isFlat`, for every dimension).
    world_flat: bool,
    /// The world is the Debug preset (`Level.isDebug`, for every dimension): its blocks
    /// cannot be changed and nothing ticks.
    world_debug: bool,
    /// A server's players on their way to another level, each waiting for
    /// the destination to load around them.
    server_trips: Vec<server_travel::Trip>,
    /// Players a command sends to another level, once it has run.
    level_teleports: Vec<server_travel::LevelTeleport>,
    /// Teleports a command made inside a level, for `native_player_teleported`
    /// once the player's move has reached their client (`leave_guest`).
    teleport_notes: Vec<server_travel::TeleportNote>,
    /// A server's levels other than the one in place (game/world/levels.rs).
    levels: levels::Levels,
    /// Which pass of a server tick runs (`tick_server`): the first one reads
    /// the network, the last one ends the players' tick.
    level_pass: levels::Pass,
    /// Nether portals: the player's, items' and mobs' (game/blocks/nether_portal.rs).
    portals: nether_portal::Portals,
    /// The respawn point when it is not the world spawn or a bed: a respawn
    /// anchor (`game/blocks/respawn_anchor.rs`).
    respawn_config: Option<respawn_anchor::RespawnConfig>,
    /// The next enter_dimension call comes from an End portal.
    end_portal_travel: bool,
    /// The open WinScreen (game/interface/win.rs).
    win: Option<win::Win>,
    end: end::EndState,
    block_ticks: Ticks,
    tick_scratch: TickScratch,
    tick_chunks: horizon_client::tick_chunk_membership::TickChunkMembership,
    tickets: horizon_client::ticket_levels::ChunkTickets,
    random_tick_sections: horizon_client::random_tick_sections::RandomTickSections,
    level_events: Vec<LevelEvent>,
    /// Falling models by state and selected-model address list; cleared on pack/atlas replacement.
    falling_vertices: RefCell<HashMap<BlockStateId, Vec<(Vec<usize>, Vec<Vertex>)>>>,
    falling: horizon_client::join_seq::JoinList<FallingBlock, false>,
    /// Blocks the server shows for a moment (`ClientboundAddTransientBlockPacket`).
    transient_blocks: transient_blocks::TransientBlocks,
    paintings: horizon_client::join_seq::JoinList<horizon_client::entities::Painting>,
    painting_variants: paintings::PaintingVariants,
    item_frames: horizon_client::join_seq::JoinList<horizon_client::entities::ItemFrame>,
    frame_model: frames::FrameModel,
    glow_frame_model: frames::FrameModel,
    /// The `map=true` frame variants.
    map_frame_model: frames::FrameModel,
    glow_map_frame_model: frames::FrameModel,
    armor_stands: horizon_client::join_seq::JoinList<horizon_client::entities::ArmorStand>,
    /// Worn armor as of the last equipment check (frost walker).
    location_armor: [Option<ItemStack>; 4],
    /// `Cushion` decoration entities (game/entities/cushions.rs).
    cushions: horizon_client::join_seq::JoinList<horizon_client::entities::Cushion>,
    /// Entities of types Horizon does not simulate.
    others: horizon_client::join_seq::ResidentList<commands::OtherEntity>,
    armor_stand_assets: armor_stands::ArmorStandAssets,
    /// `WaypointStyleManager`.
    waypoint_styles: horizon_client::locator_bar::WaypointStyles,
    /// `ClientWaypointManager`: the locator bar's waypoints.
    waypoints: horizon_client::locator_bar::ClientWaypoints,
    /// The yaw and pitch a native mod's camera drew the last frame from (`native_camera_pose`),
    /// none when it drew from vanilla's.
    posed_rotation: Option<(f32, f32)>,
    /// The roll, in degrees, a native mod's camera drew the last frame with (`native_camera_roll`),
    /// 0 when none rolled it.
    posed_roll: f32,
    /// `LocalPlayer.postEffects`: the chains the server has the screen run
    /// (`ClientboundPostEffectsPacket`), in order.
    post_effects: Vec<String>,
    /// The game time (ticks and partial) of the last frame, for `PolyDeltaTime`.
    polytone_post_ticks: f64,
    /// `GameRenderer.spectatedEntityPostEffect` (`checkEntityPostEffect`).
    spectated_post_effect: Option<&'static str>,
    /// `LocalPlayer.experienceDisplayStartTick`: (last progress, seen a change, tick).
    experience_display: (f32, bool, u64),
    boats: horizon_client::join_seq::JoinList<boats::Boat, false>,
    /// The boats a mob's lead can hang on, for this tick's mob ticks.
    leash_boats: Vec<(u32, horizon_client::mobs::leash::HolderState)>,
    boat_models: boats::BoatModels,
    /// EndCrystalModel's baked layer.
    crystal_model: Option<horizon_client::mobs::Part>,
    /// CushionModel's baked layer.
    cushion_model: Option<horizon_client::mobs::Part>,
    /// Models drawn only once such an entity shows up (lead knots, wind charges...).
    entity_models: EntityModels,
    carts: horizon_client::join_seq::JoinList<minecarts::Minecart, false>,
    /// The boats and minecarts `native_vehicle_moved` follows (lan_vehicles.rs).
    watched_vehicles: Vec<lan_vehicles::WatchedVehicle>,
    /// Songs playing from jukeboxes (tracked sound handles).
    jukebox_sounds: HashMap<(i32, i32, i32), u64>,
    bolts: Vec<lightning::Bolt>,
    maps: maps::Maps,
    /// The ender dragon fight (game/mobs/dragon.rs).
    fight: dragon::Fight,
    dragon: native_entity_refs::dragon_parts_table::DragonSlot,
    crystals: horizon_client::join_seq::JoinList<dragon::Crystal>,
    breath_clouds: horizon_client::join_seq::ResidentList<dragon::Breath>,
    dragon_model: Option<horizon_client::mobs::Part>,
    /// Map textures waiting for the next HUD batch to upload them.
    map_patches: Vec<horizon_client::font::Patch>,
    name_tag_layouts: names::NameTagLayouts,
    text_displays: std::collections::HashMap<i32, text_displays::RemoteTextDisplay>,
    interactions: std::collections::HashMap<i32, interactions::RemoteInteraction>,
    /// The fence knots a server tracks to this client, by entity id, at their
    /// block (`LeashFenceKnotEntity`).
    remote_knots: HashMap<u32, [i32; 3]>,
    /// The entity ids a hosted server gives the knots its mobs hang on, by
    /// block, while a lead is tied there.
    knot_ids: HashMap<[i32; 3], u32>,
    text_display_vertices: usize,
    /// Player skin downloads, and which skin each atlas skin slot holds
    /// (least recently shown first).
    skin_downloads: RefCell<horizon_client::skins::Downloader>,
    /// `ProfileResolver.Cached`: `Dynamic` profiles (name- or id-only heads
    /// and mannequins) resolve through the services in the background.
    skin_profiles: RefCell<horizon_client::skins::Resolver>,
    skin_slots: horizon_client::skins::SkinSlots,
    /// Cosmetics on players (game/cosmetics.rs), off by default.
    cosmetics: cosmetics::Cosmetics,
    cart_models: minecarts::CartModels,
    /// The boat the player rides.
    riding: Option<u32>,
    /// Vehicles that gained a rider this tick whose `START_RIDING_TRIGGER` was
    /// not fired yet (`game/entities/boarding.rs`), for a guest's turn.
    started_riding: Vec<u32>,
    /// A rotation-only write (`/rotate`, `Entity.forceSetRotation`) to the player of this
    /// turn: `leave_guest` sends it as `ClientboundPlayerRotationPacket`, not a teleport.
    rotation_forced: bool,
    /// The mount the last `position_rider` put the player on, to tell the tick
    /// a seat is taken (`Entity.moveTo`, no camera sweep) from the ones that
    /// follow it (`positionRider`, interpolated as usual).
    posed_rider: Option<u32>,
    /// The vehicle the player's file names, waiting for it to be in the world
    /// (`ServerPlayer.loadAndSpawnParentVehicle`).
    pending_vehicle: Option<[i32; 4]>,
    /// The ender inventory (27 slots) and the ender chest showing it.
    ender_items: Vec<Option<ItemStack>>,
    ender_open: Option<(i32, i32, i32)>,
    /// Block entity data of broken blocks waiting for their dropped item.
    pending: spill::Pending,
    /// Item entities spawned and block entities removed while the block
    /// behaviour ran (spilled contents, popped resources).
    spill: spill::Spill,
    /// The recipe book of the open screen (game/items/recipe_book.rs).
    recipe_ui: recipe_book::RecipeUi,
    /// The horse inventory (game/interface/mount_menu.rs).
    mount_menu: mount_menu::MountMenu,
    /// The player's fishing hook.
    hook: horizon_client::join_seq::ResidentSlot<fishing::Hook>,
    /// The other players' hooks a server tracks to this client.
    remote_hooks: Vec<fishing::RemoteHook>,
    tridents: horizon_client::join_seq::JoinList<trident::ThrownTrident>,
    rockets: horizon_client::join_seq::JoinList<fireworks::Rocket>,
    /// Ominous trial spawners' item spawners.
    ominous_spawners:
        horizon_client::join_seq::ResidentList<ominous_item_spawner::OminousItemSpawner>,
    creaking: creaking::CreakingState,
    /// Monster spawners' display mobs (`game/mobs/spawner.rs`).
    spawners: spawner::SpawnerState,
    /// Reused candidate buffers for natural spawning.
    spawn_scratch: natural_spawning::SpawnScratch,
    trident_model: Option<horizon_client::mobs::Part>,
    arrow_model: Option<horizon_client::mobs::Part>,
    bee_stinger_model: Option<horizon_client::mobs::Part>,
    tnts: horizon_client::join_seq::JoinList<tnt::PrimedTnt>,
    hoppers: HashMap<(i32, i32, i32), machines::HopperState>,
    /// The open `host.transaction`s, innermost last (`transactions.rs`).
    transactions: transactions::OpenTransactions,
    /// Backing blocks whose contents notice is on the call stack, innermost last.
    contents_changing: Vec<(mod_api::LevelRef, BlockPos)>,
    /// The players with each mod container open, by level (`tick_mod_container_viewers`).
    mod_container_viewers: HashMap<(&'static str, (i32, i32, i32)), u32>,
    /// Where item entities are, while the block entities tick (hoppers).
    item_columns: Option<machines::ItemColumns>,
    /// The item columns for one pass of the players' pickups
    /// (`tick_guests`), rebuilt after any write to the items (a pickup, a
    /// command, an explosion): see `item_columns`.
    pickup_columns: Option<machines::StampedItemColumns>,
    /// Where the mobs are, while the block entities tick (potent sulfur gas).
    pass_mobs: Option<potent_sulfur::PassMobs>,
    /// Block-entity positions retained between ticks for client effects.
    spawner_tick_positions: Vec<horizon_client::block_entities::Pos>,
    trial_spawner_tick_positions: Vec<horizon_client::block_entities::Pos>,
    /// The `[[native.loop]]` slots' state, per published loop (`published_loops.rs`).
    loop_states: published_loops::LoopStates,
    creative_screen: Creative,
    /// `CreativeModeInventoryScreen.ignoreTextInput`: the chat key opened the
    /// search tab, so the letter it carries is not typed into the box.
    creative_ignore_text: bool,
    control: Option<horizon_client::control::Server>,
    /// The Agent Control setting the socket was last opened or closed for (`agent_settings`).
    agent_control_on: bool,
    /// When an agent's last command came.
    agent_last: Option<Instant>,
    /// This thread's and all threads' allocation counts at the last rendered frame.
    alloc_mark: Option<(
        horizon_threads::alloc_count::Counts,
        horizon_threads::alloc_count::Counts,
    )>,
    /// This play session's frame and tick times, for opt-in performance statistics.
    perf: horizon_client::telemetry::Session,
    pending_bench: Option<(Instant, std::sync::mpsc::Sender<String>, bool)>,
    /// Horizon's extras (minimap, auto-reconnect; mods' counters): `game/interface/extras.rs`.
    extras: extras::ExtrasState,
    /// The PvP & HUD options' state (`game/entities/pvp.rs`).
    pvp: pvp::PvpState,
    /// Horizon mods (`game/modding/mods/mod.rs`), none running unless turned on.
    mods: mods::ModsState,
    /// The in-game browser's worker, separate from the cloneable reload inputs.
    mod_jobs: mod_browser::Jobs,
    /// Every item and recipe, for mods (`game/modding/native_recipes.rs`), made once.
    recipe_tables: native_recipes::RecipeTables,
    /// The item panel beside container screens (`game/modding/native_panel.rs`).
    panel: native_panel::Panel,
    /// Horizon's own shader pack, while one is picked (`game/modding/native_shaders.rs`).
    native_shaders: native_shaders::NativeShaders,
    /// Native client mods' claimed items, unpolled events and key mappings (game/modding/native_claimed_items.rs).
    native_input: native_claimed_items::ModInput,
    /// The native lifecycle bookkeeping.
    native_lifecycle: native_lifecycle::NativeLifecycle,
    resident_entities: resident_state::ResidentEntities,
    /// The mobs lent to the hook calls running now, before they join a level
    /// (game/modding/native_unspawned_mob.rs).
    lent_mobs: RefCell<native_unspawned_mob::LentMobs>,
    /// Entities whose copies are held out of storage; their handles refuse until stored back.
    held_out: Vec<u32>,
    /// Native mods' config files this game loaded (game/modding/native_configs.rs).
    native_configs: native_configs::NativeConfigs,
    mod_phase: mod_api::__private::PhaseOwner,
    /// Private custom-book connection and placement state.
    native_net: native_net::NativeNet,
    /// Meshed sections waiting for an upload budget (Horizon settings).
    pending_uploads: Vec<((i32, i32, i32), horizon_client::mesher::SectionMesh)>,
    occlusion: entity_culling::EntityOcclusion,
    /// A scripted flight from the control socket (`fly`).
    flight: Option<control::Flight>,
    /// The running agent action from the control socket (game/runtime/agent/mod.rs).
    agent: agent::Agent,
    /// What happened, for `events` and `wait-for` (game/runtime/events.rs).
    events: events::Events,
    /// The health history and client captures (game/profile_capture.rs).
    profile: profile_capture::Profile,
    /// `HORIZON_TICK_SCRIPT`'s scenario (game/world/tick_script.rs).
    tick_script: tick_script::TickScript,
    /// The overworld's last `sky_darken` and its inputs (time, rain, thunder).
    sky_darken_memo: std::cell::Cell<Option<((Dim, i64, u32, u32), i32)>>,
    /// The last water plane `water_plane` found, kept while none is in reach.
    water_plane_memo: std::cell::Cell<Option<f32>>,
    /// The camera's block and the second `water_plane` last scanned at.
    water_plane_scan: std::cell::Cell<Option<([i32; 3], i64)>>,
    /// What the boats' and minecarts' ticks know about the mobs: their box reach
    /// (`vehicle_mob_reach`) and a grid of their boxes (`mobs_meeting`).
    vehicle_mobs: boat_passengers::VehicleMobs,
    persist: persist::EntityMemory,
    /// Missing profession IDs already reported for this world session.
    missing_professions: HashSet<String>,
    entity_attachments: FxHashMap<u32, horizon_mod_runtime::attachments::Attachments>,
    entity_initialization: entity_initialization::EntityInitialization,
    /// Typed per-level membership follows dimensions, not the temporary world swap.
    attachment_indexes: attachment_index::Indexes,
    /// Connected to a server: the world comes from the network (game/network/remote/mod.rs).
    remote: Option<remote::Remote>,
    /// The current connection came from Realms (retained for reconnect).
    realm_session: bool,
    /// Replay playback (game/replay.rs), and Horizon Studio's editor on it (game/studio.rs).
    pub(super) replay: replay::Replay,
    /// This world is open to the network (game/lan.rs).
    lan: Option<lan::LanHost>,
    /// Single player's server thread, which `remote` is connected to.
    integrated: Option<integrated::Integrated>,
    /// The shapes of the open world's levels, from its dimension types.
    level_shapes: horizon_client::chunk::LevelShapes,
    /// This game is that server thread's (game/world/integrated.rs).
    serving: Option<integrated::Serving>,
    world_options: Option<lan_screen::WorldOptions>,
    /// Chat Settings opened from RestrictionsScreen: Done returns to World Options.
    options_to_world_options: bool,
    command_block_screen: Option<command_blocks::CommandBlockScreen>,
    /// `StructureTemplateManager.structureRepository`: the templates in memory
    /// by id, a miss included (game/administration/structure_blocks.rs).
    structure_templates: HashMap<String, Option<structure_blocks::TemplateEntry>>,
    structure_spawns: horizon_client::mobs::spawn_selection::StructureSpawns,
    /// The game tests read so far, by id (the test instance registry).
    test_defs: std::cell::RefCell<HashMap<String, Option<test_instances::TestDef>>>,
    /// `test_defs` holds the world's whole test instance registry
    /// (`snapshot_test_defs`), so a miss is final.
    test_defs_snapshot: bool,
    /// `GameTestBlockHighlightRenderer.markers`: block, text, removal time.
    test_highlights: HashMap<(i32, i32, i32), (String, std::time::Instant)>,
    /// `GameTestTicker.SINGLETON`: the game tests running on this server.
    game_tests: game_tests::GameTests,
    /// The F3 overlay's charts (F3+2, F3+3).
    debug_charts: horizon_client::debug_chart::DebugCharts,
    /// `Minecraft.gpuUtilization`: the last timed frame's GPU time over its
    /// CPU time, in percent.
    gpu_utilization: f64,
    /// `ServerChunkCache.lastSpawnState` of each level: its spawnable chunks
    /// and each `MobCategory`'s count.
    spawn_counts: HashMap<Dim, (i32, [i32; 8])>,
    /// The natural spawning policy mods set on each level (`LevelRef::set_spawn_limits`,
    /// `set_spawn_flags`); a level with none spawns as vanilla. Kept while the world is open.
    spawn_policies: HashMap<mod_api::DimensionKey, natural_spawning::SpawnPolicy>,
    /// The passages mods registered in each level, and a client's copy (`native_passages.rs`).
    passages: native_passages::Passages,
    death_focus: Option<u8>,
    /// MultiPlayerGameMode.previousLocalPlayerMode (F3+N goes back to it).
    previous_mode: Option<horizon_client::command::args::GameMode>,
    gamemode_switcher: Option<horizon_client::game_mode_switcher::GameModeSwitcher>,
    /// The game is handling a key, mouse or typed-text event, from the window or the
    /// control socket: the only time a mod reaches the clipboard (`with_user_input`).
    user_input: std::cell::Cell<bool>,
    /// The world was saved for the application ending (`save_for_exit`).
    saved_for_exit: bool,
    /// A mod's read-only screen hook can queue the primary client's quit.
    client_quit: std::cell::Cell<ClientQuit>,
    pending_save: Option<saving::ClosingSave>,
    /// Scheduled end of the server tick (`MinecraftServer.haveTime`).
    server_tick_deadline: Option<Instant>,
    /// The world list's icon in progress (`GameRenderer.tryTakeScreenshotIfNeeded`):
    /// `None` without a local world, or once the save has its icon.png.
    world_icon: Option<WorldIcon>,
    /// The block a guest is acting on (game/network/lan_actions/interaction.rs).
    forced_hit: Option<Hit>,
    /// `Minecraft.hitResult` while the clicks of a tick act (game/network/remote/mod.rs
    /// `remote_handle_input`): picked once, so a creative break does not
    /// move the use that follows it past the broken block.
    tick_hit: std::cell::Cell<Option<Option<Hit>>>,
    /// The mob of that `hitResult`, picked with the item held at the pick
    /// (`LocalPlayer.raycastHitResult`: a spear's `attack_range`), before the
    /// tick's number keys change it.
    tick_mob: Option<Option<(usize, f64, [f64; 3])>>,
    /// `Player.entityInteractionRange` and `blockInteractionRange`, refreshed
    /// each tick from the attribute (a server's reach modifier, creative's
    /// +2): the pick helpers' `&self` reach.
    entity_reach: f64,
    block_reach: f64,
    client_targeting: std::cell::Cell<mod_api::ClientTargeting>,
    /// The part of a right click a guest's packet asks for (game/network/lan_actions/interaction.rs).
    guest_use: Option<GuestUse>,
    /// The last `use_hand` that ended the click did so with `FAIL`, which
    /// does not consume the action (handleUseItemOn's ANY_BLOCK_USE).
    use_failed: bool,
    block_use_original: Vec<(Hand, Option<ItemStack>)>,
    /// Level events of this tick for the guests, with their excluded source.
    lan_events: Vec<QueuedLevelEvent>,
    /// Jukebox level events 1010 (song name) and 1011 (`None`) for the guests;
    /// the song's network id is only known once the hub is in hand.
    lan_songs: Vec<([i32; 3], Option<String>)>,
    lan_cushion_breaks: Vec<(horizon_world::registry::BlockStateId, [f64; 3])>,
    /// Block events (position, b0, b1, state) and entity events (id, event) for the guests.
    lan_block_events: Vec<([i32; 3], i32, i32, u32)>,
    lan_entity_events: Vec<(i32, i8)>,
    /// Successful crits (attacking guest, hit entity), queued while the hub is taken.
    lan_critical_hits: Vec<(Option<u32>, i32, bool)>,
    /// Spear stabs that landed (the stabbing guest), queued while the hub is
    /// taken: `broadcastEntityEvent(player, 2)`.
    lan_kinetic_hits: Vec<Option<u32>>,
    /// `LivingEntity.recentKineticEnemies`: when each player (a guest, or the
    /// window's own) last stabbed each entity this use.
    stabbed: HashMap<(Option<u32>, u32), i64>,
    /// The other players the acting player's spear can reach this turn,
    /// taken from the hub before it (`spear_players`).
    stab_players: Vec<spear_players::StabPlayer>,
    /// Spear stabs on players worked out in a guest's turn, landed once the
    /// hub is back (`land_player_stabs`).
    player_stabs: Vec<spear_players::StabBatch>,
    /// The movement a guest's client reported this tick
    /// (`ServerPlayer.getKnownMovement`), for their turn.
    known_movement: Vec3,
    /// `LivingEntity.lastKineticHitFeedbackTime` of the entities that felt a
    /// spear's hit (`spear::OWN` is this window's player), in game time.
    kinetic_feedback: HashMap<i32, i64>,
    /// Global level events for every guest (`ServerLevel.globalLevelEvent`), with their level.
    lan_global_events: Vec<(&'static str, i32, [i32; 3], i32)>,
    /// `ContainerOpenersCounter`: how many players (the host and the guests)
    /// have each chest open.
    lid_openers: HashMap<(i32, i32, i32), i32>,
    lan_acting: Option<u32>,
    /// A mod asked for the command trees again (`native_refresh_commands`):
    /// the next server tick asks every mod for its commands and resends them.
    native_commands: native_server::NativeCommandState,
    /// The abilities of the server player whose turn it is (`as_guest`): their record's
    /// copy, which the turn reads and writes and `leave_guest` stores. None outside a turn.
    acting_abilities: Option<horizon_client::lan::abilities::PlayerAbilities>,
    /// Other players in the current guest turn, including borrowed ancestor bodies.
    lan_building_boxes: building_players::BuildingPlayers,
    /// A guest's `/save-all`: saved once their turn ends (`run_guest_command`),
    /// when their own state is back in their record.
    save_after_guest_turn: bool,
    /// Guests whose records are displaced by nested player swaps, outer first.
    lan_guest_swaps: Vec<u32>,
    /// Guests whose last turn a panic ended after their state was written back and the outer
    /// player restored (`as_guest_at`, `as_guest_messaged`, `leave_guest`), which hides the turn
    /// from `lan_acting`: crash teardown keeps their last saved file (`keep_interrupted_players`).
    /// Cleared as each server tick starts: a panic something caught leaves a game that carried on.
    lan_interrupted_turns: Vec<u32>,
    /// Crash teardown left the integrated server's owner unplaced, or lost the hub: the save keeps
    /// the owner's player file, stats and advancements as last saved (`save_game_metadata`).
    keep_owner_files: bool,
    /// Live attachments and original snapshots retained during player replacement.
    player_state: original_player::PlayerState,
    /// The block the acting player is breaking (`break_block`): its destroy
    /// event, and the ones before it, are the player's own (`Level.levelEvent`
    /// with the player excluded); the events after it are not.
    lan_break_pos: Option<[i32; 3]>,
    /// Where a player's break puts the block's own drops instead of popping
    /// them (`PlayerRef::destroy_block` with `into_inventory`).
    break_collect: Option<Vec<ItemStack>>,
    /// The name of the guest `lan_acting` acts for (`Player.getName`).
    lan_acting_name: Option<String>,
    /// Their UUID (`Player.getUUID`).
    lan_acting_uuid: Option<u128>,
    /// Where the mods refused to prime TNT: the sound and game event the level
    /// queued right after the prime are dropped (`prime_tnt_event`).
    tnt_refused: Option<BlockPos>,
    /// Their permission level: an op's above 0 (`PlayerList.isOp`).
    lan_acting_permission: Option<u8>,
    /// The spawn protection radius that holds the acting guest back
    /// (`DedicatedServer.isUnderSpawnProtection` before the position test):
    /// None for an operator, without operators, or off a dedicated server.
    lan_acting_spawn_protection: Option<i32>,
    /// Their entity id, which the hub (often taken out of `lan` while they
    /// act) would otherwise have to be asked for.
    lan_acting_entity: Option<u32>,
    /// Whether the player currently swapped into the game is connectionless.
    lan_acting_fake: bool,
    /// A fake player's `perform_action` in progress, and what it refused.
    fake_action: std::sync::Arc<native_server::FakeAction>,
    /// Command feedback to forward to a guest.
    chat_capture: Option<Vec<horizon_client::text::Text>>,
    /// Packets for the guest acting now, sent once its action returns.
    guest_packets: Vec<horizon_protocol::packets::play::Clientbound>,
    /// The acting guest's cooldowns, as (item, ticks): their packet needs the
    /// hub's item registry, which the guest's turn does not hold.
    guest_cooldowns: Vec<(Arc<str>, i32)>,
    /// The acting guest's own entity events (`ServerPlayer.connection.send`),
    /// sent with their entity id, which the guest's turn does not hold.
    guest_entity_events: Vec<i8>,
    /// The acting guest's equipment breaks (`onEquippedItemBroken`'s entity
    /// events), broadcast with their entity id once their turn ends.
    guest_equipment_breaks: Vec<i8>,
    /// Block sounds queued by the acting guest, excluding that guest.
    guest_block_sounds: Vec<(String, [f64; 3], f32, f32)>,
    /// `Player.playSound` of the acting guest (event, position, volume,
    /// pitch): `level.playSound(player, ...)` leaves the guest out, whose
    /// own client plays it.
    guest_player_sounds: Vec<(String, [f64; 3], f32, f32)>,
    /// Sounds bound to an entity this tick, for the players (`game/interface/level_sounds.rs`).
    bound_sounds: Vec<level_sounds::BoundSound>,
    /// `broadcastToAdmins` lines a guest's command queued, sent once it
    /// returns: the line, whether operators get it and whether the console
    /// logs it.
    admin_feedback: Vec<(horizon_client::text::Text, bool, bool)>,
    /// The server console is running a command: whether its feedback goes to
    /// the operators (`DedicatedServer.shouldInformAdmins`,
    /// `broadcast-console-to-ops`).
    console_broadcast: Option<bool>,
    /// The command the console is running came over RCON: its source is named `Rcon`
    /// (`RconConsoleSource.createCommandSourceStack`), not `Server`.
    rcon_source: bool,
    /// A dedicated server's operator, whitelist and ban lists, for `/op`,
    /// `/ban` and friends (`DedicatedPlayerList`).
    server_lists: Option<std::sync::Arc<std::sync::Mutex<dedicated::Lists>>>,
    /// State of `/spectate`, `/posteffect`, `/debug`, `/waypoint` and friends.
    cmds: commands::extra::CommandExtras,
    /// The retained server reload listeners of the last successful reload.
    reload_resources: mod_api::RetainedListeners,
    ui: chat::UiState,
    combat: combat::CombatTracker,
    /// Each guest's own `CombatTracker`, out of `combat` between turns.
    guest_combat: std::collections::HashMap<u32, combat::CombatTracker>,
    mount: mounts::MountState,
    stats: stats::Stats,
    score: scoreboard::ScoreState,
    fns: functions::FnState,
    bosses: bossbars::BossState,
    /// `Hud.bossOverlay`: the bars this window is showing.
    boss_overlay: horizon_client::boss_overlay::BossOverlay,
    exec: commands::execute::ExecState,
    rand_sequences: horizon_client::random_sequences::RandomSequences,
    /// `BrushableBlockEntity`'s brush counts of the blocks being brushed.
    brushing: brush::Brushing,
    border: horizon_client::worldborder::WorldBorder,
    /// The camera's 16-block section when the wall was last invalidated.
    border_section: Option<[i32; 3]>,
    border_state: horizon_client::border_render::BorderRenderState,
    /// `Hud.tabList`: the player list's header, footer and heart states.
    tab_list: horizon_client::tab_list::TabList,
    adv: advancements::AdvState,
    restore_player_state: bool,
    /// The Server Links screens' state (game/network/server_links.rs).
    links_ui: server_links::LinksUi,
    camera_type: horizon_client::camera::CameraType,
    fangs: fangs::FangState,
    /// The local player as a model (third person, the inventory).
    player_model: Option<Mob>,
    block_entity_tick_key: Option<block_entity_tick_items::Key>,
    block_entity_tick_items: Option<Box<block_entity_tick_items::Scope>>,
    wasm_dispatch_failures: std::rc::Rc<std::cell::Cell<u64>>,
}
use horizon_gameplay::dim::Dim;
/// `Inventory.add` on a player's 36 slots (hotbar first) and off hand,
/// `max` the stack's own maximum: onto equal stacks first
/// (`getSlotWithRemainingSpace`: the selected slot, the off hand, then the
/// hotbar and the main inventory), then into empty slots of the inventory
/// (`getFreeSlot`, never the off hand). What does not fit stays in `stack`;
/// true when some of it is left.
fn inventory_add(
    inventory: &mut [Option<ItemStack>],
    offhand: &mut Option<ItemStack>,
    selected: usize,
    max: u32,
    stack: &mut ItemStack,
) -> bool {
    let order = std::iter::once(selected)
        .chain(std::iter::once(shield::OFFHAND_SLOT))
        .chain((0..36).filter(|&i| i != selected));
    for i in order {
        let slot = if i == shield::OFFHAND_SLOT {
            &mut *offhand
        } else {
            &mut inventory[i]
        };
        if let Some(slot) = slot
            && slot.same_item_same_components(stack)
            && slot.count < max
        {
            let moved = (max - slot.count).min(stack.count);
            slot.count += moved;
            stack.count -= moved;
            if stack.count == 0 {
                return false;
            }
        }
    }
    for slot in inventory.iter_mut().take(36) {
        if slot.is_none() {
            let moved = stack.count.min(max);
            *slot = Some(ItemStack {
                count: moved,
                ..stack.clone()
            });
            stack.count -= moved;
            if stack.count == 0 {
                return false;
            }
        }
    }
    true
}

fn facing_offset(facing: &str) -> [i32; 3] {
    match facing {
        "north" => [0, 0, -1],
        "south" => [0, 0, 1],
        "west" => [-1, 0, 0],
        "east" => [1, 0, 0],
        "up" => [0, 1, 0],
        _ => [0, -1, 0],
    }
}
/// How far along `dir` (within `reach`) a ray from `eye` enters `b`.
fn ray_box(eye: [f64; 3], dir: [f64; 3], reach: f64, b: Aabb) -> Option<f64> {
    let (min, max) = (b.min_position(), b.max_position());
    let (lo, hi) = ([min.x, min.y, min.z], [max.x, max.y, max.z]);
    let (mut t0, mut t1) = (0.0f64, reach);
    for a in 0..3 {
        if dir[a].abs() < 1e-9 {
            if eye[a] < lo[a] || eye[a] > hi[a] {
                return None;
            }
            continue;
        }
        let (mut ta, mut tb) = ((lo[a] - eye[a]) / dir[a], (hi[a] - eye[a]) / dir[a]);
        if ta > tb {
            std::mem::swap(&mut ta, &mut tb);
        }
        t0 = t0.max(ta);
        t1 = t1.min(tb);
    }
    (t0 <= t1).then_some(t0)
}
/// `Entity.calculateViewVector`, which `Minecraft.pick` aims with: float radians through `Mth.sin` and
/// `Mth.cos`, vanilla's 65536-entry table. A placement's cursor and an interaction's point come from this ray, and
/// the exact trigonometry below moved them by up to ~1e-4 of a block.
fn pick_view_vector(yaw: f32, pitch: f32) -> [f64; 3] {
    let v = horizon_world::view_direction::view_vector(pitch, yaw);
    [v.x, v.y, v.z]
}
fn view_vector(yaw: f32, pitch: f32) -> [f64; 3] {
    let (y, p) = (f64::from(yaw).to_radians(), f64::from(pitch).to_radians());
    [-y.sin() * p.cos(), -p.sin(), y.cos() * p.cos()]
}
/// `Level.canHaveWeather`: sky light, no ceiling, and not the End (the one
/// level the rule names by id).
fn level_can_have_weather(sky_light: bool, has_ceiling: bool, is_end: bool) -> bool {
    sky_light && !has_ceiling && !is_end
}
/// `Camera.setRotation` (`rotationYXZ(π - yaw, -pitch, 0)`) for the player's
/// view, turned round in third-person front view (`Camera.update`).
fn camera_rotation(
    camera_type: horizon_client::camera::CameraType,
    yaw: f32,
    pitch: f32,
) -> glam::Quat {
    let (view_yaw, view_pitch) =
        if camera_type == horizon_client::camera::CameraType::ThirdPersonFront {
            (yaw + 180.0, -pitch)
        } else {
            (yaw, pitch)
        };
    glam::Quat::from_euler(
        glam::EulerRot::YXZ,
        std::f32::consts::PI - view_yaw.to_radians(),
        -view_pitch.to_radians(),
        0.0,
    )
}
/// `Camera.setRotation(yaw, pitch, roll)`'s last turn, `rotationYXZ`'s `-roll`, on a camera
/// rotation: what a mod's roll (`native_camera_roll`) does to what faces the camera. The
/// conjugate is `camera::view_roll(roll)` in front of the view.
fn rolled_camera(rotation: glam::Quat, roll: f32) -> glam::Quat {
    if roll == 0.0 {
        rotation
    } else {
        rotation * glam::Quat::from_rotation_z(-roll.to_radians())
    }
}
impl Game<'_> {
    /// `Level.canHaveWeather`, from the level's dimension type (sky light, no
    /// ceiling) and the End, which vanilla names by id. Every rain, thunder
    /// and lightning rule asks this, never the dimension's id.
    fn level_key(&self) -> horizon_core::LevelKey {
        horizon_core::LevelKey::builtin_or_overworld(self.dim.id())
    }
    /// The level's dimension type: a mod's or a data pack's own when it has one, else the
    /// built-in level's.
    fn dimension_type(&self) -> &horizon_world::dimension_type::DimensionFacts {
        self.dimension_facts_for(
            horizon_core::LevelKey::parse(self.dim.id()).unwrap_or(horizon_core::LevelKey::OVERWORLD),
        )
    }
    /// A client only knows its current level's type; other keys use built-in defaults.
    fn dimension_facts_for(
        &self,
        key: horizon_core::LevelKey,
    ) -> &horizon_world::dimension_type::DimensionFacts {
        use horizon_world::dimension_type::DimensionFacts;
        {
            DimensionFacts::of_level(key)
        }
    }
    fn dimension_type_of(
        &self,
        dim: Dim,
    ) -> &'static horizon_world::dimension_type::DimensionFacts {
        horizon_world::dimension_type::DimensionFacts::of_level(
            horizon_core::LevelKey::parse(dim.id()).unwrap_or(horizon_core::LevelKey::OVERWORLD),
        )
    }
    fn eye_position(&self) -> Option<[f64; 3]> {
        let p = self.player.as_ref()?;
        Some([
            p.position.x,
            p.position.y + f64::from(p.dimensions.eye_height),
            p.position.z,
        ])
    }
    /// Where targeting starts and points: the eyes along the view. Every
    /// crosshair target (blocks, fluids, entities, mod raycasts) is found
    /// along this ray, so a tracked controller can stand in for it.
    fn aim(&self) -> Option<([f64; 3], [f64; 3])> {
        // Minecraft.pick: from the camera entity's eyes along its view
        // (LivingEntity.getViewYRot: the head's yaw).
        if let Some(e) = self.spectated_camera_snapshot() {
            let [x, y, z] = e.position;
            let eye = [x, y + f64::from(e.eye_height), z];
            return Some((eye, pick_view_vector(e.y_rot, e.x_rot)));
        }
        Some((self.eye_position()?, pick_view_vector(self.yaw, self.pitch)))
    }
    /// `Player.blockInteractionRange`: the block pick's and mining's reach.
    fn reach(&self) -> f64 {
        self.block_reach
    }
    /// Outline shapes that depend on more than the block state, as one box
    /// in block space: `LightBlock` and `ScaffoldingBlock.getShape` are a full
    /// block while the main hand holds the block's own item
    /// (`CollisionContext.isHoldingItem`), and `ShulkerBoxBlock.getShape`
    /// grows with the lid (`ShulkerBoxBlockEntity.getBoundingBox`).
    fn context_outline(
        &self,
        pos: [i32; 3],
        state: BlockStateId,
        player_context: bool,
        anims: &horizon_client::block_entity_anim::BlockEntityAnims,
    ) -> Option<[f64; 6]> {
        let name = &self.res.facts().get(state).name;
        if matches!(&**name, "minecraft:light" | "minecraft:scaffolding") {
            return (player_context && self.held().is_some_and(|s| *s.item == **name))
                .then_some([0.0, 0.0, 0.0, 1.0, 1.0, 1.0]);
        }
        if !name.ends_with("shulker_box") {
            return None;
        }
        let progress = anims.shulkers.get(&pos).map_or(0.0, |s| s.progress);
        let facing = self
            .pack
            .assets
            .registry
            .state(state)
            .and_then(|s| s.properties.get("facing").cloned());
        let d = facing_offset(facing.as_deref().unwrap_or("up"));
        let mut b = [0.0, 0.0, 0.0, 1.0, 1.0, 1.0];
        for a in 0..3 {
            match d[a] {
                1 => b[a + 3] += 0.5 * f64::from(progress),
                -1 => b[a] -= 0.5 * f64::from(progress),
                _ => {}
            }
        }
        Some(b)
    }
    fn outline_context_kind(&self) -> horizon_world::collision_context::EntityContextKind {
        if self.spectated_camera_snapshot().is_some() {
            horizon_world::collision_context::EntityContextKind::Living
        } else {
            horizon_world::collision_context::EntityContextKind::Player
        }
    }
    /// A block's outline boxes in world space (`getShape` plus the random offset).
    fn outline_boxes(&self, pos: [i32; 3]) -> Vec<[f64; 6]> {
        self.outline_boxes_in_context(&self.world, &self.be_anims, pos, true)
    }
    fn outline_boxes_in_context(
        &self,
        world: &World,
        anims: &horizon_client::block_entity_anim::BlockEntityAnims,
        pos: [i32; 3],
        player_context: bool,
    ) -> Vec<[f64; 6]> {
        self.outline_boxes_iter(
            world.block(pos[0], pos[1], pos[2]),
            anims,
            pos,
            player_context,
        )
        .collect()
    }
    /// The declared outline uses vanilla's inside-ray test and interaction override.
    fn clip_declared_outline(
        &self,
        state: BlockStateId,
        pos: [i32; 3],
        origin: [f64; 3],
        dir: [f64; 3],
        reach: f64,
    ) -> Option<Hit> {
        use horizon_core::geometry::{Aabb, Vec3};
        use std::borrow::Cow;
        let facts = self.res.facts().get(state);
        let cached = self.collision.cached_shapes();
        let shape = cached.context_outline_shape(state, self.outline_context_kind());
        let boxes: Cow<'_, [Aabb]> = if let Some(shape) = shape {
            Cow::Borrowed(shape.boxes())
        } else if let Some(b) = self.context_outline(pos, state, true, &self.be_anims) {
            Cow::Owned(vec![Aabb::new(
                Vec3::new(b[0], b[1], b[2]),
                Vec3::new(b[3], b[4], b[5]),
            )])
        } else {
            Cow::Owned(
                facts
                    .outline
                    .iter()
                    .map(|b| Aabb::new(Vec3::new(b[0], b[1], b[2]), Vec3::new(b[3], b[4], b[5])))
                    .collect(),
            )
        };
        let offset = if facts.outline_offset {
            self.pack
                .context
                .tables
                .offset(state, pos[0], pos[2])
                .unwrap_or([0.; 3])
        } else {
            [0.; 3]
        };
        let offset = Vec3::new(offset[0], offset[1], offset[2]);
        let interaction: Cow<'_, [Aabb]> = if let Some(shape) = cached.interaction_shape(state) {
            Cow::Borrowed(shape.boxes())
        } else {
            let facing = self
                .pack
                .assets
                .registry
                .state(state)
                .and_then(|s| s.properties.get("facing"))
                .map(String::as_str);
            Cow::Owned(
                interaction_shapes::boxes(&facts.name, facing)
                    .into_iter()
                    .map(|b| Aabb::new(Vec3::new(b[0], b[1], b[2]), Vec3::new(b[3], b[4], b[5])))
                    .collect(),
            )
        };
        let start = Vec3::new(origin[0], origin[1], origin[2]);
        let end = Vec3::new(
            origin[0] + dir[0] * reach,
            origin[1] + dir[1] * reach,
            origin[2] + dir[2] * reach,
        );
        let hit = horizon_world::interaction_shapes::clip_with_interaction_override(
            boxes.iter().map(|b| b.move_by(offset)),
            interaction.iter().copied(),
            start,
            end,
            BlockPos::new(pos[0], pos[1], pos[2]),
        )?;
        Some(Hit::from_block_hit(pos, &hit))
    }
    /// Voxel traversal against non-air, non-fluid blocks, clipped against
    /// each block's outline shape (`BlockGetter.clip` with `ClipContext.Block.OUTLINE`).
    fn pick(&self) -> Option<Hit> {
        // A guest of a hosted world acts on the block their client targeted.
        if let Some(hit) = self.forced_hit {
            return Some(hit);
        }
        if let Some(hit) = self.tick_hit.get() {
            return hit;
        }
        let (origin, dir) = self.aim()?;
        // GameRenderer.pick: the block ray runs out to the larger of the two
        // interaction ranges; filterHitResult then drops a hit past the block's.
        self.clip_targeting_ray(origin, dir, self.block_reach.max(self.entity_reach))
            .filter(|hit| {
                let distance: [f64; 3] = std::array::from_fn(|i| hit.point[i] - origin[i]);
                distance.iter().map(|v| v * v).sum::<f64>() <= self.block_reach.powi(2)
            })
    }
    /// The first block outline hit within `reach` along the unit `dir`.
    fn clip_ray(&self, origin: [f64; 3], dir: [f64; 3], reach: f64) -> Option<Hit> {
        self.clip_ray_fluids(origin, dir, reach, false)
    }
    /// `LocalPlayer.moveTowardsClosestSpace` at the body's four corners
    /// (aiStep, unless a spectator or a passenger): a corner inside a
    /// suffocating block sets a 0.1 speed towards the nearest side that is
    /// free, on that side's axis; a later corner overwrites an earlier one.
    /// Suffocating is a full collision cube that is also a full render cube,
    /// as `in_wall` has it.
    fn closest_space_nudge(&self) -> [Option<f64>; 2] {
        match self.player.as_ref() {
            Some(p) if !p.spectator && self.riding.is_none() => {
                closest_space_nudge(p, &self.world, self.res.facts())
            }
            _ => [None; 2],
        }
    }
    /// What the window decides for its player's body every tick: the look
    /// (Player.travel steers sprint-swimming by the pitch), Auto Jump (LocalPlayer.sendPosition), the slowdown of an
    /// item in use (LocalPlayer.modifyInput), the helmet's Respiration
    /// (decreaseAirSupply), the wings, leather boots on powder snow
    /// (PowderSnowBlock.canEntityWalkOnPowderSnow), freeze-immune wear
    /// (LivingEntity.canFreeze), the dimension's FAST_LAVA, the sleeping
    /// pose and blindness (Player.isMobilityRestricted, which stops a sprint).
    pub(crate) fn sync_player_body(&mut self) {
        let glider = self.glider_worn();
        let sleeping = self.sleeping.is_some();
        let item_in_use = self
            .using
            .and_then(|(slot, _)| {
                if slot == shield::OFFHAND_SLOT {
                    self.offhand.as_ref()
                } else {
                    self.inventory.get(slot)?.as_ref()
                }
            })
            .and_then(|s| self.items().get(&s.item));
        let item_use_speed = item_in_use.map(|i| i.use_speed);
        let item_use_slows_sprint = item_in_use.is_some_and(|i| !i.can_sprint);
        let respiration = self.armor[0]
            .as_ref()
            .map_or(0, |s| s.enchantment_level("minecraft:respiration") as i32);
        let leather_boots = worn_boots::leather_boots(&self.armor);
        let freeze_immune = self.wears_freeze_immune();
        let burn_immune = self.wears_burn_from_stepping_immunity();
        let nudge = self.closest_space_nudge();
        let fast_lava = self.dimension_type().fast_lava;
        let blind = self.potions.effects.get("blindness").is_some();
        let Some(player) = self.player.as_mut() else {
            return;
        };
        player.nudge = nudge;
        player.mobility_restricted = blind;
        player.yaw = self.yaw;
        player.pitch = self.pitch;
        player.auto_jump_enabled = self.settings.auto_jump;
        player.item_use_speed = item_use_speed;
        player.item_use_slows_sprint = item_use_slows_sprint;
        player.respiration = respiration;
        player.glider = glider;
        // A client of a server, the window's own integrated one included, takes
        // the glide's end from the server's flag alone (`DATA_SHARED_FLAGS_ID`,
        // applied by the SetEntityData handler): vanilla's LocalPlayer never
        // ends a glide itself.
        player.server_owns_glide = false;
        let _ = player.set_sleeping(sleeping);
        player.leather_boots = leather_boots;
        player.freeze_immune = freeze_immune;
        player.burn_from_stepping_immune = burn_immune;
        player.fluids.fast_lava = fast_lava;
    }
    /// Joins an item only after drop hooks have seen its constructor's motion.
    fn add_item_entity_prepared(
        &mut self,
        stack: ItemStack,
        position: [f64; 3],
        velocity: Option<[f64; 3]>,
        pickup_delay: u32,
        random: Option<spill::ItemRandom>,
    ) -> [f64; 3] {
        let velocity = self.push_item_entity(stack, position, velocity, pickup_delay, random);
        self.join_item_entity(self.next_entity, position);
        velocity
    }
    /// The block entity at `pos` saved with its id and position
    /// (`saveWithFullMetadata`), or the saved one Horizon keeps as loaded:
    /// what a broken block's loot sees as `LootContextParams.BLOCK_ENTITY`.
    fn loot_block_entity(&self, pos: [i32; 3], block: &str) -> Option<horizon_core::nbt::Tag> {
        let key = (pos[0], pos[1], pos[2]);
        match self.world.block_entities.get(&key) {
            Some(entity) => Some(horizon_client::save::block_entity_encode::encode(
                key, entity, block,
            )),
            None => spill::saved_entity(&self.world, pos),
        }
    }
    /// Where a loot roll at `origin` happens, for a mod's own loot kinds:
    /// the block entity it may copy from, and this level as
    /// `LootContext.getLevel` gives it (`Level.isRaining`: rain above 0.2;
    /// `isThundering`: thunder above 0.9; neither where `canHaveWeather` is
    /// false: a level with a ceiling or no sky light, or the End).
    fn loot_place_at(
        &self,
        origin: [f64; 3],
        block_entity: Option<horizon_core::nbt::Tag>,
    ) -> horizon_world::loot::LootPlace {
        let (rain, thunder) = self.world.weather.levels(1.0);
        // Level.canHaveWeather: sky light, no ceiling and not the End.
        let weather =
            self.world.shape.sky_light && !self.world.shape.has_ceiling && self.dim != Dim::End;
        horizon_world::loot::LootPlace {
            origin: Some(origin),
            block_entity,
            level: Some(horizon_world::loot::LootLevel {
                dimension: self.dim.id().to_owned(),
                game_time: self.world.game_time,
                day_time: self.world.day_time,
                raining: weather && rain > 0.2,
                thundering: weather && thunder > 0.9,
            }),
        }
    }
    /// `Block.getDrops` for a block a player breaks: the block's loot table
    /// with the player as `this`, drawing from its server random sequence
    /// (`LootContext.Builder.create`), else the level random. The roll's
    /// origin is the block's centre and `block_entity` the entity it had.
    fn block_loot_at(
        &mut self,
        pos: [i32; 3],
        state: BlockStateId,
        held: Option<&ItemStack>,
        block_entity: Option<horizon_core::nbt::Tag>,
    ) -> Vec<ItemStack> {
        let dynamic = self.pending.dynamic(pos);
        let place = self.loot_place_at(pos.map(|v| f64::from(v) + 0.5), block_entity);
        let (_, entity) = self.native_clicker();
        let mut drops = horizon_world::loot::with_native_loot_place(place, || {
            let roll = horizon_client::items::BlockLootRoll {
                held,
                dynamic: &dynamic,
                breaker: true,
                ..Default::default()
            };
            self.roll_block_loot(pos, state, Some(entity as u32), roll)
        });
        let player = self.native_acting_id();
        self.native_block_loot(pos, state, Some(player), held, None, &mut drops);
        drops
    }
    /// `Player.drop`: the statistics and a thrown item entity in front of the player.
    pub(super) fn player_drop(&mut self, thrown: ItemStack) -> Option<u32> {
        let eye = self.eye_position()?;
        // LivingEntity.drop: every drop swings the main hand, a menu's
        // THROW and a creative tab's drop included.
        self.swing_hand(Hand::Main, SwingAnimation::DEFAULT);
        // ServerPlayer.drop: the drop statistics, only when an item entity came of it.
        let (item, count) = (thrown.item.clone(), thrown.count as i32);
        let dropped = self.throw_from_eyes(thrown, eye, false, true)?;
        self.award_stat("dropped", &item, count);
        self.award_custom_stat("drop", 1);
        Some(dropped)
    }
    /// `Player.drop(stack, false, thrown_from_hand)`: the stack thrown from the eyes along the
    /// view, the main hand swinging for every drop (`LivingEntity.drop`; `PREDICTED` only keeps
    /// the swing from being sent back to the player who made it). `thrown_from_hand` names the
    /// player as the thrower, as the drop key does. The item's id, none for an empty stack or a
    /// toss a mod canceled.
    pub(super) fn drop_stack_from_eyes(
        &mut self,
        stack: ItemStack,
        thrown_from_hand: bool,
    ) -> Option<u32> {
        if stack.count == 0 {
            return None;
        }
        self.swing_hand(Hand::Main, SwingAnimation::DEFAULT);
        let eye = self.eye_position()?;
        self.throw_from_eyes(stack, eye, false, thrown_from_hand)
    }
    /// The UUID of the player acting: the guest whose turn it is, else the host.
    pub(super) fn acting_player_uuid(&mut self) -> [i32; 4] {
        match self.lan_acting_uuid {
            Some(guest) => horizon_client::lan::playerdata::uuid_ints(guest),
            None => self.level.ensure_player_uuid(),
        }
    }
    /// `Block.fallOn` for item `id`, which fell `distance` onto the block at `on`
    /// (`Entity.checkFallDamage`): a mod block's `fall_on` Part hears it, through the same
    /// `block_contact::fall` as a player's and a mob's landing. An item takes no fall damage.
    /// Nothing is lent or built unless a Part of that block listens.
    fn item_fall_on(&mut self, id: u32, on: horizon_core::position::BlockPos, distance: f64) {
        let state = self.world.block(on.x, on.y, on.z);
        if !horizon_client::block_contact::fall_listens(state) {
            return;
        }
        let mut lent = horizon_client::health::LentLevel::take(&mut self.world);
        // A native fall_on Part can unwind. Keep the persistent level carrier
        // outside the callback's temporary views, and return its progressed state
        // before propagating the original panic. This does not restore the item batch.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut level = horizon_client::health::DamageLevel::lent(
                &self.world,
                self.pack.assets.registry,
                &mut lent,
                Some(world_items!(self)),
                Some(&self.pack.block_registries),
                &self.damage_hooks,
            );
            let contact = horizon_client::block_contact::Contact {
                entity: id as i32,
                kind: "minecraft:item",
                state,
                position: on,
            };
            let _ = horizon_client::block_contact::fall(&mut level, contact, distance, 0.0);
        }));
        lent.give_back(&mut self.world);
        if let Err(payload) = result {
            std::panic::resume_unwind(payload);
        }
    }
    /// The item entities `tick_dropped_items` took out, back in the level,
    /// before those its hooks spawned meanwhile (`addFreshEntity` appends).
    /// They keep their join numbers; the spawned ones got theirs on joining.
    fn put_back_dropped(&mut self, items: horizon_client::join_seq::JoinList<ItemEntity>) {
        self.dropped.restore(
            items,
            horizon_client::join_seq::RestoreOrder::SurvivorsFirst,
        );
    }
    /// `Entity.doWaterSplashEffect` for a dropped item: the server plays
    /// `entity.generic.splash` (`Entity.playSound`), which the players hear.
    pub(super) fn apply_item_splash(&mut self, splash: horizon_client::entities::ItemSplash) {
        if let horizon_client::entities::ItemSplash::Sound { at, volume, pitch } = splash {
            self.audio
                .play("minecraft:entity.generic.splash", Some(at), volume, pitch);
        }
    }
    /// Run vanilla block behaviour against the world through `LevelAccess`.
    fn with_level<R>(
        &mut self,
        f: impl FnOnce(&Behaviours, &mut GameLevel<'_>, &mut Ticks) -> R,
    ) -> R {
        self.ensure_stateful_entity_table();
        let darken = self.sky_darken();
        let (rain, thunder) = self.weather_levels(1.0);
        // Spectators ignore block triggers (Entity.isIgnoringBlockTriggers).
        let spectator = self.spectator();
        let player_box = self.player.as_ref().filter(|_| !spectator).map(|p| {
            let w = f64::from(p.dimensions.width) / 2.0;
            (
                [p.position.x - w, p.position.y, p.position.z - w],
                [
                    p.position.x + w,
                    p.position.y + f64::from(p.dimensions.height),
                    p.position.z + w,
                ],
            )
        });
        let player_collision = self.player.as_ref().filter(|_| !spectator).map(|p| {
            let main_hand = self.inventory.get(self.selected).and_then(Option::as_ref);
            PlayerCollision {
                y: p.position.y,
                descending: p.velocity.y < 0.0,
                fall_distance: p.fall_distance,
                main_hand_item: main_hand.map_or_else(
                    || Arc::from("minecraft:air"),
                    |stack| Arc::clone(&stack.item),
                ),
                main_hand_count: main_hand.map_or(0, |stack| stack.count as i32),
                leather_boots: worn_boots::leather_boots(&self.armor),
            }
        });
        let guest_boxes = self.lan_block_trigger_boxes();
        let building_player_boxes = self
            .lan_acting
            .is_some()
            .then(|| self.lan_building_boxes.boxes(self.dim));
        let dimension = self.dim.level();
        let rules = self.level_rules();
        let fire_players: Vec<[f64; 3]> = self
            .player
            .iter()
            .filter(|_| !spectator)
            .map(|p| [p.position.x, p.position.y, p.position.z])
            .chain(self.lan.iter().flat_map(|h| {
                h.hub
                    .guests
                    .simulation_iter()
                    .filter(|g| h.hub.here(g) && !g.spectator())
                    .map(|g| g.position)
            }))
            .collect();
        // EntityType.LIGHTNING_BOLT: 0 x 0.
        let trigger_extras: Vec<Aabb> = self
            .bolts
            .iter()
            .map(|b| {
                let at = Vec3::new(b.at[0], b.at[1], b.at[2]);
                Aabb::new(at, at)
            })
            .collect();
        // `ContainerOpenersCounter.getEntitiesWithContainerOpen`: the players
        // whose menu still holds a container of the block at a position. A
        // double chest answers for both halves (`ChestBlockEntity$1` takes a
        // `CompoundContainer`), and a spectator's menu counts for nothing.
        let mut open_containers: Vec<(i32, i32, i32)> = Vec::new();
        for key in self.container.into_iter().filter(|_| !spectator).chain(
            self.lan
                .iter()
                .flat_map(|h| {
                    h.hub
                        .guests
                        .simulation_iter()
                        .filter(|g| h.hub.here(g) && !g.spectator())
                })
                .filter_map(|g| g.menu.as_ref().and_then(|m| m.pos)),
        ) {
            open_containers.push(key);
            if let Some(other) = self.chest_other_half(key) {
                open_containers.push(other);
            }
        }
        // CopperGolem.hasContainerOpen: the chest a golem opened, and its other half.
        let mut golem_chests: Vec<(i32, i32, i32)> = Vec::new();
        for &key in self.world.chest_openers.keys() {
            golem_chests.push(key);
            golem_chests.extend(self.chest_other_half(key));
        }
        let native_authoritative = self.native_authoritative();
        let environment = self.res.environment_of(&self.dimension_type().id);
        let other_entities = entity::entity_lists_of!(self, without_carts);
        let mut level = GameLevel {
            cake_probe: false,
            cake_requested: std::cell::Cell::new(false),
            cake_answer: None,
            api_level: self.level(self.dim.id()),
            command_block_transition: None,
            positions: piston_carry_state::PositionState {
                gateways: &mut self.be_anims.gateway_servers,
                hoppers: &mut self.hoppers,
            },
            border: self.border.geometry(),
            redstone_experiments: self
                .level
                .enabled_features
                .iter()
                .any(|feature| feature == "minecraft:redstone_experiments"),
            ecs: &mut self.ecs,
            other_entities,
            crystals: &self.crystals,
            trigger_extras,
            world: &mut self.world,
            facts: self.res.facts(),
            collision: self.collision,
            events: &mut self.level_events,
            dimension,
            darken,
            environment,
            raining: rain > 0.2,
            thundering: thunder > 0.9,
            enabled_features: self
                .remote
                .as_ref()
                .map_or(&self.level.enabled_features, |remote| &remote.features),
            player_box,
            guest_boxes,
            building_player_boxes,
            player_collision,
            biomes: world_biomes!(self),
            seed: self.level.seed,
            behaviours: self.res.behaviours(),
            items: world_items!(self),
            recipes: world_recipes!(self),
            // A client rolls no loot (LootTable.getRandomItems needs a
            // ServerLevel): it doesn't load the tables on its game thread.
            loot: {
                self.server_loot.get()
            },
            registry: self.pack.assets.registry,
            block_registries: &self.pack.block_registries,
            rules,
            difficulty: i32::from(self.ui.difficulty),
            fire_players,
            rand_sequences: &mut self.rand_sequences,
            pending: &mut self.pending,
            spill: &mut self.spill,
            tickets: self.tickets.levels(),
            random_tick_sections: &mut self.random_tick_sections,
            lid_openers: &mut self.lid_openers,
            open_containers: &open_containers,
            golem_chests: &golem_chests,
            client_side: false,
            native_authoritative,
            native_player: self.lan_acting_uuid,
            native_player_entity: self.lan_acting_entity.map(|id| id as i32),
            debug: self.world_debug,
            native_sounds: &mut self.creatures.lan_sounds,
            loop_context: &self.loop_context,
            native_particles: &mut self.creatures.lan_particles,
            sound_seeds: self.lan.as_ref().map(|host| &host.hub),
            drop_tool: None,
            drop_entity: None,
            drop_breaker_at: None,
            drop_into: None,
        };
        let result = f(self.res.behaviours(), &mut level, &mut self.block_ticks);
        self.flush_spill();
        if self.lan.is_some() {
            self.flush_lan_particles();
            self.flush_native_sounds();
        }
        result
    }
    pub(super) fn native_block_ignite_allowed(
        &mut self,
        pos: BlockPos,
        cause: &'static str,
    ) -> bool {
        self.with_level(|_, level, _| level.allow_block_ignite(pos, cause))
    }
    pub(super) fn native_cauldron_level_change_allowed(
        &mut self,
        pos: BlockPos,
        from: i32,
        to: i32,
        reason: &'static str,
    ) -> bool {
        self.with_level(|_, level, _| level.allow_cauldron_level_change(pos, from, to, reason))
    }
    /// Level.setBlock(pos, state, UPDATE_ALL) with neighbour/shape updates.
    fn level_set(&mut self, pos: [i32; 3], state: BlockStateId) -> Result<()> {
        self.level_set_with_flags(pos, state, level_flags::UPDATE_ALL)
    }
    /// Level.setBlock with the caller's update flags.
    fn level_set_with_flags(
        &mut self,
        pos: [i32; 3],
        state: BlockStateId,
        flags: i32,
    ) -> Result<()> {
        self.with_level(|b, level, ticks| {
            b.set_block(
                level,
                ticks,
                BlockPos::new(pos[0], pos[1], pos[2]),
                state,
                flags,
            );
        });
        self.process_level_events()
    }
    /// `BlockState.canSurvive(level, pos)` for `state` at `pos` in the level in
    /// place: the rule a placement checks before `level_set` (a sapling on
    /// grass, a torch on a wall), with a mod block's own `native_can_survive`
    /// hook. False for an invalid state, an unloaded or out-of-height
    /// position, and on a client.
    ///
    /// This is the non-closure equivalent of `with_level(|b, level, ticks|
    /// b.can_survive(level, ticks, pos, state))`, for a wasm mod whose
    /// internals view cannot send a closure.
    // A wasm mod's staged internals table calls this; nothing in Horizon does.
    #[allow(dead_code)]
    fn level_can_survive(&mut self, pos: [i32; 3], state: BlockStateId) -> bool {
        if !self.is_logical_server()
            || !self.world.loaded(pos[0], pos[2])
            || !self.world.shape.contains_y(pos[1])
            || self.pack.assets.registry.state(state).is_none()
        {
            return false;
        }
        self.with_level(|b, level, ticks| {
            b.can_survive(level, ticks, BlockPos::new(pos[0], pos[1], pos[2]), state)
        })
    }
    fn attack_block(&mut self, pos: [i32; 3]) -> Result<()> {
        let note = self.with_level(|b, level, ticks| {
            b.attack_block(level, ticks, BlockPos::new(pos[0], pos[1], pos[2]))
        });
        // NoteBlock.attack awards the statistic.
        if note {
            self.award_custom_stat("play_noteblock", 1);
        }
        self.process_level_events()
    }
    /// `AbstractCandleBlock.getParticleOffsets`, in sixteenths of a block:
    /// `CandleBlock.PARTICLE_OFFSETS` by its `candles` (one to four wicks),
    /// then `CandleCakeBlock.PARTICLE_OFFSETS`, whose one wick sits a block up.
    fn candle_offsets(name: &str, candles: Option<usize>) -> &'static [[u8; 3]] {
        const OFFSETS: [&[[u8; 3]]; 5] = [
            &[[8, 8, 8]],
            &[[6, 7, 8], [10, 8, 7]],
            &[[8, 5, 10], [6, 7, 8], [9, 8, 7]],
            &[[7, 5, 9], [10, 7, 9], [6, 7, 6], [9, 8, 6]],
            &[[8, 16, 8]],
        ];
        if name.ends_with("candle_cake") {
            OFFSETS[4]
        } else {
            OFFSETS[candles.unwrap_or(1).clamp(1, 4) - 1]
        }
    }
    /// `DistanceManager`'s tickets of the level in place, from every player in
    /// it (the local player, LAN guests, a server's players; a spectator the
    /// rule skips holds none), its forced chunks and its portal tickets. A game
    /// with neither a server nor a player (the headless fixtures) ticks
    /// everywhere.
    fn update_chunk_tickets(&mut self) {
        let rule = self
            .ui
            .rules
            .bool_at(horizon_client::gamerules::vanilla::SPECTATORS_GENERATE_CHUNKS);
        let mut players: Vec<((i32, i32), bool)> = self.lan.as_ref().map_or_else(Vec::new, |h| {
            h.hub
                .guests
                .iter()
                .filter(|g| h.hub.here(g))
                .filter(|g| !g.is_fake()) // fake players hold small forced tickets below
                .map(|g| (g.ticket_chunk(), g.spectator()))
                .collect()
        });
        if let Some(p) = &self.player {
            players.push((
                (
                    (p.position.x.floor() as i32) >> 4,
                    (p.position.z.floor() as i32) >> 4,
                ),
                self.spectator(),
            ));
        }
        // A spectator the rule skips is still a player: a game that has one
        // must not take the headless fixtures' tick-everywhere case below.
        let any_player = !players.is_empty();
        let sources = (true || any_player).then(|| {
            horizon_client::ticket_levels::Sources {
                players: players
                    .into_iter()
                    .filter(|&(_, spectator)| game_mode::loads_chunks(rule, spectator))
                    .map(|(chunk, _)| chunk)
                    .collect(),
                forced: self.streaming.forced.iter().copied().collect(),
                // Filled from the `PORTAL` tickets by `update`.
                portal: Vec::new(),
                // EnderDragonFight.tick: while a player watches the fight, the arena holds a
                // `DRAGON` ticket of radius 9 at (0, 0), so its island ticks out to 7 chunks
                // whatever the watcher's simulation distance. The fight is where the type has one.
                dragon: (self.dimension_type().has_ender_dragon_fight
                    && self.fight.arena_watched())
                .then_some((0, 0)),
                viewed: self.viewed_tickets(),
                view_distance: self.settings.render_distance.clamp(2, 32),
                simulation_distance: self.settings.simulation_distance.max(2),
            }
        });
        // The levels' changes are kept only for a listener, so that a tick
        // with none looks at no chunk.
        self.tickets.record_changes(
            self.is_logical_server()
                && mod_api::__private::listened(mod_api::HookId::ChunkTicketLevelUpdated),
        );
        self.tickets.update(sources);
        self.run_chunk_ticket_level_updates();
    }
    /// `ServerLevel.startTickingChunk`: a loaded chunk's saved ticks are
    /// scheduled from the game time at which it becomes a ticking chunk
    /// (block ticking, with its neighbours loaded). Chunks become ticking
    /// between ticks in vanilla: before this tick's clock advanced.
    fn start_ticking_chunks(&mut self) {
        let levels = self.tickets.levels();
        let world = &self.world;
        let started = self
            .tick_chunks
            .start_ticking(|(x, z)| levels.loaded_ticking(x, z) && ticking_chunk(world, x, z));
        let time = self.world.game_time - 1;
        for (x, z) in started {
            let pos = horizon_core::position::ChunkPos::new(x, z);
            self.block_ticks.blocks.unpack_chunk(pos, time);
            self.block_ticks.fluids.unpack_chunk(pos, time);
        }
    }

    /// Copies the moving-piston block entities into the world, where the
    /// collision queries and the renderer read them.
    ///
    /// A client runs this for every block change that ends a piston's move,
    /// so it stays linear in the pistons moving: a clock of a thousand pistons
    /// ends two thousand moves at once.
    fn sync_moving_pistons(&mut self) {
        use horizon_client::fast_hash::{FxHashMap, FxHashSet};
        let previous = std::mem::take(&mut self.world.moving_pistons);
        let current: Vec<_> = self
            .block_ticks
            .moving_pistons
            .iter()
            .map(|(p, piston)| ([p.x, p.y, p.z], piston.shape_state()))
            .collect();
        // The positions drawn by `current` or `fallback` so far.
        let mut drawn: FxHashSet<[i32; 3]> = current.iter().map(|(p, _)| *p).collect();
        let mut fallback: Vec<_> = previous
            .into_iter()
            .filter(|(pos, _)| !drawn.contains(pos))
            .collect();
        drawn.extend(fallback.iter().map(|(p, _)| *p));
        // A finished piston stays drawn until the chunk mesh shows the block
        // it placed, however many ticks that takes. Pistons share a few
        // sections, so each section is asked once.
        let older = std::mem::take(&mut self.world.moving_piston_fallback);
        let mut pending: FxHashMap<(i32, i32, i32), bool> = FxHashMap::default();
        for (pos, piston) in older {
            if drawn.contains(&pos) {
                continue;
            }
            let Some(section) = self.section_of(pos) else {
                continue;
            };
            if *pending
                .entry(section)
                .or_insert_with(|| self.section_mesh_pending_in(section))
            {
                drawn.insert(pos);
                fallback.push((pos, piston));
            }
        }
        self.world.moving_piston_fallback = fallback;
        self.world.moving_pistons = current;
    }
    /// Side effects reported by the behaviour library, in vanilla order.
    fn process_level_events(&mut self) -> Result<()> {
        let events = std::mem::take(&mut self.level_events);
        self.lan_record_events(&events);
        dispense_fixture::record_events(&events);
        for event in events {
            match event {
                LevelEvent::LevelEvent {
                    id: 2001,
                    pos,
                    data,
                    ..
                } => {
                    // LevelEventHandler: destroy particles and the block's
                    // break sound (none for air).
                    let state = BlockStateId(data as u32);
                    let p = [pos.x, pos.y, pos.z];
                    self.destroy_particles(p, state);
                    self.play_break_sound(p, state);
                }
                LevelEvent::LevelEvent {
                    id: 3006,
                    pos,
                    data,
                    ..
                } => self.sculk_charge_event(pos, data),
                LevelEvent::LevelEvent { id: 3007, pos, .. } => self.shriek_event(pos),
                // LevelEventHandler 1010 and 1011: a jukebox's song starts
                // (`playJukeboxSong`) or stops, and the parrots nearby dance or stop.
                LevelEvent::LevelEvent {
                    id: 1010,
                    pos,
                    data,
                    ..
                } => self.jukebox_song_event([pos.x, pos.y, pos.z], data),
                LevelEvent::LevelEvent { id: 1011, pos, .. } => {
                    self.jukebox_stop_event([pos.x, pos.y, pos.z])
                }
                // LevelEventHandler 2019 and 2020: the crack on a face of the
                // block being mined, with its hit sound for 2020.
                LevelEvent::LevelEvent {
                    id: id @ (2019 | 2020),
                    pos,
                    data,
                    ..
                } => {
                    let face = horizon_core::geometry::Direction::ALL[data.clamp(0, 5) as usize];
                    self.breaking_block_effects([pos.x, pos.y, pos.z], face.normal(), id == 2020);
                }
                LevelEvent::LevelEvent {
                    id: 3008,
                    pos,
                    data,
                    ..
                } => self.brush_complete_event([pos.x, pos.y, pos.z], data),
                LevelEvent::LevelEvent { id: 2004, pos, .. } => {
                    self.spawner_burst([pos.x, pos.y, pos.z])
                }
                // LevelEventHandler 1505: the growth particles, then the sound.
                LevelEvent::LevelEvent {
                    id: 1505,
                    pos,
                    data,
                    ..
                } => {
                    let at = [pos.x, pos.y, pos.z].map(|v| f64::from(v) + 0.5);
                    self.audio
                        .play("minecraft:item.bone_meal.use", Some(at), 1.0, 1.0);
                }
                LevelEvent::LevelEvent {
                    id: 2015,
                    pos,
                    data,
                    ..
                } => self.dragon_egg_particles([pos.x, pos.y, pos.z], data),
                LevelEvent::LevelEvent {
                    id: 2016,
                    pos,
                    data,
                    ..
                } => self.shulker_teleport_particles([pos.x, pos.y, pos.z], data),
                LevelEvent::LevelEvent {
                    id: 2017,
                    pos,
                    data,
                    ..
                } => self.consume_teleport_particles([pos.x, pos.y, pos.z], data),
                LevelEvent::LevelEvent {
                    id: 2018,
                    pos,
                    data,
                    ..
                } => self.enderman_teleport_particles([pos.x, pos.y, pos.z], data),
                LevelEvent::LevelEvent { id: 2003, pos, .. } => {
                    self.eye_break_particles([pos.x, pos.y, pos.z]);
                }
                LevelEvent::LevelEvent {
                    id: 2013,
                    pos,
                    data,
                    ..
                } => self.smash_dust(pos, data),
                // LevelEventHandler 2002 and 2007: a potion's splash.
                LevelEvent::LevelEvent {
                    id: 2002,
                    pos,
                    data,
                    ..
                } => self.potion_splash_particles(pos, data, false),
                LevelEvent::LevelEvent {
                    id: 2007,
                    pos,
                    data,
                    ..
                } => self.potion_splash_particles(pos, data, true),
                LevelEvent::LevelEvent {
                    id: id @ 3002..=3005,
                    pos,
                    data,
                    ..
                } => self.block_particle_event(id, pos, data),
                LevelEvent::LevelEvent {
                    id: 1500,
                    pos,
                    data,
                    ..
                } => self.composter_fill_event(pos, data == 1),
                // LevelEventHandler 1504: the stalactite's drip particle.
                LevelEvent::LevelEvent { id: 1504, pos, .. } => self.dripstone_drip_particle(pos),
                // LevelEventHandler 2006: the dragon fireball's burst.
                LevelEvent::LevelEvent {
                    id: 2006,
                    pos,
                    data,
                    ..
                } => self.dragon_fireball_burst(pos, data),
                // LevelEventHandler 2009: a wet sponge drying.
                LevelEvent::LevelEvent { id: 2009, pos, .. } => {
                    self.puffs_above_block(pos, horizon_client::particles::ParticleType::Cloud)
                }
                // LevelEventHandler 2011 and 2012: a bee growing a crop, a
                // turtle egg laid.
                LevelEvent::LevelEvent {
                    id: 2011 | 2012,
                    pos,
                    data,
                    ..
                } => self.turtle_egg_particles(pos, data),
                // LevelEventHandler 2014: a straw bed destroyed, with no sound.
                LevelEvent::LevelEvent {
                    id: 2014,
                    pos,
                    data,
                    ..
                } => self.destroy_particles([pos.x, pos.y, pos.z], BlockStateId(data as u32)),
                // LevelEventHandler 3009: a sniffer egg cracking.
                LevelEvent::LevelEvent { id: 3009, pos, .. } => {
                    self.sniffer_egg_crack_particles(pos)
                }
                LevelEvent::LevelEvent {
                    id: id @ 3011..=3021,
                    pos,
                    data,
                    ..
                } => self.trial_level_event(id, [pos.x, pos.y, pos.z], data),
                LevelEvent::LevelEvent { id, pos, .. } => {
                    let c = Some([
                        f64::from(pos.x) + 0.5,
                        f64::from(pos.y) + 0.5,
                        f64::from(pos.z) + 0.5,
                    ]);
                    let sound = match id {
                        1503 => Some(("minecraft:block.end_portal_frame.fill", 1.0)),
                        1501 => Some(("minecraft:block.lava.extinguish", 0.5)),
                        1009 => Some(("minecraft:block.fire.extinguish", 0.5)),
                        1502 => Some(("minecraft:block.redstone_torch.burnout", 0.5)),
                        1033 => Some(("minecraft:block.chorus_flower.grow", 1.0)),
                        1034 => Some(("minecraft:block.chorus_flower.death", 1.0)),
                        1045 => Some(("minecraft:block.pointed_dripstone.land", 2.0)),
                        1052 => Some(("minecraft:block.sulfur_spike.land", 2.0)),
                        1046 => Some((
                            "minecraft:block.pointed_dripstone.drip_lava_into_cauldron",
                            2.0,
                        )),
                        1047 => Some((
                            "minecraft:block.pointed_dripstone.drip_water_into_cauldron",
                            2.0,
                        )),
                        // LevelEvent.SOUND_ZOMBIE_WOODEN_DOOR (attack, break).
                        1019 => Some(("minecraft:entity.zombie.attack_wooden_door", 2.0)),
                        1021 => Some(("minecraft:entity.zombie.break_wooden_door", 2.0)),
                        1026 => Some(("minecraft:entity.zombie.infect", 2.0)),
                        1027 => Some(("minecraft:entity.zombie_villager.converted", 2.0)),
                        1030 => Some(("minecraft:block.anvil.use", 1.0)),
                        1029 => Some(("minecraft:block.anvil.destroy", 1.0)),
                        1031 => Some(("minecraft:block.anvil.land", 0.3)),
                        1042 => Some(("minecraft:block.grindstone.use", 1.0)),
                        1044 => Some(("minecraft:block.smithing_table.use", 1.0)),
                        // LevelEvent.SOUND_PAGE_TURN (a lectern's page).
                        1043 => Some(("minecraft:item.book.page_turn", 1.0)),
                        // LevelEvent.SOUND_PHANTOM_BITE: a phantom's bite.
                        1039 => Some(("minecraft:entity.phantom.bite", 0.3)),
                        // LevelEvent.SOUND_SPLASH_POTION and its instant twin.
                        1053 | 1054 => Some(("minecraft:entity.splash_potion.break", 1.0)),
                        _ => None,
                    };
                    if let Some((name, volume)) = sound {
                        let pitch = match id {
                            1009 => {
                                2.6 + (self.world.random.next_float()
                                    - self.world.random.next_float())
                                    * 0.8
                            }
                            // LevelEventHandler draws these from the client
                            // level's random, before 1501's and 1502's smoke.
                            1501 | 1502 => {
                                let r = self.particles.level_random();
                                2.6 + (r.next_float() - r.next_float()) * 0.8
                            }
                            1039 | 1045 | 1046 | 1047 | 1052 | 1053 | 1054 => {
                                // LevelEventHandler draws these from the client
                                // level's random, not the server's.
                                self.particles.level_random().next_float() * 0.1 + 0.9
                            }
                            1029 | 1030 | 1042 | 1043 | 1044 => {
                                self.world.random.next_float() * 0.1 + 0.9
                            }
                            1026 | 1027 => {
                                let r = self.particles.level_random();
                                (r.next_float() - r.next_float()) * 0.2 + 1.0
                            }
                            1019 | 1021 => {
                                (self.world.random.next_float() - self.world.random.next_float())
                                    * 0.2
                                    + 1.0
                            }
                            _ => 1.0,
                        };
                        // Clients play these sounds from their level events, not
                        // captured for the others.
                        if !matches!(
                            id,
                            1019 | 1021
                                | 1026
                                | 1027
                                | 1029
                                | 1030
                                | 1039
                                | 1045
                                | 1052
                                | 1053
                                | 1054
                                | 1501
                                | 1503
                        ) {
                            self.audio.play(name, c, volume, pitch);
                        }
                    }
                }
                LevelEvent::ContainerLootTable { pos, tag } => {
                    let key = (pos.x, pos.y, pos.z);
                    if let Some(mut entity) = self.world.block_entities.remove(&key) {
                        self.take_loot_table(key, &tag, &mut entity);
                        self.world.block_entities.insert(key, entity);
                        self.world.modified.insert((pos.x >> 4, pos.z >> 4));
                    }
                }
                // A prime the mods refused is silent: no fuse sound, no game event.
                LevelEvent::Sound { pos, ref sound, .. }
                    if self.tnt_refused == Some(pos)
                        && &**sound == "minecraft:entity.tnt.primed" => {}
                LevelEvent::GameEvent {
                    pos,
                    event: "minecraft:prime_fuse",
                    ..
                } if self.tnt_refused == Some(pos) => self.tnt_refused = None,
                LevelEvent::Sound {
                    pos,
                    sound,
                    volume,
                    pitch,
                } => {
                    let c = [
                        f64::from(pos.x) + 0.5,
                        f64::from(pos.y) + 0.5,
                        f64::from(pos.z) + 0.5,
                    ];
                    let name = if sound.contains(':') {
                        sound.to_string()
                    } else {
                        format!("minecraft:{sound}")
                    };
                    self.audio.play(&name, Some(c), volume, pitch);
                }
                // Handled as the behaviour emits it (GameLevel::emit).
                LevelEvent::DropResources { pos, state } => {
                    self.with_level(|_, level, _| level.drop_resources_now(pos, state));
                }
                LevelEvent::FallingBlock {
                    pos,
                    state,
                    hurt_per_distance,
                } => {
                    let mut damage = horizon_client::falling_block::Damage::falling(
                        &self.res.facts().get(state).name,
                    );
                    if let Some(per_distance) = hurt_per_distance {
                        damage.hurt_entities = true;
                        damage.per_distance = per_distance;
                    }
                    let p = Vec3::new(
                        f64::from(pos.x) + 0.5,
                        f64::from(pos.y),
                        f64::from(pos.z) + 0.5,
                    );
                    self.next_entity += 1;
                    self.falling.push(FallingBlock {
                        attachment_lifetime: Default::default(),
                        rotation: [0.0, 0.0],
                        join: Default::default(),
                        id: self.next_entity,
                        state,
                        start_pos: pos,
                        position: p,
                        previous: p,
                        velocity: Vec3::ZERO,
                        age: 0,
                        no_gravity: false,
                        portal: Default::default(),
                        drop_item: true,
                        fall_distance: 0.0,
                        on_ground: false,
                        stuck: Vec3::ZERO,
                        damage,
                        saved: None,
                    });
                }
                LevelEvent::Player { pos, effect, .. } => match effect {
                    PlayerEffect::OpenMenu => self.open_block_effect_menu([pos.x, pos.y, pos.z]),
                    PlayerEffect::Sleep if self.beds_destroy_on_use() => {
                        self.destroy_bed_on_use([pos.x, pos.y, pos.z])?
                    }
                    PlayerEffect::Sleep => self.try_sleep([pos.x, pos.y, pos.z]),
                    PlayerEffect::GiveItem(item) => {
                        if let Some(rest) = self.add_stack_to_inventory(ItemStack {
                            item,
                            count: 1,
                            damage: 0,
                            extra: None,
                        }) {
                            self.pop_resource([pos.x, pos.y, pos.z], rest);
                        }
                    }
                    // Popped as the behaviour emits it (GameLevel::emit).
                    PlayerEffect::DropItem(_) => {}
                    PlayerEffect::EatCake => {
                        // CakeBlock.eat
                        self.award_custom_stat("eat_cake_slice", 1);
                        if let Some(p) = self.player.as_mut() {
                            let food = &mut p.health.food;
                            food.food = (food.food + 2).min(20);
                            food.saturation = (food.saturation + 0.4).min(food.food as f32);
                        }
                    }
                    PlayerEffect::RespawnAnchor => {
                        self.respawn_anchor_used([pos.x, pos.y, pos.z])?
                    }
                    PlayerEffect::InteractLoot(_) => {}
                },
                LevelEvent::PrimeTnt { pos, rot } => self.prime_tnt_event(pos, rot),
                LevelEvent::BlockEvent {
                    pos,
                    state,
                    kind: "dispense",
                } => self.dispense_from(pos, state)?,
                LevelEvent::BlockEvent {
                    pos, kind: "golem", ..
                } => self.check_golem_spawn([pos.x, pos.y, pos.z])?,
                LevelEvent::BlockEvent {
                    pos,
                    kind: "command_block",
                    ..
                } => self.command_block_tick(pos),
                LevelEvent::BlockEvent {
                    pos,
                    kind: "structure_block",
                    ..
                } => self.structure_block_neighbor_changed((pos.x, pos.y, pos.z)),
                LevelEvent::BlockEvent {
                    pos,
                    kind: "test_block",
                    ..
                } => self.test_block_neighbor_changed((pos.x, pos.y, pos.z)),
                LevelEvent::BlockEvent {
                    pos,
                    kind: "test_block_tick",
                    ..
                } => self.reset_test_block((pos.x, pos.y, pos.z)),
                LevelEvent::BlockEvent {
                    pos,
                    kind: "brushable",
                    ..
                } => self.brushable_tick([pos.x, pos.y, pos.z]),
                LevelEvent::BlockEvent {
                    pos, kind: "hatch", ..
                } => {
                    // FrogspawnBlock.hatchFrogspawn: 2-5 tadpoles.
                    let at = [
                        f64::from(pos.x) + 0.5,
                        f64::from(pos.y) + 0.5,
                        f64::from(pos.z) + 0.5,
                    ];
                    self.audio
                        .play("minecraft:block.frogspawn.hatch", Some(at), 1.0, 1.0);
                    let count = 2 + self.world.random.next_int_bound(4).unwrap_or(0);
                    for _ in 0..count {
                        // getRandomTadpolePositionOffset: 0.2..0.8 inside the block.
                        let r = &mut self.world.random;
                        let off = |r: &mut horizon_core::random::LegacyRandom| {
                            f64::from(r.next_float()) * 0.6 + 0.2
                        };
                        let (dx, dz) = (off(r), off(r));
                        let spawn = [
                            f64::from(pos.x) + dx,
                            f64::from(pos.y) - 0.5,
                            f64::from(pos.z) + dz,
                        ];
                        let _ = self.spawn_mob(
                            "minecraft:tadpole",
                            spawn,
                            true,
                            creatures::SpawnSite::Plain,
                        );
                    }
                }
                LevelEvent::BlockEvent {
                    pos,
                    kind: "hatch_sniffer",
                    ..
                } => self.hatch_sniffer([pos.x, pos.y, pos.z]),
                LevelEvent::BlockEvent {
                    pos,
                    state,
                    kind: "hatch_turtles",
                } => self.hatch_turtles([pos.x, pos.y, pos.z], state),
                LevelEvent::BlockEvent {
                    pos,
                    kind: "portal_spawn",
                    ..
                } => self.portal_spawn([pos.x, pos.y, pos.z]),
                LevelEvent::BlockEvent {
                    pos,
                    state,
                    kind: "spawn_ghastling",
                } => self.spawn_ghastling([pos.x, pos.y, pos.z], state),
                LevelEvent::GameEvent { pos, event, state } => {
                    self.sculk_level_event(pos, event, state)
                }
                LevelEvent::BlockEvent {
                    pos,
                    state,
                    kind: "shrieker_respond",
                } => self.shrieker_block_event(pos, state),
                LevelEvent::BlockEntityEvent { pos, a, b, .. } => {
                    self.block_entity_event([pos.x, pos.y, pos.z], a, b)
                }
                // NoteBlock.getCustomSoundId: the skull above's note block sound,
                // played at volume 3 (vanilla's pitch for a head is 1).
                LevelEvent::BlockEvent {
                    pos,
                    kind: "note_custom_sound",
                    ..
                } => {
                    if let Some(BlockEntity::Skull(skull)) =
                        self.world.block_entities.get(&(pos.x, pos.y + 1, pos.z))
                        && let Some(sound) = skull.note_block_sound.as_deref()
                    {
                        let at = [
                            f64::from(pos.x) + 0.5,
                            f64::from(pos.y) + 0.5,
                            f64::from(pos.z) + 0.5,
                        ];
                        self.audio.play(sound, Some(at), 3.0, 1.0);
                    }
                }
                // `level.addParticle`: only the local player's own click draws it.
                LevelEvent::BlockEvent {
                    kind: "candle_smoke",
                    ..
                } if self.lan_acting.is_some() => {}
                LevelEvent::BlockEvent {
                    pos,
                    state,
                    kind: "candle_smoke",
                } => self.candle_smoke([pos.x, pos.y, pos.z], state),
                // The block at `pos` has just replaced `state`: Block.pushEntitiesUp.
                LevelEvent::BlockEvent {
                    pos,
                    state,
                    kind: "push_entities_up",
                } => self.push_entities_up(pos, state),
                LevelEvent::GrowTree { .. }
                | LevelEvent::PlaceFeature { .. }
                | LevelEvent::BlockEvent { .. } => {}
            }
        }
        Ok(())
    }
    /// Menus opened by PlayerEffect::OpenMenu.
    fn open_block_effect_menu(&mut self, pos: [i32; 3]) {
        if self.fake_blocks(mod_api::FakeEffect::Menu) {
            return;
        }
        let name = self
            .res
            .facts()
            .get(self.world.block(pos[0], pos[1], pos[2]))
            .name
            .clone();
        let key = (pos[0], pos[1], pos[2]);
        if self.chest_menu_blocked(key, &name) {
            // The use still succeeds, but no menu, stat or piglin anger.
            return;
        }
        // BaseContainerBlockEntity.createMenu: a lock the opener cannot
        // unlock refuses the menu with vanilla's action bar and sound.
        if self.refuse_locked_container(key, &name) {
            return;
        }
        if &*name == "minecraft:lectern" {
            self.open_lectern(pos);
            return;
        }
        if horizon_client::command_block::Mode::of_block(&name).is_some() {
            self.open_command_block(key);
            return;
        }
        // The game master blocks' screens open on the player's own client
        // (`StructureBlockEntity.usedBy`); the server has nothing to open.
        if matches!(
            &*name,
            "minecraft:structure_block"
                | "minecraft:jigsaw"
                | "minecraft:test_block"
                | "minecraft:test_instance_block"
        ) {
            return;
        }
        // SignBlock.useWithoutItem: the text editor, not a container.
        let block = self.world.block(pos[0], pos[1], pos[2]);
        if self.res.facts().get(block).sign_form().is_some() {
            self.use_sign_empty_hand(pos);
            return;
        }
        if &*name == "minecraft:crafting_table" {
            self.craft_size = 3;
            self.craft_table = Some(pos);
        } else if &*name == "minecraft:ender_chest" {
            self.open_ender_chest(key);
            // EnderChestBlock.useWithoutItem
            self.award_custom_stat("open_enderchest", 1);
            self.anger_piglins(true);
            // EnderChestBlockEntity.startOpen: the first opener's CONTAINER_OPEN
            // (ContainerOpenersCounter.incrementOpeners), as for chests below.
            if self.container_opener_event(key, true) {
                self.post_block_game_event(
                    "minecraft:container_open",
                    [key.0, key.1, key.2],
                    Some(horizon_client::mobs::Who::Player),
                    None,
                );
                self.container_edge_sound(key, true);
            }
        } else if matches!(&*name, "minecraft:beacon" | "minecraft:enchanting_table")
            && self.open_station(pos, &name)
        {
            // Station menus whose block has a block entity, not a container.
            self.world.block_entities.get_or_insert_with(key, || {
                BlockEntity::for_block(&name).unwrap_or(BlockEntity::Chest(Vec::new()))
            });
        } else if storage::is_shulker_box(&name) && !self.shulker_can_open(pos) {
            // ShulkerBoxBlock.canOpen: the lid has no room; nothing opens.
        } else if let Some(entity) = BlockEntity::for_block(&name) {
            self.world.block_entities.get_or_insert_with(key, || entity);
            horizon_client::spans::time("open_loot", || self.unpack_opened_loot(key, &name));
            let centre = [
                f64::from(key.0) + 0.5,
                f64::from(key.1) + 0.5,
                f64::from(key.2) + 0.5,
            ];
            horizon_client::spans::time("open_maps", || {
                self.resolve_container_maps(key, centre);
            });
            self.container = Some(key);
            self.open_container_slots = Some((key, self.open_container_size()));
            let first = self.container_opener_event(key, true);
            if first {
                self.post_block_game_event(
                    "minecraft:container_open",
                    [key.0, key.1, key.2],
                    Some(horizon_client::mobs::Who::Player),
                    None,
                );
            }
            let stat = match &*name {
                "minecraft:chest" => "open_chest",
                "minecraft:trapped_chest" => "trigger_trapped_chest",
                "minecraft:barrel" => "open_barrel",
                "minecraft:hopper" => "inspect_hopper",
                "minecraft:dropper" => "inspect_dropper",
                "minecraft:dispenser" => "inspect_dispenser",
                n if storage::is_shulker_box(n) => "open_shulker_box",
                _ => "",
            };
            if !stat.is_empty() {
                self.award_custom_stat(stat, 1);
            }
            // Opening a piglin-guarded container angers the piglins who see it.
            if self.guarded_by_piglins(&name) {
                self.anger_piglins(true);
            }
            if first {
                self.container_edge_sound(key, true);
            }
        } else if self.open_mod_menu(key) {
            // A mod block's container (`native_block_entity_container`).
            self.container = Some(key);
        } else if self.open_station(pos, &name) {
            // Workstation menus: game/blocks/stations.rs.
        } else {
            self.message = Some((
                format!(
                    "{} is not available yet",
                    name.trim_start_matches("minecraft:").replace('_', " ")
                ),
                Instant::now(),
            ));
            return;
        }
        self.set_screen(Screen::Inventory);
        self.capture(false);
        self.right = false;
    }
    /// `DecoratedPotBlock.playerWillDestroy`: a pot mined with an item in
    /// `breaks_decorated_pots`, unless an enchantment prevents shattering, is
    /// cracked first (flags 260), so it shatters into its sherds.
    fn crack_mined_pot(&mut self, pos: [i32; 3], state: BlockStateId) -> BlockStateId {
        let registry = &self.pack.assets.registry;
        let Some(mut pot) = registry
            .state(state)
            .filter(|s| s.name == "minecraft:decorated_pot")
            .cloned()
        else {
            return state;
        };
        let Some(held) = self.held() else {
            return state;
        };
        let breaks = self
            .res
            .items()
            .item_tags
            .get("minecraft:breaks_decorated_pots")
            .is_some_and(|t| t.contains(&*held.item));
        let enchantments = &self.stations().enchantments;
        // `EnchantmentHelper.hasTag`.
        let kept = held
            .all_enchantments()
            .iter()
            .any(|(e, _)| enchantments.in_tag("minecraft:prevents_decorated_pot_shattering", e));
        if !breaks || kept {
            return state;
        }
        pot.properties.insert("cracked".into(), "true".into());
        let Ok(cracked) = registry.resolve(&pot) else {
            return state;
        };
        let at = BlockPos::new(pos[0], pos[1], pos[2]);
        self.with_level(|b, level, ticks| b.set_block(level, ticks, at, cracked, 260));
        cracked
    }
    fn break_block_as_player(&mut self, pos: [i32; 3]) -> Result<bool> {
        // DebugStickItem.canDestroyBlock: a left click selects a property and
        // never destroys the block.
        if !self.debug_stick_can_destroy(pos)? {
            return Ok(false);
        }
        // ServerPlayerGameMode.destroyBlock.
        if self.block_action_restricted(pos) {
            return Ok(false);
        }
        if !self
            .res
            .items()
            .can_destroy_block(self.held(), self.creative)
        {
            return Ok(false);
        }
        // ServerPlayerGameMode.destroyBlock (and MultiPlayerGameMode.destroyBlock
        // on the client): a GameMasterBlock stays when the player cannot use
        // game master blocks (Player.canUseGameMasterBlocks).
        let state = self.world.block(pos[0], pos[1], pos[2]);
        if !self.can_use_game_master_blocks()
            && is_game_master_block(&self.res.facts().get(state).name)
        {
            return Ok(false);
        }
        // Native mods, once the game allowed the break: a canceled one keeps
        // the block, and a guest's acknowledgement puts it back in its world.
        let mut canceled = false;
        let mut exp_to_drop = None;
        self.native_break_block(pos, state, &mut canceled, &mut exp_to_drop);
        if canceled {
            return Ok(false);
        }
        let broken = state;
        let facts = self.res.facts().get(state).clone();
        let held = self.held().cloned();
        let key = (pos[0], pos[1], pos[2]);
        self.unpack_loot(container_loot::LootTarget::Block(key), None);
        let loot_entity = if self.creative {
            None
        } else {
            self.loot_block_entity(pos, &facts.name)
        };
        let live = self.live_block_entity(key, &facts.name);
        if let Some((entity, _)) = &live {
            self.pending
                .record_from(&self.world, pos, entity, &facts.name);
        } else if let Some(tag) = spill::saved_entity(&self.world, pos) {
            self.pending.record_saved(pos, &tag);
        }
        let hive = match self.world.block_entities.get(&key) {
            Some(BlockEntity::Beehive(h)) => self.hive_state(pos).map(|s| ((**h).clone(), s)),
            _ => None,
        };
        let filled_hive = hive
            .as_ref()
            .is_some_and(|(h, (honey, _))| !h.bees.is_empty() || honey.is_some_and(|l| l > 0));
        let creative = self.creative;
        // The block's parts first, then playerWillDestroy and Level.removeBlock.
        let (removed, state, harvest) =
            self.player_destroy_block_removal(pos, state, &facts, held.as_ref(), filled_hive);
        if !creative {
            // The stack as the removal left it: a listener may have swapped it.
            let held = self.held().cloned();
            if removed {
                self.mine_block_with_held(&facts.name, facts.hardness);
            }
            if removed && harvest {
                // Block.playerDestroy: the mined statistic, the exhaustion and the drops.
                self.award_stat("mined", &facts.name, 1);
                if let Some(p) = self.player.as_mut() {
                    p.cause_food_exhaustion(0.005);
                }
                let stacks = self.block_loot_at(pos, state, held.as_ref(), loot_entity);
                let silk = held
                    .as_ref()
                    .is_some_and(|h| h.enchantment_level("minecraft:silk_touch") > 0);
                // Block.dropResources: the item entities, then spawnAfterBreak's
                // experience, with the mods' say between the two.
                let breaker = Some(self.native_acting_id());
                let dropped = self.player_block_dropped(
                    (pos, state),
                    (breaker, held.as_ref(), live),
                    (stacks, exp_to_drop),
                );
                let canceled = dropped.canceled;
                if let Some(kept) = self.break_collect.as_mut() {
                    kept.extend(dropped.items.into_iter().map(|item| item.stack));
                    if dropped.experience > 0 {
                        self.spawn_orbs(pos.map(|v| f64::from(v) + 0.5), dropped.experience);
                    }
                } else {
                    self.spawn_block_drops(pos, dropped);
                }
                // BeehiveBlock.playerDestroy: silk touch keeps the bees in, so
                // the trigger counts them; otherwise they are out already.
                if let Some((hive, (honey, facing))) = hive {
                    let bees = if silk { hive.bees.len() as i64 } else { 0 };
                    if !silk {
                        self.hive_broken_by_player(pos, hive, honey, facing);
                    }
                    self.trigger_bee_nest_destroyed(broken, held.as_ref(), bees);
                }
                // InfestedBlock.spawnAfterBreak: its silverfish comes out unless the
                // tool has silk touch (#prevents_infested_spawns) or blocks don't drop.
                if facts.name.starts_with("minecraft:infested_")
                    && !silk
                    && !canceled
                    && self
                        .ui
                        .rules
                        .bool_at(horizon_client::gamerules::vanilla::BLOCK_DROPS)
                {
                    let at = [
                        f64::from(pos[0]) + 0.5,
                        f64::from(pos[1]),
                        f64::from(pos[2]) + 0.5,
                    ];
                    if let Some(id) = self.spawn_mob(
                        "minecraft:silverfish",
                        at,
                        false,
                        creatures::SpawnSite::Plain,
                    ) {
                        self.mob_poof(id);
                    }
                }
                // TurtleEggBlock.playerDestroy: after the drops, one egg of a
                // mined stack is taken and the rest stay.
                horizon_threads::work::clear_work();
                self.with_level(|b, level, ticks| {
                    b.turtle_egg_destroyed_by_player(
                        level,
                        ticks,
                        BlockPos::new(pos[0], pos[1], pos[2]),
                        broken,
                    );
                });
            }
            // IceBlock.playerDestroy: the ice melts back into water (the
            // #prevents_ice_melting tool tag holds only silk touch).
            let prevents_melting = held
                .as_ref()
                .is_some_and(|h| h.enchantment_level("minecraft:silk_touch") > 0);
            if removed {
                horizon_threads::work::clear_work();
                self.with_level(|b, level, ticks| {
                    b.ice_destroyed_by_player(
                        level,
                        ticks,
                        BlockPos::new(pos[0], pos[1], pos[2]),
                        broken,
                        prevents_melting,
                    );
                });
            }
        }
        self.pending.clear(pos);
        self.process_level_events()?;
        Ok(removed)
    }
    /// The `ItemStack.interactLivingEntity` fallback in `Player.interactOn`,
    /// currently for native Mob entries after the entity's own action passed.
    fn item_interact_mob(&mut self, target: Option<InteractTarget>) -> bool {
        if self.spectator() || !mod_api::__private::has_part_hook(mod_api::HookId::ItemInteractMob)
        {
            return false;
        }
        let (entity, kind) = if self.ecs.is_active() {
            let Some(id) = target.map(|t| t.id).or_else(|| self.ecs_mob_hit().map(|h| h.0)) else {
                return false;
            };
            let Some(k) = self.ecs.kind(id).filter(|_| self.mob_alive(id)) else {
                return false;
            };
            (id as i32, horizon_client::command::args::normalize(k))
        } else {
            let Some(index) = self.mob_interaction_target(target) else {
                return false;
            };
            let mob = &self.remote_entities[index];
            if mob.removed || !mob.alive() {
                return false;
            }
            (mob.id as i32, horizon_client::command::args::normalize(&mob.kind.id))
        };
        let Some(stack) = self.held().filter(|stack| stack.count > 0).cloned() else {
            return false;
        };
        let mut result = None;
        self.native_item_interact_mob(self.hand, entity, &kind, &stack, &mut result);
        result.is_some_and(|answer| self.interaction_consumed(answer))
    }
    /// `ItemStack.applyAfterUseComponentSideEffects`: a used stack with a
    /// `use_cooldown` component starts its entry (`UseCooldown.apply`). A
    /// server's guest also gets `ClientboundCooldownPacket`
    /// (`ServerItemCooldowns.onCooldownStarted`), queued for their turn's end.
    pub(super) fn apply_use_cooldown(&mut self, stack: &ItemStack) {
        let default = self
            .res
            .items()
            .get(&stack.item)
            .and_then(|i| i.use_cooldown.clone());
        let Some(component) = horizon_client::cooldowns::component(stack, default.as_ref()) else {
            return;
        };
        let group = horizon_client::cooldowns::group(stack, default.as_ref());
        let mut cancel = false;
        if let (Some(player), Some(name)) = (self.lan_acting_uuid, self.lan_acting_name.clone()) {
            self.native_player_item_cooldown(
                player,
                &name,
                &stack.item,
                component.ticks(),
                &mut cancel,
            );
        }
        if !cancel {
            self.add_cooldown(group, component.ticks());
        } else if self.lan_acting.is_some() {
            // The guest's client started the cooldown itself; 0 ends it there.
            self.guest_cooldowns.push((group, 0));
        }
    }
    /// `ItemCooldowns.addCooldown` (a guest also gets the packet).
    pub(super) fn add_cooldown(&mut self, group: Arc<str>, ticks: i32) {
        self.cooldowns.add(Arc::clone(&group), ticks);
        if self.lan_acting.is_some() {
            self.guest_cooldowns.push((group, ticks));
        }
    }
    fn tick_mining(&mut self) -> Result<()> {
        if self.destroy_delay > 0 {
            self.destroy_delay -= 1;
        }
        if !self.left || self.dialog_open() {
            self.mining = None;
            return Ok(());
        }
        let Some(hit) = self.pick() else {
            self.mining = None;
            return Ok(());
        };
        // Player.blockActionRestricted: spectators, and adventure players
        // without a matching can_break item.
        if self.block_action_restricted(hit.pos)
            || !self
                .res
                .items()
                .can_destroy_block(self.held(), self.creative)
        {
            self.mining = None;
            return Ok(());
        }
        if self.creative {
            if self.destroy_delay == 0 {
                self.break_block(hit.pos)?;
                self.destroy_delay = 5;
            }
            return Ok(());
        }
        if self.destroy_delay > 0 {
            return Ok(());
        }
        if self.mining.as_ref().is_none_or(|m| m.pos != hit.pos) {
            self.attack_block(hit.pos)?;
            self.mining = Some(Mining {
                pos: hit.pos,
                progress: 0.0,
                per_tick: 0.0,
                ticks: 0,
            });
        }
        let state = self.world.block(hit.pos[0], hit.pos[1], hit.pos[2]);
        if self.res.facts().get(state).hardness < 0.0 {
            // The server still counts its destroy ticks on an unbreakable block.
            let m = self.mining.as_mut().ok_or("mining")?;
            m.per_tick = 0.0;
            m.ticks += 1;
            let (pos, ticks) = (m.pos, m.ticks);
            self.destroy_progress_event(pos, hit.face, ticks);
            return Ok(());
        }
        let progress = self.destroy_progress(hit.pos);
        self.swing_arm();
        let m = self.mining.as_mut().ok_or("mining")?;
        m.per_tick = progress;
        m.progress += progress;
        m.ticks += 1;
        // An instant break at START sends no destroy events.
        if m.progress < 1.0 || m.ticks > 1 {
            let (pos, ticks) = (m.pos, m.ticks);
            self.destroy_progress_event(pos, hit.face, ticks);
        }
        let m = self.mining.as_mut().ok_or("mining")?;
        // ServerPlayerGameMode breaks a player's block at once only when it is instant (START's
        // insta-mine); otherwise it waits for the client's STOP_DESTROY_BLOCK
        // (`guest_stop_digging`), never breaking it on its own count. Breaking it here would
        // take the block from under the player's crack before the player's client predicts the
        // break, so it gets neither particles nor sound.
        if m.progress >= 1.0 && (self.lan_acting.is_none() || m.ticks == 1) {
            let pos = m.pos;
            self.mining = None;
            self.break_block(pos)?;
            self.destroy_delay = 5;
        }
        Ok(())
    }
    /// A column whose surface is a full solid, non-fluid, non-leaf block open to the sky.
    fn land_column(&self, x: i32, z: i32) -> Option<i32> {
        if self.dim == Dim::Nether {
            // Below the bedrock ceiling: a solid floor with two air blocks above.
            for y in (32..120).rev() {
                let floor = self.res.facts().get(self.world.block(x, y - 1, z));
                if floor.full_collision
                    && floor.fluid == Fluid::None
                    && self.res.facts().is_air(self.world.block(x, y, z))
                    && self.res.facts().is_air(self.world.block(x, y + 1, z))
                {
                    return Some(y);
                }
            }
            return None;
        }
        let mut y = self.world.top_y() - 1;
        while y > self.world.bottom_y() {
            let s = self.world.block(x, y, z);
            if !self.res.facts().is_air(s) {
                let f = self.res.facts().get(s);
                let ok = f.full_collision
                    && f.fluid == Fluid::None
                    && !f.name.contains("leaves")
                    && f.hardness >= 0.0;
                return ok.then_some(y + 1);
            }
            y -= 1;
        }
        None
    }
    /// Landing rules (farmland trampling, turtle eggs) wrote or removed the block
    /// raw: finish `Level.setBlock`'s neighbour updates, so a crop on trampled
    /// farmland breaks. The local player's and every server player's landings.
    /// The level events they send go out too: a turtle egg's break particles
    /// and sound (`decreaseEggs`' 2001, `Level.destroyBlock`'s for the last).
    ///
    /// A mod hears the change first (`native_entity_change_block_pre`, Bukkit's
    /// `EntityChangeBlockEvent`): cancelling puts the block back and skips the
    /// updates and the events, so nothing the change would have caused happens -
    /// the crop on trampled farmland stays, because breaking it is the
    /// neighbour update below and not a write of its own.
    ///
    /// The game event the rule posts comes back for the caller to post with the
    /// landing player in place (`post_landing_vibration`).
    fn landing_block_updates(
        &mut self,
        player: Option<u128>,
        event: &horizon_client::landing::FallEvent,
    ) -> Option<LandingVibration> {
        use horizon_client::landing::FallEvent;
        use horizon_world::block_landing::LandingEvent;
        let mut vibration = None;
        // The block the landing changed, what it replaced, and what it became.
        let (position, from, to, flags) = match *event {
            FallEvent::Write {
                position,
                from,
                state,
                flags,
            } => (position, from, state, flags as i32),
            FallEvent::Destroy {
                position, state, ..
            } => {
                // `Level.destroyBlock`: BLOCK_DESTROY with no source, after the
                // write (`TurtleEggBlock.decreaseEggs`' last egg).
                vibration = Some(LandingVibration {
                    event: "minecraft:block_destroy",
                    position,
                    state,
                    from_entity: false,
                });
                (position, state, self.air_state(), level_flags::UPDATE_ALL)
            }
            // `FarmlandBlock.turnToBaseBlock`'s BLOCK_CHANGE from the entity that
            // trampled it, and `TurtleEggBlock.decreaseEggs`' BLOCK_DESTROY
            // (`Context.of(state)`, no source): the write is already applied.
            FallEvent::BlockEvent {
                kind,
                position,
                state,
                with_entity,
            } => {
                // A cancelled change put the block back: nothing happened.
                let current = self.world.block(position.x, position.y, position.z);
                let cancelled = match kind {
                    LandingEvent::BlockChange => current != state,
                    LandingEvent::BlockDestroy => current == state,
                };
                if cancelled {
                    return None;
                }
                return Some(LandingVibration {
                    event: match kind {
                        LandingEvent::BlockChange => "minecraft:block_change",
                        LandingEvent::BlockDestroy => "minecraft:block_destroy",
                    },
                    position,
                    state,
                    from_entity: matches!(kind, LandingEvent::BlockChange) && with_entity,
                });
            }
            FallEvent::LevelEvent {
                event,
                position,
                data,
            } => {
                self.level_event_at(event, [position.x, position.y, position.z], data);
                return None;
            }
            _ => return None,
        };
        if self.landing_change_cancelled(player, position, from, to) {
            self.world
                .set(position.x, position.y, position.z, from)
                .ok();
            return None;
        }
        if matches!(event, FallEvent::Destroy { .. }) {
            self.level_event_at(2001, [position.x, position.y, position.z], from.0 as i32);
        }
        self.with_level(|b, level, ticks| b.notify_written(level, ticks, position, flags));
        vibration
    }
    /// Post a landing's block game event (`FarmlandBlock.turnToBaseBlock`'s
    /// BLOCK_CHANGE, `TurtleEggBlock.decreaseEggs`' BLOCK_DESTROY) as the player
    /// whose landing it was, who must be in place (`as_guest`). A sculk sensor, a
    /// calibrated sensor and a warden hear it.
    fn post_landing_vibration(&mut self, vibration: LandingVibration) {
        let at = [
            vibration.position.x,
            vibration.position.y,
            vibration.position.z,
        ];
        let who = vibration
            .from_entity
            .then_some(horizon_client::mobs::Who::Player);
        self.post_block_game_event(vibration.event, at, who, Some(vibration.state));
    }
    /// `Blocks.AIR.defaultBlockState`: what a landing's destroy leaves.
    fn air_state(&self) -> BlockStateId {
        self.pack
            .assets
            .registry
            .default_state("minecraft:air")
            .unwrap_or(BlockStateId(0))
    }
    /// Whether a mod cancelled a landing's block change
    /// (`native_entity_change_block_pre`): asked with the two blocks' names and
    /// the player whose landing it was (`None` when no player did it).
    fn landing_change_cancelled(
        &mut self,
        player: Option<u128>,
        position: BlockPos,
        from: BlockStateId,
        to: BlockStateId,
    ) -> bool {
        if !mod_api::__private::listened(mod_api::HookId::EntityChangeBlockPre) {
            return false;
        }
        let mut cancel = false;
        self.native_entity_change_block_pre(native_hooks::EntityChangeBlockPreCall {
            player,
            entity: None,
            kind: "minecraft:player",
            pos: [position.x, position.y, position.z],
            from,
            to,
            cancel: &mut cancel,
        });
        cancel
    }
    fn tick(&mut self) -> Result<()> {
        self.expire_client_cracks();
        if self.agent.hold_server {
            return Ok(());
        }
        self.abort_open_transactions();
        self.update_interaction_ranges();
        // A logical server has no window to lead and never follows another server.
        if !self.is_logical_server() {
            self.tick_client_lead()?;
        }
        self.tick_first_level()
    }
    /// What the tick runs after the local player's own update: effects, the
    /// world's entities and blocks, and the network hub.
    fn tick_world(&mut self, running: bool) -> Result<()> {
        // Entities added since the last tick join the level before it ticks.
        span("mods_joins_flush", || self.native_joins_flush(false, 0));
        span("share_spawn_conditions", || self.share_spawn_conditions());
        horizon_client::mobs::sulfur_cube::set_tnt_explodes(
            self.ui
                .rules
                .bool_at(horizon_client::gamerules::vanilla::TNT_EXPLODES),
        );
        self.update_chunk_tickets();
        self.score_destroyed_entities();
        horizon_client::spans::time("tick_effects", || self.tick_effects(running));
        // The window's own ticks: a game that is not a logical server runs them, as it always did.
        let window = !self.is_logical_server();
        if window {
            self.tick_sign_editor();
        }
        // Player.tick's ItemCooldowns.tick for the window's player (a
        // server's players count theirs down in `tick_guests`).
        self.shield_cooldown = self.shield_cooldown.saturating_sub(1);
        self.cooldowns.tick();
        horizon_client::spans::time("tick_using", || self.tick_using());
        if window {
            self.tick_bundle_hover();
        }
        // The player's digging comes in with its packets, before the level
        // ticks; ServerLevel.tick then runs the scheduled and random block
        // ticks before any entity, so what they drop or spawn (dispensed
        // items, popped crops) moves on the tick it appears, as in vanilla.
        // On the server thread the window is a hub guest: drain its packets
        // here too, before falling-block schedules run (#4510).
        horizon_client::spans::time("tick_local_hand", || self.tick_local_hand())?;
        horizon_client::spans::time("postprocess_chunks", || self.postprocess_chunks())?;
        let mut sent = None;
        if running {
            horizon_client::spans::time("tick_lan_packets", || self.tick_lan_packets());
            sent = horizon_client::spans::time("tick_blocks", || self.tick_blocks())?;
            // What the blocks spawned (a dispensed minecart, a lit TNT, a
            // placed boat) joins before the entities tick.
            span("mods_joins_flush", || self.native_joins_flush(false, 0));
            span("lan_mark_synced", || self.lan_mark_synced());
            // `EntityTickList.forEach`: what joined before the entity pass
            // (function summons, dropped or dispensed items) ticks in it;
            // what joins during it waits for the next tick
            // (`ensureActiveIsNotIterated`). Primed TNT and falling blocks
            // keep their own packet cutoff (`ordered.announced`, #10063).
            self.ticked_entities_through = self.next_entity;
        }
        // ChunkMap still ticks while /tick freeze holds block/entity ticks.
        if !running {
            span("save_chunks_eagerly", || self.save_chunks_eagerly());
        }
        self.begin_ordered_ticks();
        if running {
            self.tick_explicit_knots();
            self.tick_summoned_eyes();
            self.ordered_before(ordered_tick::Batch::ArmorStands)?;
            horizon_client::spans::time("tick_armor_stands", || self.tick_armor_stands())?;
            horizon_client::spans::time("tick_cushions", || self.tick_cushions());
            // ServerLevel.tick: the arrivals of this server tick are in the level
            // (`addDuringTeleport`), and each vehicle handles its portal as its
            // own tick opens.
            span("portal_arrivals", || self.land_portal_arrivals());
            span("vehicle_portals", || self.handle_vehicle_portals());
            self.ordered_before(ordered_tick::Batch::Boats)?;
            self.tick_vehicle_watch();
            let from = self.lan_synced_from();
            horizon_client::spans::time("tick_thunder", || self.tick_thunder());
            self.lan_synced_since(from);
            horizon_client::spans::time("tick_lecterns", || self.tick_lecterns());
            horizon_client::spans::time("tick_maps", || self.tick_maps());
            self.tick_lodestone_compasses();
            horizon_client::spans::time("tick_bolts", || self.tick_bolts());
            self.ordered_before(ordered_tick::Batch::Tridents)?;
            self.ordered_before(ordered_tick::Batch::Rockets)?;
            horizon_client::spans::time("tick_ominous_item_spawners", || {
                self.tick_ominous_item_spawners()
            });
        }
        if running {
            horizon_client::spans::time("entity_portals", || {
                self.handle_entity_portals();
                self.handle_end_gateway_portals();
            });
            self.ordered_before(ordered_tick::Batch::Items)?;
            horizon_client::spans::time("tick_items", || self.tick_items());
            {
                span("hoppers_catch_items", || self.hoppers_catch_items());
            }
            // Merged, burnt, despawned or picked up.
            span("mods_leaves_flush", || self.native_leaves_flush());
            span("mods_joins_flush", || self.native_joins_flush(false, 0));
        }
        horizon_client::spans::time("spawn_loaded_entities", || self.spawn_loaded_entities());
        // The ride the player's file names, once the vehicle is in the world.
        self.restore_root_vehicle();
        if running {
            self.ordered_before(ordered_tick::Batch::Mobs)?;
            horizon_client::spans::time("tick_mobs", || self.tick_mobs())?;
            // Dead, despawned or gone into a hive.
            span("mods_leaves_flush", || self.native_leaves_flush());
            span("mod_viewers", || self.tick_mod_container_viewers());
            // Orbs, primed TNT and falling blocks still to tick, in join order.
            self.run_ordered_ticks(horizon_client::join_seq::JoinSeq::MAX, false)?;
            horizon_client::spans::time("tick_experience", || self.tick_experience());
            // Merged, despawned or picked up.
            span("mods_leaves_flush", || self.native_leaves_flush());
            horizon_client::spans::time("entity_portals", || {
                self.find_entity_portals();
                self.resolve_arrivals();
            });
        }
        // The rest of the tick's spawns join, and what left is told of.
        span("mods_joins_flush", || self.native_joins_flush(false, 0));
        span("mods_leaves_flush", || self.native_leaves_flush());
        // ServerLevel.tick broadcasts the changed blocks after the scheduled and
        // random ticks (`tick_blocks`), before the block events, the entities
        // and the block entities: what those change goes out with the next
        // tick's. A piston's moved block reaches the clients once their own
        // copy of it finished moving, so a slow tick rate shows it slide
        // (#8430).
        if let (Some(sent), Some(log)) = (sent, self.world.block_log.as_mut()) {
            self.world.block_log_held = log.split_off(sent.min(log.len()));
        }
        horizon_client::spans::time("tick_lan", || self.tick_lan(false));
        let held = std::mem::take(&mut self.world.block_log_held);
        if let Some(log) = self.world.block_log.as_mut() {
            log.extend(held);
        }
        span("tick_friends", || self.tick_friends());
        if window {
            horizon_client::spans::time("tick_hand", || self.tick_hand());
        }
        horizon_client::spans::time("tick_sleep", || self.tick_sleep());
        span("sleep_status", || self.announce_sleep_status());
        span("sleep_status", || self.free_left_beds());
        if running {
            horizon_client::spans::time("tick_end", || self.tick_end())?;
            horizon_client::spans::time("end_gateways", || {
                self.find_end_gateway_portals();
                self.resolve_end_gateway_arrivals()
            })?;
            span("end_gateways", || self.keep_gateway_destinations());
        }
        if running {
            // Primed TNT and falling blocks that joined since the spawn packets.
            self.run_ordered_ticks(horizon_client::join_seq::JoinSeq::MAX, true)?;
            span("late_wind_pushes", || self.apply_late_wind_pushes());
        }
        // Everything that has joined has had its spawn packet and, if it joined
        // before the entity passes (TNT a piston or repeater primed), its first
        // tick in the late pass above (#10063).
        self.ordered.announced = horizon_client::join_seq::JoinSeq::upcoming();
        if running {
            horizon_client::spans::time("tick_moving_pistons", || self.tick_moving_pistons())?;
            horizon_client::spans::time("tick_block_entities", || self.tick_block_entities())?;
            horizon_client::spans::time("tick_sculk_entities", || self.tick_sculk_entities());
            horizon_client::spans::time("tick_sculk_blocks", || self.tick_sculk_blocks())?;
        }
        if running {
            if window {
                // ClientLevel.tickBlockEntities (not while paused or frozen):
                // chest lids, books, shulker boxes, bells, conduits, heads.
                horizon_client::spans::time("tick_block_entity_anims", || {
                    self.tick_block_entity_anims()
                });
            } else {
                // The bells' and conduits' server tickers (not while paused or frozen).
                horizon_client::spans::time("tick_block_entity_servers", || {
                    self.tick_block_entity_servers()
                });
            }
            horizon_client::spans::time("tick_particles", || self.tick_particles());
            if window {
                span("tick_client_ambient", || self.tick_client_ambient());
            }
        }
        // ServerLevel.tick's `entityManager.tick()`, frozen or not, after every
        // block entity ticker (the bells, conduits and gateways above too): the
        // chunks loaded before this pass have their entities loaded from the next.
        self.world.entities_pending.clear();
        if window {
            span("tick_frame_visuals", || self.tick_frame_visuals(running));
        }
        if running {
            // MinecraftServer.tickServer: GameTestTicker.SINGLETON.tick().
            span("tick_game_tests", || self.tick_game_tests());
        }
        // ServerBossEvent broadcasts, after this level's entities have ticked.
        span("publish_boss_bars", || self.publish_boss_bars());
        // Minecraft.handleKeybinds: held use repeats only while no item is in
        // use (a raised shield or a drawn bow places and uses nothing).
        if window
            && self.right
            && self.use_delay == 0
            && self.using.is_none()
            && !self.dialog_open()
            && self.sleeping.is_none()
        {
            self.use_with_hand()?;
            self.use_delay = 4;
        }
        if running {
            horizon_client::spans::time("lan_note_joined", || self.lan_note_joined());
        }
        Ok(())
    }

    pub(super) fn close_container(&mut self) {
        // `Gui.setScreen` clears the old screen's focus before it goes: the
        // key that closed the container (Tab bound to the inventory) has
        // already moved the focus, and it must not outlive the screen.
        self.pad_clear_focus();
        self.mod_browser_closed();
        self.close_station();
        self.mount_closed();
        self.recipe_ui.reset_screen();
        let mut returned: Vec<ItemStack> = self.carried.take().into_iter().collect();
        returned.extend(self.craft_grid.iter_mut().filter_map(Option::take));
        for stack in returned {
            if let Some(rest) = self.add_stack_to_inventory(stack)
                && let Some(eye) = self.eye_position()
            {
                // AbstractContainerMenu.dropOrPlaceInInventory: `drop(stack, false)`.
                self.throw_from_eyes(rest, eye, false, false);
            }
        }
        self.craft_size = 2;
        self.craft_table = None;
        self.open_container_slots = None;
        if let Some(key) = self.container.take() {
            // A cart's or chest boat's items go back to it.
            self.close_cart_container(key);
            let last = self.container_opener_event(key, false);
            if last {
                self.post_block_game_event(
                    "minecraft:container_close",
                    [key.0, key.1, key.2],
                    Some(horizon_client::mobs::Who::Player),
                    None,
                );
                self.container_edge_sound(key, false);
            }
            self.world.modified.insert((key.0 >> 4, key.2 >> 4));
        }
        self.close_ender_chest();
    }

    /// `mark_motion` ids of entities that aren't mobs or dropped items (boats, minecarts, armor
    /// stands, item frames, paintings, primed TNT, falling blocks, rockets, tridents, thrown
    /// potions): their entity id under this tag.
    pub(super) const MOTION_OBJECT: u64 = 3 << 32;
    /// An item frame's held item, apart from the frame (one id may mark one run).
    pub(super) const MOTION_FRAME_ITEM: u64 = 4 << 32;
    /// A block entity, by its position (`motion_block_entity`).
    pub(super) const MOTION_BLOCK_ENTITY: u64 = 5 << 32;
    /// A block a piston moves, by the cell it moves to (`motion_at`).
    pub(super) const MOTION_MOVING_BLOCK: u64 = 6 << 32;
    /// The player's own model (third person, and a shader pack's shadow pass).
    pub(super) const MOTION_OWN_PLAYER: u64 = 7 << 32;

    fn orb_geometry<'a>(
        &self,
        camera: glam::DVec3,
        orbs: impl Iterator<Item = (&'a horizon_client::entities::ExperienceOrb, [f64; 3], f32)>,
        orb_view: Option<&horizon_client::gfx::JoinedFrusta>,
        out: &mut Vec<Vertex>,
    ) {
        if let Some(r) = self
            .gui
            .atlas
            .sprites
            .get("entity/experience/experience_orb")
            .copied()
        {
            let (yaw, pitch) = (self.yaw.to_radians(), self.pitch.to_radians());
            let forward = glam::Vec3::new(
                -yaw.sin() * pitch.cos(),
                -pitch.sin(),
                yaw.cos() * pitch.cos(),
            );
            let right = forward.cross(glam::Vec3::Y).normalize_or_zero();
            let up = right.cross(forward).normalize_or_zero();
            for (o, at, partial) in orbs {
                let p = glam::DVec3::new(at[0], at[1] + 0.1, at[2]) - camera;
                let c = p.as_vec3();
                if orb_view.is_some_and(|view| {
                    forward.dot(c) < 0.0 && !entity_culling::orb_in_view(view, c)
                }) {
                    continue;
                }
                let icon = o.icon();
                let (du, dv) = ((r.u1 - r.u0) / 4.0, (r.v1 - r.v0) / 4.0);
                let (u0, v0) = (r.u0 + (icon % 4) as f32 * du, r.v0 + (icon / 4) as f32 * dv);
                let t = (o.age as f32 + partial) / 2.0;
                let red = ((t.sin() + 1.0) * 0.5 * 255.0) as u32;
                let blue = (((t + 4.188_79).sin() + 1.0) * 0.1 * 255.0) as u32;
                let color = 0x8000_0000 | blue << 16 | 255 << 8 | red;
                // `ExperienceOrbRenderer.getBlockLightLevel` (the block light
                // plus 7, clamped) at the block the orb's eye is in.
                let light = self.world.light(
                    o.position.x.floor() as i32,
                    o.light_probe_y().floor() as i32,
                    o.position.z.floor() as i32,
                );
                let packed = o.render_light(i32::from(light));
                // `ExperienceOrbRenderer.submit`: the quad runs from
                // (-0.5, -0.25) to (0.5, 0.75) and is scaled by 0.3, so the
                // sprite hangs a quarter of its height below the orb's centre.
                let s = horizon_client::entities::ORB_QUAD_SCALE;
                for (k, uv) in horizon_client::entities::ORB_QUAD.iter().zip([
                    [u0, v0 + dv],
                    [u0 + du, v0 + dv],
                    [u0 + du, v0],
                    [u0, v0],
                ]) {
                    let v = c + right * (k[0] * s) + up * (k[1] * s);
                    out.push(Vertex {
                        normal: [0; 4],
                        pos: v.to_array(),
                        uv,
                        // ExperienceOrbRenderer: `RenderTypes.entityTranslucent`; in HDR
                        // output the orbs shine (entity.wgsl hdr_entity_mark).
                        layer: horizon_client::gfx::OIT_ENTITY
                            | u32::from(horizon_client::mesher::VERY_BRIGHT) << 16,
                        color,
                        light: packed,

                        glint_uv: [0.0, 0.0],
                    });
                }
                self.lit_entity(out.len() - 4..out.len());
            }
        }
    }

    /// `ServerLevel.destroyBlockProgress`: breaker `breaker`'s crack on `pos`
    /// (0..=9, anything else takes it away), for every player within 64 blocks.
    pub(super) fn server_destroy_block_progress(
        &mut self,
        breaker: i32,
        pos: [i32; 3],
        progress: i32,
    ) {
        self.destroy_block_progress(breaker, pos, progress);
        if let Some(host) = self.lan.as_ref() {
            host.hub.broadcast_near(
                [f64::from(pos[0]), f64::from(pos[1]), f64::from(pos[2])],
                64.0,
                &horizon_protocol::packets::play::Clientbound::BlockDestruction(
                    horizon_protocol::packets::play::BlockDestruction {
                        id: breaker,
                        pos,
                        progress: if (0..10).contains(&progress) {
                            progress as u8
                        } else {
                            255
                        },
                    },
                ),
                None,
            );
        }
    }

    /// The mob hit along the view, the distance to it and the point the ray
    /// met its box at, measured from the mob's feet (`EntityHitResult
    /// .getLocation() - entity.position()`, the interact packet's location).
    fn mob_hit(&self) -> Option<(usize, f64, [f64; 3])> {
        if let Some(hit) = self.tick_mob {
            return hit;
        }
        self.mob_hit_within(self.entity_reach)
    }
    /// [`Game::mob_hit`] out to `reach` instead of the entity interaction range
    /// (`ProjectileUtil.getHitResultOnViewVector` with another range).
    pub(super) fn mob_hit_within(&self, reach: f64) -> Option<(usize, f64, [f64; 3])> {
        let (eye, dir) = self.aim()?;
        let mut best: Option<(usize, f64, [f64; 3])> = None;
        for (i, m) in self.remote_entities.iter().enumerate() {
            // ProjectileUtil.getEntityHitResult: the mob the player rides
            // shares its root vehicle and is never the hit, nor is the
            // camera entity the pick starts from. EntitySelector.CAN_BE_PICKED
            // is isPickable — `!isRemoved`, not alive: a health-0 fake player
            // still answers a click.
            if m.removed || Some(m.id) == self.riding || Some(m.id) == self.remote_camera_id() {
                continue;
            }
            if let Some(t) = ray_box(eye, dir, reach, m.bounds())
                && best.is_none_or(|b| t < b.1)
            {
                let point = Vec3::new(
                    eye[0] + dir[0] * t,
                    eye[1] + dir[1] * t,
                    eye[2] + dir[2] * t,
                );
                best = Some((i, t, interact_location(point, m.position)));
            }
        }
        best
    }
    /// `Player.isSweepAttack` with the player's speed as it is, then
    /// `native_sweep_attack`, on either side: whether the hit on `target` sweeps.
    pub(super) fn sweep_decision(&mut self, scale: f32, crit: bool, target: &str) -> bool {
        let Some(p) = self.player.as_ref() else {
            return false;
        };
        let sword = self.held().is_some_and(|s| {
            self.res
                .items()
                .item_tags
                .get("minecraft:swords")
                .is_some_and(|tag| tag.contains(&*s.item))
        });
        let vanilla = p.is_sweep_attack(scale, crit, sword);
        let (mut sweeping, mut canceled) = (vanilla, false);
        // `setCanceled` skips later mods and keeps the current `sweeping`.
        self.native_sweep_attack(target, vanilla, &mut sweeping, &mut canceled);
        sweeping
    }
    /// `Player.attack`'s `isSweepAttack`, decided before the hit as vanilla does
    /// (`crit`: the hit on a living target is critical): where the sweep plays,
    /// none without one. Reads the charge, so before the hit resets it.
    pub(super) fn sweep_before_hit(&mut self, target: &str, crit: bool) -> Option<[f64; 3]> {
        let scale = self.attack_strength_scale();
        self.sweep_attack_position(scale, crit, target)
    }
    /// `Player.attack`'s sounds when a vehicle takes the hit (it always does, and
    /// can't be hit critically): knockback when sprinting at full strength, then
    /// strong or weak, or the `sweep` decided before the hit around the vehicle's
    /// box `area`. Before the hit resets the charge (`Player.onAttack`).
    pub(super) fn vehicle_attack_sounds(
        &mut self,
        area: Aabb,
        sweep: Option<[f64; 3]>,
        magic: Option<i32>,
    ) {
        self.attack_effects(area, sweep, None, false, mace::Spared::Nothing, magic);
    }
    /// A mob explosion (`ServerLevel.explode` with `ExplosionInteraction.MOB`:
    /// blocks are kept without mobGriefing, else the mob_explosion_drop_decay
    /// rule picks decay). Mods are asked through `can_entity_grief` for
    /// `source`, the exploding entity (the rule alone without one).
    fn explode_with(
        &mut self,
        source: Option<u32>,
        center: [f64; 3],
        radius: f32,
        fire: bool,
        blast: explosion::ExplosionSource,
    ) -> Result<()> {
        let interaction = if !self.can_entity_grief(source) {
            explosion::BlockInteraction::Keep
        } else {
            self.destroy_type(horizon_client::gamerules::vanilla::MOB_EXPLOSION_DROP_DECAY)
        };
        self.server_explode_from(center, radius, interaction, fire, blast)
    }
    /// Copies of a save batch carrying its live block entities and save data.
    fn with_entities<'a>(
        &self,
        chunks: impl IntoIterator<Item = &'a Arc<horizon_client::chunk::Chunk>>,
    ) -> Vec<Arc<horizon_client::chunk::Chunk>> {
        horizon_client::save::chunks_with_block_entities(
            chunks,
            &self.world.block_entities,
            &self.world.block_names,
        )
        .into_iter()
        .map(|mut c| {
            self.attach_save_data(&mut c);
            Arc::new(c)
        })
        .collect()
    }
    fn tick_mobs(&mut self) -> Result<()> {
        self.sync_mob_death_protection();
        self.tick_ecs()?;
        if !self.entity_attachments.is_empty() {
            self.prune_entity_attachments();
        }
        self.native_mobs_equipment();
        self.tick_equipment_attributes();
        self.position_rider();
        self.tick_player_stat_counters();
        self.tick_scoreboard();
        self.load_bossbars();
        self.tick_worldborder();
        self.tick_subtitles();
        self.spawn_phantoms();
        self.tick_advancements();
        self.tick_player_model();
        Ok(())
    }

    fn finish_open_world(&mut self, prepared: world_loading::PreparedWorld) -> Result<()> {
        self.reset_tutorial();
        let world_loading::PreparedWorld {
            save_root,
            shapes,
            level,
            world,
            biomes,
            streaming,
            save,
            session_lock,
            rand_sequences,
            locator,
            loot,
            owner_migration,
        } = prepared;
        self.server_loot = loot;
        let dim = Dim::from_id(&level.dimension);
        self.level_shapes = shapes;
        self.world_root = save_root.clone();
        // The root moves before the prepared level is installed. Until then a
        // final save must not run: the lock below only marks tag tables.
        self.world_adopted = false;
        self.owner_migration = owner_migration;
        // Saves and chunk readers share the same placeholder IDs.
        self.world_biomes = Some(biomes);
        self.world_biomes_base = self.world_biomes.clone();
        horizon_client::crash::set_detail(
            "World",
            format!("{} ({})", level.name, save_root.display()),
        );
        self.end.set_locator(locator);
        self.share_spawn_conditions();
        self.score = Default::default();
        let stack = self.open_world_datapacks(&level.enabled_features)?;
        let root = self.options.resources.clone();
        self.open_world_tables(&stack, &root)?;
        self.invalidate_tagged_shader_ids();
        self.damage_tag_generation = self.damage_tag_generation.wrapping_add(1);
        self.resolve_parts()?;
        // A code-mod switch can have rebuilt the root since `Game::new`.
        self.structure_spawns = horizon_client::mobs::spawn_selection::StructureSpawns::load(&root);
        // Native tag reads use the lock to tell that the tag tables are ready.
        self.session_lock = Some(session_lock);
        // Keep the no-pack preload; a world's packs replace it atomically.
        // A later /reload keeps this session's synchronized registries.
        lan::sync::open_world_sync_data(
            &self.options.resources,
            &self.res.core.item_data,
            &pack_files(&self.world_tables),
            &self.health_resources.damage,
        )
        .map_err(|e| format!("world registries: {e}"))?;
        // Warm this world's pack-aware loot data before gameplay can unpack
        // a container's pending table on its first inventory access.
        self.load_server_loot();
        // MinecraftServer's constructor reads CustomBossEvents before any
        // server event.
        self.bosses = Default::default();
        self.load_bossbars();
        // ServerLevel's constructor reads its Raids saved data before any
        // server event; their randoms are seeded on the first raid tick.
        {
            self.read_raids();
        }
        self.announce_data_bound(mod_api::TagsUpdateCause::ServerDataLoad { reload: false });
        self.announce_data_maps_updated(mod_api::DataMapsUpdateCause::ServerReload);
        self.sync_recipe_content();
        reset_world_progress(&mut self.adv, &mut self.stats);
        self.prefetch_advancement_tree(&stack);
        self.restore_player_state = true;
        self.rand_sequences = rand_sequences;
        // ServerLevel.getWorldBorder: the level in place reads its own saved
        // border when it is made, before any command (the parked levels read
        // theirs in `open_level`).
        self.border = self.load_border(dim);
        self.set_mode_id(level.mode());
        self.previous_mode = level.previous_game_mode.map(game_mode::mode_from_id);
        let mut inventory = self.load_saved_inventory(&level);
        if self.creative && level.inventory.is_empty() && level.position.is_none() {
            for (i, name) in [
                "minecraft:grass_block",
                "minecraft:stone",
                "minecraft:oak_planks",
                "minecraft:glass",
                "minecraft:torch",
                "minecraft:oak_log",
                "minecraft:chest",
                "minecraft:furnace",
                "minecraft:diamond_pickaxe",
            ]
            .iter()
            .enumerate()
            {
                inventory[i] = Some(ItemStack {
                    item: Arc::from(*name),
                    count: 1,
                    damage: 0,
                    extra: None,
                });
            }
        }
        let (yaw, pitch) = self
            .options
            .look
            .unwrap_or((level.rotation[0], level.rotation[1]));
        self.yaw = yaw;
        self.pitch = pitch;
        self.selected = level.selected_slot.clamp(0, 8) as usize;
        self.streaming = streaming;
        self.world = world;
        self.inventory = inventory;
        self.save = self.with_profession_save_snapshot(save);
        // MinecraftServer.prepareLevels: the saved tickets come back.
        self.tickets = {
            horizon_client::ticket_levels::ChunkTickets::with_saved_portal(
                self.save.load_portal_tickets(),
            )
        };
        // The ride the player's file names, once the vehicle is in the world.
        self.pending_vehicle = level.root_vehicle;
        self.level = level;
        self.ecs = self.new_ecs_level(dim, self.level.seed);
        self.check_mod_mismatch();
        self.world_adopted = true;
        // ServerRecipeBook.loadUntrusted: recipes the manager does not have are dropped.
        let index = &world_stations!(self).recipe_index;
        let book = &mut self.level.recipe_book;
        book.recipes.retain(|r| index.contains(r));
        book.to_be_displayed.retain(|r| index.contains(r));
        self.enter_opened_dim(dim);
        // LevelStorageAccess.getIconFile: this save's icon.png is what the
        // Select World list shows.
        self.world_icon = Some(WorldIcon {
            path: save_root.join("icon.png"),
            last_attempt: None,
        });
        self.world_flat = horizon_client::save::WorldGen::read(&save_root)
            .is_some_and(|w| w.preset == horizon_client::save::WorldPreset::Flat);
        self.world_debug = horizon_client::save::WorldGen::read(&save_root)
            .is_some_and(|w| w.preset == horizon_client::save::WorldPreset::Debug);
        self.block_ticks = self.levels.new_ticks(self.level.seed as i32);
        self.tick_chunks.clear();
        self.clear_entities();
        self.player = None;
        self.sleeping = None;
        self.sleep_timer = 0;
        self.player_beds.clear();
        self.spawn = None;
        self.respawn_config = self
            .level
            .respawn_elsewhere
            .as_ref()
            .map(respawn_anchor::RespawnConfig::from_saved);
        self.spawn_origin = None;
        self.spawn_hook_fired = false;
        self.spawn_hook_cancelled = false;
        self.spawn_spiral = Default::default();
        self.player_spawn = None;
        self.creatures.reset_trader();
        self.creatures.cat = Default::default();
        self.creatures.custom_spawners.clear();
        self.portals = Default::default();
        self.experience = Experience {
            level: self.level.xp_level,
            progress: self.level.xp_progress,
            total: self.level.xp_total,
            score: self.level.score,
        };
        self.restore_effects();
        self.mining = None;
        self.container = None;
        let kinds = std::mem::take(&mut self.be_anims.kinds);
        self.be_anims = horizon_client::block_entity_anim::BlockEntityAnims {
            kinds,
            ..Default::default()
        };
        self.carried = None;
        self.using = None;
        self.set_screen(Screen::Playing);
        self.hide_title();
        self.clear_lod();
        self.apply_settings();
        let facts = self.dimension_type_of(dim);
        self.streaming.sky_light = facts.height.has_skylight;
        self.streaming.nether_shading =
            facts.cardinal_light == horizon_world::dimension_type::CardinalLight::Nether;
        self.load_world_state()?;
        self.native_world_opened(dim);
        self.open_saved_portal_levels()
    }
    fn finish_close_world(&mut self) -> Result<()> {
        // The save screen has finished every write before releasing storage.
        self.ui.dispatcher = Default::default();
        self.server_loot.set(None);
        self.server_items = None;
        self.session_lock = None;
        self.clear_world_parts();
        self.world_adopted = false;
        self.world_icon = None;
        horizon_client::crash::clear_detail("World");
        self.item_activation.reset();
        self.mod_view_distances.take();
        self.bosses = Default::default();
        self.streaming.paused = true;
        self.world = Game::world_for_mods(self.res.facts().air, 0);
        self.player = None;
        // `LocalPlayer`'s constructor: `KeyMapping.resetToggleKeys`, so a sneak or sprint toggled
        // in this world is not still on in the next one.
        self.reset_toggle_keys();
        self.sleeping = None;
        self.sleep_timer = 0;
        self.player_beds.clear();
        self.clear_entities();
        self.set_screen(Screen::Playing);
        self.clear_lod();
        // Clear sync data (registries, dialog tags) so dialogs don't leak
        // between worlds.
        lan::sync::forget_sync_data();
        lan::sync::forget_server_sources();
        let mut title = self
            .new_title(std::env::current_dir()?.join("saves"))
            .with_user(|| self.options.player_name());
        title.last_server = self.settings.file.string("lastServer");
        title.skip_multiplayer_warning = self.settings.file.bool("skipMultiplayerWarning");
        self.show_title(title);
        self.capture(false);
        Ok(())
    }
    /// Let go of every entity of the level (saved first): the next world
    /// opened must not inherit them, nor save them as its own.
    pub(super) fn clear_entities(&mut self) {
        self.clear_pending_mob_actions();
        self.remote_entities.clear();
        self.dropped.clear();
        self.orbs.clear();
        self.creatures.projectiles.clear();
        self.falling.clear();
        self.transient_blocks.clear();
        self.tnts.clear();
        self.boats.clear();
        self.carts.clear();
        self.armor_stands.clear();
        self.cushions.clear();
        self.item_frames.clear();
        self.paintings.clear();
        self.others.clear();
        self.dragon.discard();
        self.crystals.clear();
        self.breath_clouds.clear();
        self.tridents.clear();
        self.rockets.clear();
        self.bolts.clear();
        self.clear_client_particles();
        self.riding = None;
        self.persist = Default::default();
        self.missing_professions.clear();
        self.entity_attachments.clear();
        self.clear_remote_objects();
        self.clear_remote_projectiles();
        self.levels.clear();
        self.entity_vertices.clear();
        self.see_through_vertices.clear();
        self.glowing_vertices.get_mut().clear();
        self.shadow_only_vertices.get_mut().clear();
        self.clear_entity_marks();
    }
    /// Voxel traversal that also stops at fluid source blocks (bucket pick).
    fn pick_fluid(&self) -> Option<Hit> {
        let (origin, dir) = self.aim()?;
        let mut t = 0.0;
        let mut last = origin.map(|v| v.floor() as i32);
        let mut first = true;
        while t <= self.reach() {
            let p = [
                origin[0] + dir[0] * t,
                origin[1] + dir[1] * t,
                origin[2] + dir[2] * t,
            ];
            let b = p.map(|v| v.floor() as i32);
            // `BlockGetter.traverseBlocks` visits the block the ray starts in
            // first: the eyes' own water, under water (#3787).
            if first || b != last {
                let state = self.world.block(b[0], b[1], b[2]);
                let f = self.res.facts().get(state);
                let face = [last[0] - b[0], last[1] - b[1], last[2] - b[2]];
                if f.fluid != Fluid::None
                    && f.fluid_level == 8
                    && !f.fluid_falling
                    && !f.waterlogged
                    && f.render_shape != horizon_client::blocks::RenderShape::Model
                {
                    if !first {
                        return Some(Hit {
                            pos: b,
                            face,
                            point: p,
                            inside: false,
                        });
                    }
                    let above = self.world.block(b[0], b[1] + 1, b[2]);
                    let same_above = self.res.facts().get(above).fluid == f.fluid;
                    if let Some((face, point)) =
                        clip_start_source(origin, dir, self.reach(), b, same_above)
                    {
                        return Some(Hit {
                            pos: b,
                            face,
                            point,
                            // A start in the shape hits at the eyes.
                            inside: point == origin,
                        });
                    }
                } else if !first && !self.res.facts().is_air(state) && f.fluid == Fluid::None {
                    // `ClipContext.Block.OUTLINE`: the ray stops at the block's
                    // outline, not its cell; it passes through the empty part
                    // of a torch, a fence or a flower to the water behind.
                    if let Some(hit) =
                        outline_stop(origin, dir, self.reach(), b, face, || self.outline_boxes(b))
                    {
                        return Some(hit);
                    }
                }
                last = b;
                first = false;
            }
            t += 0.05;
        }
        None
    }
    /// The cauldron at `pos`, and whether water is above it
    /// (`CauldronInteractions.isUnderWater`).
    pub(super) fn cauldron_at(
        &self,
        pos: [i32; 3],
    ) -> Option<(horizon_world::chunk::BlockState, bool)> {
        let registry = self.pack.assets.registry;
        let id = self.world.block(pos[0], pos[1], pos[2]);
        let state = registry
            .state(id)
            .filter(|s| cauldron_interactions::snapshot().contains_block(&s.name))
            .cloned()?;
        let above = self.world.block(pos[0], pos[1] + 1, pos[2]);
        let under_water = cauldrons::is_under_water(self.res.facts(), above);
        Some((state, under_water))
    }

    /// What `BlockTransformer.transformBlock` would do with a held axe, hoe
    /// or shovel on the block at `pos`, without doing it: the new state, its
    /// sound, loot table, the durability it costs, its particle and its
    /// transform type.
    pub(super) fn block_transform(
        &self,
        stack: &ItemStack,
        hit: &Hit,
    ) -> Option<(BlockStateId, String, Option<String>, u32, String, String)> {
        let blocking = self
            .offhand
            .as_ref()
            .is_some_and(|s| self.blocks_attacks(s).is_some());
        self.block_transform_in(
            stack,
            hit,
            TransformContext {
                world: &self.world,
                player: self.player.is_some().then(|| self.own_entity_id() as u32),
                hand: self.hand,
                blocking,
                simulated: false,
            },
        )
    }

    fn block_transform_in(
        &self,
        stack: &ItemStack,
        hit: &Hit,
        context: TransformContext<'_>,
    ) -> Option<(BlockStateId, String, Option<String>, u32, String, String)> {
        use horizon_client::block_transform::Around;
        let TransformContext {
            world,
            player,
            hand,
            blocking,
            simulated,
        } = context;
        let Hit { pos, face, .. } = *hit;
        struct Column<'w> {
            world: &'w World,
            registry: &'w horizon_world::registry::BlockRegistry,
            pos: [i32; 3],
        }
        impl Around for Column<'_> {
            fn state(&self, o: [i32; 3]) -> horizon_world::chunk::BlockState {
                let id =
                    self.world
                        .block(self.pos[0] + o[0], self.pos[1] + o[1], self.pos[2] + o[2]);
                self.registry.state(id).cloned().unwrap_or_else(|| {
                    horizon_world::chunk::BlockState {
                        name: "minecraft:air".into(),
                        properties: Default::default(),
                    }
                })
            }
        }
        let id = self
            .res
            .items()
            .get(&stack.item)
            .and_then(|i| i.block_transformer.clone())?;
        // playerHasBlockingItemUseIntent: a shield in the off hand is raised
        // instead, unless sneaking.
        if blocking {
            return None;
        }
        let face_name = match face {
            [0, -1, 0] => "down",
            [0, 1, 0] => "up",
            [0, 0, -1] => "north",
            [0, 0, 1] => "south",
            [-1, 0, 0] => "west",
            _ => "east",
        };
        let registry = self.pack.assets.registry;
        let block = world.block(pos[0], pos[1], pos[2]);
        let around = Column {
            world,
            registry,
            pos,
        };
        let vanilla = self
            .res
            .transformers()
            .apply(self.items(), registry, &id, face_name, &around)
            .and_then(|(_, t, state)| Some((registry.resolve(&state).ok()?, t)));
        let result = |new, t: &horizon_client::block_transform::Transform| {
            (
                new,
                t.sound.clone(),
                t.loot.clone(),
                t.damage,
                t.particle.clone(),
                t.transform_type.clone(),
            )
        };
        let mapped = self.axe_mapped(&id, block);
        if mod_api::__private::mods().is_empty() {
            if let Some((new, ability)) = mapped {
                return Some(Self::tool_ability_result(new, ability));
            }
            let (new, t) = vanilla?;
            return Some(result(new, t));
        }
        use mod_api::ToolAbility::*;
        let actions: &[_] = match id.as_str() {
            "minecraft:axe" => &[AxeStrip, AxeScrape, AxeWaxOff],
            "minecraft:shovel" => &[ShovelFlatten],
            "minecraft:hoe" => &[HoeTill],
            _ => return vanilla.map(|(new, t)| result(new, t)),
        };
        for &ability in actions {
            let mut state = match mapped {
                Some((new, from)) => (from == ability).then_some(new),
                None => vanilla
                    .as_ref()
                    .and_then(|(state, t)| vanilla_tool_state(&id, &t.particle, ability, *state)),
            };
            let matches_vanilla = state.is_some() && mapped.is_none();
            let mut cancel = false;
            let tool = mod_api::ToolUse::from(mod_api::__private::built::ToolUse {
                player,
                hand,
                pos,
                face,
                click: hit.point,
                ability,
                simulated,
            });
            self.mod_tool_modification(stack, block, &tool, &mut state, &mut cancel);
            if cancel {
                return None;
            }
            if let Some(new) = state {
                if matches_vanilla {
                    let (_, t) = vanilla?;
                    return Some(result(new, t));
                }
                return Some(Self::tool_ability_result(new, ability));
            }
        }
        None
    }
    /// `BlockTransformer.transformBlock` for a held axe, hoe or shovel.
    fn use_block_transformer(&mut self, stack: &ItemStack, hit: &Hit) -> Result<bool> {
        let pos = hit.pos;
        let face = hit.face;
        let Some((new, sound, loot, damage, particle, transform_type)) =
            self.block_transform(stack, hit)
        else {
            return Ok(false);
        };
        // BlockTransformer.transformBlock: ITEM_USED_ON_BLOCK before the swap.
        self.trigger_block("minecraft:item_used_on_block", pos, stack);
        // A loot table drops on the clicked face, before the block changes.
        if loot.as_deref() == Some("minecraft:till/rooted_dirt") {
            self.pop_resource_from_face(pos, face, ItemStack::new("minecraft:hanging_roots", 1));
        }
        let old = self.world.block(pos[0], pos[1], pos[2]);
        self.level_set(pos, new)?;
        // BlockTransformer.TransformParticle.send.
        self.transform_particle(&particle, pos);
        // `Level.playSound(player, ...)`: the actor's client already played
        // the strip/scrape/till sound in `predict_item_on_block` (#4681).
        let at = [
            f64::from(pos[0]) + 0.5,
            f64::from(pos[1]) + 0.5,
            f64::from(pos[2]) + 0.5,
        ];
        if self.lan_acting.is_some() {
            self.guest_player_sounds.push((sound.clone(), at, 1.0, 1.0));
        } else {
            self.audio.play(&sound, Some(at), 1.0, 1.0);
        }
        // ItemStack.useOn: a use that acted counts as one of the item.
        self.award_stat("used", &stack.item, 1);
        self.damage_held(damage);
        self.post_block_game_event("minecraft:block_change", pos, self.sculk_actor(), Some(new));
        if transform_type == "copper_chest" {
            self.transform_copper_chest_neighbour(pos, old, &particle);
        }
        Ok(true)
    }
    /// `HoneycombItem.useOn`'s `Level.playSound(player, ...)`: the actor's client
    /// already played it in `predict_item_on_block`.
    fn wax_on_sound(&mut self, at: [f64; 3]) {
        let sound = "minecraft:item.honeycomb.wax_on";
        if self.lan_acting.is_some() {
            self.guest_block_sounds
                .push((sound.to_owned(), at, 1.0, 1.0));
        } else {
            self.audio.play(sound, Some(at), 1.0, 1.0);
        }
    }
    /// `BlockTransformer.transformBlock`'s `COPPER_CHEST` extra: a double
    /// chest's other half hears a block change and gets the same particle.
    fn transform_copper_chest_neighbour(
        &mut self,
        pos: [i32; 3],
        old: BlockStateId,
        particle: &str,
    ) {
        let old = self.pack.assets.registry.state(old).cloned();
        let Some(old) = old else { return };
        if !old.name.ends_with("copper_chest") {
            return;
        }
        let kind = old
            .properties
            .get("type")
            .map(String::as_str)
            .unwrap_or("single");
        let Some(facing) = old.properties.get("facing").map(String::as_str) else {
            return;
        };
        // `ChestBlock.getConnectedDirection`: LEFT is facing.getClockWise().
        let (dx, dz) = match (kind, facing) {
            ("left", "north") => (1, 0),
            ("left", "south") => (-1, 0),
            ("left", "west") => (0, -1),
            ("left", "east") => (0, 1),
            ("right", "north") => (-1, 0),
            ("right", "south") => (1, 0),
            ("right", "west") => (0, 1),
            ("right", "east") => (0, -1),
            _ => return,
        };
        let neighbour = [pos[0] + dx, pos[1], pos[2] + dz];
        let state = self.world.block(neighbour[0], neighbour[1], neighbour[2]);
        self.post_block_game_event(
            "minecraft:block_change",
            neighbour,
            self.sculk_actor(),
            Some(state),
        );
        self.transform_particle(particle, neighbour);
    }
    fn set_state_named(&mut self, pos: [i32; 3], name: &str, keep: &[&str]) -> Result<bool> {
        let registry = &self.pack.assets.registry;
        let old = self.world.block(pos[0], pos[1], pos[2]);
        let Some(default) = registry.default_state(name) else {
            return Ok(false);
        };
        let mut state = registry.state(default).cloned().ok_or("state")?;
        if let Some(previous) = registry.state(old) {
            for k in keep {
                if let Some(v) = previous.properties.get(*k) {
                    state.properties.insert((*k).to_owned(), v.clone());
                }
            }
        }
        let id = registry.resolve(&state).unwrap_or(default);
        self.level_set(pos, id)?;
        Ok(true)
    }
    /// Block tool use: distinguish success, failure and fall-through for hand feedback.
    fn use_tool(
        &mut self,
        stack: &ItemStack,
        allow_block_actions: bool,
    ) -> Result<horizon_client::first_person::UseResult> {
        use horizon_client::first_person::UseResult;
        let item = stack.item.trim_start_matches("minecraft:").to_owned();
        let at = |p: [i32; 3]| {
            [
                f64::from(p[0]) + 0.5,
                f64::from(p[1]) + 0.5,
                f64::from(p[2]) + 0.5,
            ]
        };
        // BucketItem has no useOn: a guest's UseItemOn leaves it to UseItem.
        let bucket = horizon_client::items::bucket_contents(&stack.item);
        if matches!(self.guest_use, Some(GuestUse::OnBlock(_)))
            && (item == "bucket" || bucket.is_some())
        {
            return Ok(UseResult::Pass);
        }
        if item == "bucket" {
            let Some(hit) = self.bucket_hit() else {
                return Ok(UseResult::Pass);
            };
            // `BucketItem.use`: `level.mayInteract` and `mayUseItemAt`.
            if self.bucket_use_refused(hit.pos) {
                return Ok(UseResult::Fail);
            }
            let mut canceled = false;
            self.native_fill_bucket(stack, hit.pos, &mut canceled);
            if canceled {
                return Ok(UseResult::Fail);
            }
            if self.is_powder_snow(hit.pos) {
                self.pick_up_powder_snow(hit.pos)?;
                return Ok(UseResult::Success { swing: true });
            }
            // `BucketPickup.pickupBlock`: a fluid source, a waterlogged block
            // or powder snow. A block that gives nothing fails the click.
            let pos = BlockPos::new(hit.pos[0], hit.pos[1], hit.pos[2]);
            let creative = self.creative;
            let Some(filled) =
                self.with_level(|b, level, ticks| b.bucket_pickup(level, ticks, pos, creative))
            else {
                return Ok(UseResult::Fail);
            };
            self.process_level_events()?;
            // `player.awardStat(Stats.ITEM_USED.get(this))`,
            // `getPickupSound` and `level.gameEvent(player, FLUID_PICKUP)`.
            self.award_stat("used", "minecraft:bucket", 1);
            let sound = if filled == "minecraft:lava_bucket" {
                "minecraft:item.bucket.fill_lava"
            } else {
                "minecraft:item.bucket.fill"
            };
            self.audio.play(sound, Some(at(hit.pos)), 1.0, 1.0);
            self.post_block_game_event(
                "minecraft:fluid_pickup",
                hit.pos,
                Some(horizon_client::mobs::Who::Player),
                None,
            );
            let filled = ItemStack::new(filled, 1);
            self.filled_result(filled.clone());
            self.trigger_item("minecraft:filled_bucket", &filled);
            return Ok(UseResult::Success { swing: true });
        }
        // BucketItem.use has its own ray, even when the crosshair hits an entity.
        if !allow_block_actions && bucket.is_none() {
            return Ok(UseResult::Pass);
        }
        let Some(hit) = self.pick() else {
            return Ok(UseResult::Pass);
        };
        let clicked = self.world.block(hit.pos[0], hit.pos[1], hit.pos[2]);
        if let Some((content, sound, mob)) = bucket {
            if self.bucket_use_refused(hit.pos) {
                return Ok(UseResult::Fail);
            }
            let target = self.bucket_target(content, &hit);
            let mut canceled = false;
            self.native_fill_bucket(stack, target, &mut canceled);
            // `mayUseItemAt(blockPos2)`: a block that refuses the fluid passes
            // it to the block beside, so a mod hears that block too (powder
            // snow places like a block and never goes there).
            let beside_too = content != BucketContent::Powder && hit.beside() != target;
            if !canceled && beside_too {
                self.native_fill_bucket(stack, hit.beside(), &mut canceled);
            }
            if canceled
                || self.bucket_empty_cancelled(stack, target)
                || beside_too && self.bucket_empty_cancelled(stack, hit.beside())
            {
                return Ok(UseResult::Fail);
            }
            if let Some(result) = self.player_empty_contents(stack, target, &hit) {
                return Ok(result);
            }
            let pos = BlockPos::new(target[0], target[1], target[2]);
            let [x, y, z] = hit.beside();
            let beside = Some(BlockPos::new(x, y, z));
            // `Player.isShiftKeyDown`: a block that refuses the fluid (any
            // non-air block under Shift) passes it to the block beside, where
            // Shift no longer counts.
            let sneaking = self.keys.held(Mapping::Sneak);
            let creative = self.creative;
            let emptied = self.with_level(|b, level, ticks| {
                b.bucket_empty(
                    level, ticks, pos, beside, content, sound, sneaking, creative,
                )
            });
            self.process_level_events()?;
            let Some(emptied) = emptied else {
                return Ok(UseResult::Fail);
            };
            if self.dimension_type().water_evaporates && content.is_water() {
                self.evaporation_smoke([emptied.x, emptied.y, emptied.z]);
            }
            // `BucketItem.emptyContents` uses `playSound(player, …)`: the
            // actor's client already played the empty / evaporate sound from
            // its DryLevel prediction (#4684).
            self.keep_guest_bucket_sounds(sound);
            if let Some(kind) = mob {
                if let Some(id) = self.spawn_bucket_mob(kind, (target[0], target[1], target[2])) {
                    self.mob_placed(id, stack)?;
                }
                self.post_block_game_event(
                    "minecraft:entity_place",
                    target,
                    self.sculk_actor(),
                    None,
                );
            }
            self.trigger_block("minecraft:placed_block", target, stack);
            self.bucket_emptied(&stack.item);
            return Ok(UseResult::Success { swing: true });
        }
        if self.may_use_on(hit.pos) && self.use_block_transformer(stack, &hit)? {
            return Ok(UseResult::Success { swing: true });
        }
        if self.may_use_on(hit.pos) && item == "honeycomb" {
            let registry = self.pack.assets.registry;
            let old = registry.state(clicked);
            if let Some(state) = self.honeycomb_waxed(clicked) {
                let other = old.and_then(|old| storage::chest_connected_pos(old, hit.pos));
                // HoneycombItem.useOn: ITEM_USED_ON_BLOCK before the swap.
                self.trigger_block("minecraft:item_used_on_block", hit.pos, stack);
                self.level_set(hit.pos, state)?;
                let source = self
                    .lan_acting_uuid
                    .map(horizon_world::level::LevelEventPlayer);
                self.level_event_from(source, 3003, hit.pos, 0);
                self.wax_on_sound(at(hit.pos));
                // HoneycombItem.useOn's game event, and ItemStack.useOn's used statistic.
                self.post_block_game_event(
                    "minecraft:block_change",
                    hit.pos,
                    self.sculk_actor(),
                    Some(state),
                );
                // A double chest's other half: its game event and particles, and
                // a second sound from the clicked block.
                if let Some(other) = other {
                    let now = self.world.block(other[0], other[1], other[2]);
                    self.post_block_game_event(
                        "minecraft:block_change",
                        other,
                        self.sculk_actor(),
                        Some(now),
                    );
                    self.level_event_at(3003, other, 0);
                    self.wax_on_sound(at(hit.pos));
                }
                self.award_stat("used", &stack.item, 1);
                if !self.creative {
                    self.consume_held();
                }
                return Ok(UseResult::Success { swing: true });
            }
        }
        if item == "flint_and_steel" || item == "fire_charge" {
            if !self.may_build() && !self.can_place_on_in_adventure(hit.pos) {
                return Ok(UseResult::Fail);
            }
            let name = self.res.facts().get(clicked).name.clone();
            if self.can_light(clicked, &name) {
                let registry = self.pack.assets.registry;
                let mut state = registry
                    .state(clicked)
                    .cloned()
                    .ok_or("missing lit block state")?;
                state.properties.insert("lit".into(), "true".into());
                self.level_set(hit.pos, registry.resolve(&state)?)?;
                let sound = if item == "fire_charge" {
                    "minecraft:item.firecharge.use"
                } else {
                    "minecraft:item.flintandsteel.use"
                };
                let pitch = horizon_client::items::fire_starter_pitch(
                    item == "fire_charge",
                    &mut self.world.random,
                );
                self.play_block_use_sound(sound, at(hit.pos), pitch);
                // FlintAndSteelItem / FireChargeItem.useOn: BLOCK_CHANGE at the lit block,
                // and ItemStack.useOn's used statistic.
                self.post_block_game_event(
                    "minecraft:block_change",
                    hit.pos,
                    self.sculk_actor(),
                    None,
                );
                self.award_stat("used", &stack.item, 1);
                if item == "fire_charge" {
                    self.consume_held();
                } else {
                    self.damage_held(1);
                }
                return Ok(UseResult::Success { swing: true });
            }
            let target = [
                hit.pos[0] + hit.face[0],
                hit.pos[1] + hit.face[1],
                hit.pos[2] + hit.face[2],
            ];
            if self.can_place_used_fire(target) {
                // With mods, the fire goes in and `BaseFireBlock.onPlace`
                // fills the frame, as vanilla's order is, so that
                // `portal_spawn` is asked once, where vanilla asks it.
                if !mod_api::__private::mods().is_empty() || !self.try_light_portal(target)? {
                    self.place_used_fire(target)?;
                }
                let sound = horizon_client::items::fire_starter_sound(item == "fire_charge");
                let pitch = horizon_client::items::fire_starter_pitch(
                    item == "fire_charge",
                    &mut self.world.random,
                );
                self.play_block_use_sound(sound, at(target), pitch);
                // BLOCK_PLACE: flint and steel names the clicked block, a fire charge the
                // fire. Then ItemStack.useOn's used statistic.
                let placed_at = if item == "fire_charge" {
                    target
                } else {
                    hit.pos
                };
                self.post_block_game_event(
                    "minecraft:block_place",
                    placed_at,
                    self.sculk_actor(),
                    None,
                );
                self.award_stat("used", &stack.item, 1);
                if item == "fire_charge" {
                    self.consume_held();
                } else {
                    self.damage_held(1);
                }
                return Ok(UseResult::Success { swing: true });
            }
            return Ok(UseResult::Fail);
        }
        Ok(UseResult::Pass)
    }
    pub(super) fn can_place_used_fire(&mut self, target: [i32; 3]) -> bool {
        let pos = BlockPos::new(target[0], target[1], target[2]);
        let direction = match ((self.yaw / 90.0 + 0.5).floor() as i32).rem_euclid(4) {
            0 => Direction::South,
            1 => Direction::West,
            2 => Direction::North,
            _ => Direction::East,
        };
        self.with_level(|b, level, ticks| b.fire_can_be_placed_at(level, ticks, pos, direction))
    }
    fn place_used_fire(&mut self, target: [i32; 3]) -> Result<()> {
        let pos = BlockPos::new(target[0], target[1], target[2]);
        let state = self.with_level(|b, level, ticks| b.fire_state_for(level, ticks, pos));
        self.level_set(target, state)
    }
    /// `ExperienceOrb.followNearbyPlayer`'s candidates: each orb goes for the
    /// nearest living player within 8 blocks, the host's own or a guest. Each
    /// is the player's feet, the point the orb homes on and the player's UUID.
    pub(super) fn orb_followed(&self) -> Vec<xp_orb_targeting::OrbCandidate> {
        let spectator = self.spectator();
        let player = self
            .player
            .as_ref()
            .filter(|p| !p.health.dead_or_dying() && !spectator && !self.lan_acting_fake);
        // ExperienceOrb.followNearbyPlayer: each orb goes for the nearest
        // living player within 8 blocks, the host's own or a guest.
        let uuid = self
            .level
            .player_uuid
            .map(horizon_client::lan::playerdata::ints_uuid);
        let mut followed: Vec<xp_orb_targeting::OrbCandidate> = player
            .map(|p| {
                let feet = [p.position.x, p.position.y, p.position.z];
                let eye = f64::from(p.dimensions.eye_height);
                (
                    feet,
                    [feet[0], feet[1] + eye / 2.0, feet[2]],
                    uuid,
                    p.entity_id as u32,
                )
            })
            .into_iter()
            .collect();
        if let Some(host) = self.lan.as_ref() {
            followed.extend(
                host.hub
                    .guests
                    .iter()
                    .filter(|g| {
                        g.placed
                            && g.participates_in_player_simulation()
                            && g.loaded
                            && !g.dead
                            && g.health > 0.0
                    })
                    .filter(|g| !g.spectator())
                    .map(|g| {
                        let p = g.position;
                        (
                            p,
                            [p[0], p[1] + g.eye_height() / 2.0, p[2]],
                            Some(g.uuid),
                            g.entity_id as u32,
                        )
                    }),
            );
        }
        followed
    }
    /// `Player.getVehicle().getBoundingBox()` for the local player's vehicle
    /// (`riding`): a mob, one of `object_target`'s kinds (boat, cart, armour
    /// stand, item frame, painting) or a tracked special — the same chain
    /// `guest_target_box` resolves a guest's vehicle through.
    fn riding_box(&self) -> Option<Aabb> {
        let id = self.riding?;
        if let Some(b) = self.mob_box(id) {
            return Some(b);
        }
        if let Some((lo, hi, _)) = self.object_target(id as i32) {
            return Some(Aabb::new(
                Vec3::new(lo[0], lo[1], lo[2]),
                Vec3::new(hi[0], hi[1], hi[2]),
            ));
        }
        self.tracked_special_box(id as i32)
    }
    /// `Player.giveExperiencePoints`: `add_experience_levels` runs once for each
    /// level gained or lost, and the mods hear of the level change once.
    pub(super) fn give_experience_points(&mut self, points: i32) {
        let Some(points) = self.xp_change(points) else {
            return;
        };
        let old_level = self.experience.level;
        self.experience.add_points(points);
        while self.experience.progress < 0.0 {
            let xp = &self.experience;
            let lost = xp.progress * horizon_client::entities::Experience::needed(xp.level) as f32;
            let had_level = xp.level > 0;
            self.add_experience_levels(-1);
            let xp = &mut self.experience;
            xp.progress = if had_level {
                1.0 + lost / horizon_client::entities::Experience::needed(xp.level) as f32
            } else {
                0.0
            };
        }
        while self.experience.progress >= 1.0 {
            let xp = &mut self.experience;
            xp.progress =
                (xp.progress - 1.0) * horizon_client::entities::Experience::needed(xp.level) as f32;
            self.add_experience_levels(1);
            let xp = &mut self.experience;
            xp.progress /= horizon_client::entities::Experience::needed(xp.level) as f32;
        }
        self.native_player_level_changed(old_level);
    }
    /// A translated message for the player the game acts for: `overlay` is
    /// `sendOverlayMessage` (the action bar), else `sendSystemMessage` (chat).
    /// A guest gets a packet, so its own client translates it.
    fn tell_sleeper(&mut self, key: &str, fallback: &str, overlay: bool) {
        if self.lan_acting.is_some() {
            self.guest_packets
                .push(horizon_protocol::packets::play::Clientbound::SystemChat(
                    horizon_protocol::packets::play::SystemChat {
                        content: horizon_protocol::text::translatable(key, &[]),
                        overlay,
                    },
                ));
        } else if overlay {
            self.message = Some((self.translate_with_fallback(key, fallback), Instant::now()));
        } else {
            self.system_message(horizon_client::text::Text::translate(key, &[]));
        }
    }
    /// ServerPlayer.startSleepInBed: in reach, not obstructed, night or thunder,
    /// and (outside creative) no monsters within 8 blocks; the bed becomes the
    /// respawn point either way.
    fn try_sleep(&mut self, pos: [i32; 3]) {
        if self.fake_blocks(mod_api::FakeEffect::Sleep) {
            return;
        }
        let Some((position, yaw)) = self.player.as_ref().map(|p| (p.position, p.yaw)) else {
            return;
        };
        let state = self.world.block(pos[0], pos[1], pos[2]);
        let props = self
            .pack
            .assets
            .registry
            .state(state)
            .map(|s| s.properties.clone())
            .unwrap_or_default();
        // The head and the foot: the block faces from the foot to the head.
        let d = facing_offset(props.get("facing").map_or("north", String::as_str));
        let (head, foot) = if props.get("part").map(String::as_str) == Some("foot") {
            ([pos[0] + d[0], pos[1], pos[2] + d[2]], pos)
        } else {
            (pos, [pos[0] - d[0], pos[1], pos[2] - d[2]])
        };
        // AbstractBedBlock.useWithoutItem: in an occupied bed a villager
        // asleep there is woken (kickVillagerOutOfBed), else someone else
        // sleeps in it; the player stays up either way.
        let head_state = self.world.block(head[0], head[1], head[2]);
        let occupied = self
            .pack
            .assets
            .registry
            .state(head_state)
            .is_some_and(|s| s.properties.get("occupied").is_some_and(|v| v == "true"));
        if occupied {
            if !self.kick_villager_out_of_bed(head) {
                self.tell_sleeper("block.minecraft.bed.occupied", "This bed is occupied", true);
            }
            return;
        }
        // ServerPlayer.bedInRange / isReachableBedBlock: within 3 blocks
        // sideways and 2 vertically of the head or the foot.
        let reachable = |bed: [i32; 3]| {
            (position.x - f64::from(bed[0]) - 0.5).abs() <= 3.0
                && (position.y - f64::from(bed[1])).abs() <= 2.0
                && (position.z - f64::from(bed[2]) - 0.5).abs() <= 3.0
        };
        let mut problem: Option<&'static str> = if !reachable(head) && !reachable(foot) {
            Some("too_far_away")
        } else {
            None
        };
        // ServerPlayer.bedBlocked: Player.freeAt (not suffocating) for the
        // blocks above the head and the foot, as `in_wall` reads suffocating.
        let blocked = problem.is_none()
            && [head, foot].iter().any(|bed| {
                let f = self
                    .res
                    .facts()
                    .get(self.world.block(bed[0], bed[1] + 1, bed[2]));
                f.full_collision && f.solid_render
            });
        if blocked {
            problem = Some("obstructed");
        }
        // In reach (bedInRange), day or night: setRespawnPosition(
        // RespawnConfig(the head, the player's yaw), not forced). It replaces
        // an anchor or a spawnpoint; respawning checks the bed is still there
        // and finds where to stand up (findRespawnAndUseSpawnBlock).
        // Straw beds never set it (`straw_bed_rule`).
        if problem.is_none() && self.bed_sets_spawn(state) {
            let config = respawn_anchor::RespawnConfig::wrapped(self.dim, head, yaw, false);
            // ServerPlayer.setRespawnPosition: the chat message only when the
            // point moved (`isSamePosition`).
            let moved = self
                .respawn_config
                .is_none_or(|c| (c.dim, c.pos) != (config.dim, config.pos));
            // `canSetSpawn` calls the setter every time, the same bed too (it
            // refreshes the angle).
            let cancel = self.spawn_point_change(self.dim, Some(head), false);
            if !cancel {
                self.respawn_config = Some(config);
                if moved {
                    self.tell_sleeper("block.minecraft.set_spawn", "Respawn point set", false);
                }
                // The fallback becomes the world spawn: a guest's is none of its
                // own, so it follows `setworldspawn`.
                self.spawn = if self.lan_acting.is_some() {
                    None
                } else {
                    self.level
                        .world_spawn
                        .map(|[x, y, z]| {
                            Vec3::new(f64::from(x) + 0.5, f64::from(y), f64::from(z) + 0.5)
                        })
                        .or(self.spawn)
                };
            }
        }
        if problem.is_none() && !self.bed_can_sleep_now() {
            problem = Some("not_possible_now");
        }
        // Creative players skip the monster check (`!this.isCreative()`).
        if problem.is_none()
            && !self.creative
            && if self.ecs.is_active() {
                self.ecs.ids_near([position.x, position.y, position.z], 13.0).into_iter().any(|id| {
                    self.creature_species(id).is_some_and(|s| {
                        horizon_client::mobs::ai::is_monster(s)
                            && (s != horizon_client::mobs::Species::ZombifiedPiglin
                                || (self.ecs.is_angry(id) && self.ecs.ai_target(id).is_some()))
                    }) && self.ecs.alive(id)
                        && self.ecs.position(id).is_some_and(|p| {
                            (p[0] - position.x).abs() < 8.0
                                && (p[1] - position.y).abs() < 5.0
                                && (p[2] - position.z).abs() < 8.0
                        })
                })
            } else {
                self.remote_entities.iter().any(|m| {
                // `Monster.class` entities that `isPreventingPlayerRest`: a
                // zombified piglin only while it is angry.
                horizon_client::mobs::ai::is_monster(m.species())
                    && (m.species() != horizon_client::mobs::Species::ZombifiedPiglin
                        || (m.extra.anger > 0 && m.ai.target.is_some()))
                    && m.alive()
                    && (m.position.x - position.x).abs() < 8.0
                    && (m.position.y - position.y).abs() < 5.0
                    && (m.position.z - position.z).abs() < 8.0
                })
            }
        {
            problem = Some("not_safe");
        }
        // NeoForge's CanPlayerSleepEvent: a mod may clear or replace the
        // problem vanilla found, or add one.
        let mut problem = problem.map(str::to_owned);
        if let (Some(player), Some(name)) = (self.lan_acting_uuid, self.lan_acting_name.clone()) {
            self.native_can_player_sleep(player, &name, head, &mut problem);
        }
        if let Some(problem) = problem {
            match problem.as_str() {
                "too_far_away" => self.tell_sleeper(
                    "block.minecraft.bed.too_far_away",
                    "You may not rest now; the bed is too far away",
                    true,
                ),
                "obstructed" => self.tell_sleeper(
                    "block.minecraft.bed.obstructed",
                    "This bed is obstructed",
                    true,
                ),
                "not_safe" => self.tell_sleeper(
                    "block.minecraft.bed.not_safe",
                    "You may not rest now; there are monsters nearby",
                    true,
                ),
                "not_possible_now" => self.tell_sleeper(
                    "block.minecraft.bed.no_sleep",
                    "You can sleep only at night or during thunderstorms",
                    true,
                ),
                // `other_problem` and any name a mod invents: refused without a message.
                _ => {}
            }
            return;
        }
        // ServerPlayer.startSleepInBed: a percentage above 100 cannot skip the night.
        if self
            .ui
            .rules
            .int_at(horizon_client::gamerules::vanilla::PLAYERS_SLEEPING_PERCENTAGE)
            > 100
        {
            self.tell_sleeper(
                "sleep.not_possible",
                "No amount of rest can pass this night",
                true,
            );
        }
        if let (Some(player), Some(name)) = (self.lan_acting_uuid, self.lan_acting_name.clone()) {
            let mut cancel = false;
            self.native_player_bed_enter(player, &name, head, &mut cancel);
            if cancel {
                return;
            }
        }
        if self.lie_in_bed(head) {
            let bed = self.world.block(head[0], head[1], head[2]);
            self.award_custom_stat(self.slept_in_bed_stat(bed), 1);
            self.trigger_player("minecraft:slept_in_bed");
            self.stats.reset("custom", "minecraft:time_since_rest");
        }
    }
    /// `LivingEntity.startSleeping`: the player takes the bed whose head
    /// `head` names, moved onto it and asleep from now on. False when the
    /// bed gives no shape to lie on.
    fn lie_in_bed(&mut self, head: [i32; 3]) -> bool {
        // The shape the bed gives the player to lie on (`getSleepHeight`).
        // A straw bed's head answers with its foot's shape, and with no foot
        // of its own there is none, so the player stays up as
        // `startSleeping` returning false leaves them.
        let Some(at) = self.sleeping_position(head) else {
            return false;
        };
        // LivingEntity.startSleeping: stopRiding once the bed is valid.
        self.stop_riding();
        self.stop_mining();
        self.left = false;
        self.right = false;
        self.using = None;
        self.sleeping = Some(head);
        self.sleep_timer = 0;
        // LivingEntity.startSleeping and ServerPlayer.startSleeping: setPosToBed
        // and connection.teleport.
        if let Some(p) = self.player.as_mut() {
            p.position = at;

            p.velocity = Vec3::ZERO;
            let _ = p.set_sleeping(true);
            p.pitch = 0.0;
            p.teleported = true;
            p.teleported_delta = [false; 3];
        }
        self.pitch = 0.0;
        if self.lan_acting.is_none() {
            self.previous_position = at;
            self.snap_player_model_pitch();
        }
        self.occupy_bed(head);
        true
    }
    /// `Player.stopSleepInBed`: preserve the flags of the owning wake caller.
    fn wake_up(&mut self, wake_immediately: bool, update_level: bool) {
        // PlayerWakeUpEvent: heard at the start of stopSleepInBed, still in bed;
        // a listener may have woken the player, so the sleep is read after it.
        self.native_acting_bed_leave(wake_immediately, update_level);
        let Some(head) = self.sleeping.take() else {
            return;
        };
        self.sleep_timer = if wake_immediately { 0 } else { 100 };
        // The active guest's live body wakes here; the hub owns its wake
        // animation and sleeping metadata, not the suspended outer actor.
        if let Some(i) = self.lan_acting.and_then(|conn| self.guest_index(conn))
            && let Some(host) = self.lan.as_mut()
        {
            // The live turn may have changed its bed since entry.
            host.hub.guests.update(i, |guest| guest.bed = Some(head));
            host.hub.wake(i, wake_immediately);
            // This live body applies the stand-up pose below, not the later
            // wake_woken_guests pass over stored guest bodies.
            host.hub.guests.update(i, |guest| guest.woke_from = None);
        }
        let stand = self.bed_wake_pose(head, self.yaw);
        if let Some(p) = self.player.as_mut() {
            if let Some((at, yaw)) = stand {
                p.position = at;
                p.velocity = Vec3::ZERO;
                p.yaw = yaw;
                p.pitch = 0.0;
                p.teleported = true;
                p.teleported_delta = [false; 3];
            }
            let _ = p.set_sleeping(false);
        }
        if let Some((at, yaw)) = stand {
            self.yaw = yaw;
            self.pitch = 0.0;
            if self.lan_acting.is_none() {
                self.previous_position = at;
                self.snap_player_model_pitch();
            }
        }
    }
    fn tick_sleep(&mut self) {
        self.sleep_timer = tick_sleep_timer(self.sleep_timer, self.sleeping.is_some());
        if let Some(head) = self.sleeping {
            // LivingEntity.tick's checkBedExists: a sleeper whose bed is
            // gone gets up.
            let bed = self.world.block(head[0], head[1], head[2]);
            let bed_gone = !self.res.facts().get(bed).name.ends_with("_bed");
            let wake_immediately = bed_gone
                || self
                    .player
                    .as_ref()
                    .is_none_or(|p| p.health.dead_or_dying());
            if wake_immediately {
                self.wake_up(wake_immediately, true);
                return;
            }
            // setPosToBed: the bed's top (`getSleepHeight`) and 0.125 above
            // it. A sleeper whose bed no longer gives a shape to lie on stays
            // where it is: vanilla only ever places a sleeper when it starts.
            let at = self.sleeping_position(head);
            if let (Some(p), Some(at)) = (self.player.as_mut(), at) {
                p.position = at;
                p.velocity = Vec3::ZERO;
                p.pitch = 0.0;
            }
            // LivingEntity.tick: a sleeper's xRot is 0.
            self.pitch = 0.0;
            self.snap_player_model_pitch();
        }
        // ServerLevel.tick: once enough of the players have slept 100 ticks,
        // skip to morning if `advance_time` is on, wake everyone, and
        // resetWeatherCycle if `advance_weather` is on and it is raining.
        if self.enough_slept() {
            if self
                .ui
                .rules
                .bool_at(horizon_client::gamerules::vanilla::ADVANCE_TIME)
            {
                // SleepFinishedTimeEvent on the level's own clock (game/world/wake_time.rs): None
                // when a mod canceled, so `native_time_skip` is not asked, or no time fits.
                if let Some((clock, t, target)) = self.sleep_finished_move() {
                    let mut cancel = false;
                    self.native_time_skip(self.dim.id(), t, target, "sleep", &mut cancel);
                    if !cancel {
                        self.set_command_clock_time(&clock, target);
                    }
                }
            }
            // Level.isRaining: the rain level is above 0.2.
            if self
                .ui
                .rules
                .bool_at(horizon_client::gamerules::vanilla::ADVANCE_WEATHER)
                && self.weather_levels(1.0).0 > 0.2
            {
                self.world.weather.raining = false;
                self.world.weather.thundering = false;
                self.world.weather.rain_time = 0;
                self.world.weather.thunder_time = 0;
                self.ecs_weather_changed();
            }
            if self.sleeping.is_some() {
                self.wake_up(false, false);
            }
            self.wake_guests();
            // SleepStatus.removeAllSleepers: waking everyone is not announced.
            self.sleep_seen.1 = 0;
        }
    }
    /// Copy the live player and world state into the `Level` that gets saved.
    pub(super) fn sync_level(&mut self) {
        self.level.player_impulse_grace = self.player.as_ref().map(|p| p.impulse.reset_grace_time);
        self.level.player_fall_particles = self
            .player
            .as_ref()
            .map(|p| p.spawn_extra_particles_on_fall);
        // Level.write saves the game rules (game_rules.dat) with the rest.
        self.level.game_rules = self.ui.rules.by_name();
        // PrimaryLevelData.setDifficulty: /difficulty and the options button change it.
        self.level.difficulty =
            horizon_client::title::DIFFICULTIES[usize::from(self.ui.difficulty) & 3].into();
        self.level.time = self.world.game_time;
        self.level.weather = self.world.weather;
        // level.dat holds the Overworld clock's total, whichever level is in place.
        self.level.day_time = self.command_clock_time("minecraft:overworld");
        self.level.end_clock = Some(self.world.end_clock);
        // A server has no player of its own; `enter_guest` puts one in
        // `self.player` only while a command reads it.
        if self.player.is_none() && self.options.dedicated.is_some() {
            return;
        }
        self.level.selected_slot = self.selected as i32;
        let mode = self.mode_id();
        // The vehicle the player rides (`ServerPlayer.saveParentVehicle`).
        let others = self
            .lan
            .as_ref()
            .map(|h| self.ridden_by_others(&h.hub.guests, self.dim.id(), None, false))
            .unwrap_or_default();
        let vehicle = self.parent_vehicle_tag(self.riding, &others);
        // Only the local player's file is this one's to rewrite.
        let local = self.serving.is_none() && self.options.dedicated.is_none();
        let level = &mut self.level;
        level.creative = self.creative;
        level.game_mode = mode;
        level.previous_game_mode = self.previous_mode.map(mod_api::GameMode::id);
        level.spawn = self.spawn.map(|s| [s.x, s.y, s.z]);
        level.respawn_elsewhere = self.respawn_config.map(|c| c.saved());
        level.rotation = [self.yaw, self.pitch];
        level.xp_level = self.experience.level;
        level.xp_progress = self.experience.progress;
        level.xp_total = self.experience.total;
        level.score = self.experience.score;
        level.root_vehicle = vehicle.as_ref().map(|v| v.2).or(self.pending_vehicle);
        // The ride's tree, or none once the file's own came back.
        match vehicle {
            Some((tag, ..)) => {
                let extra = level
                    .player_extra
                    .get_or_insert_with(|| horizon_core::nbt::Tag::Compound(Default::default()));
                if let horizon_core::nbt::Tag::Compound(map) = extra {
                    map.insert("RootVehicle".into(), tag);
                }
            }
            None if local && self.pending_vehicle.is_none() => {
                if let Some(horizon_core::nbt::Tag::Compound(map)) = &mut level.player_extra {
                    map.remove(&"RootVehicle".into());
                }
            }
            None => {}
        }
        if let Some(p) = &self.player {
            level.position = Some([p.position.x, p.position.y, p.position.z]);
            level.health = p.health.health();
            level.absorption = p.health.absorption();
            let extra = level
                .player_extra
                .get_or_insert_with(|| horizon_core::nbt::Tag::Compound(Default::default()));
            if let horizon_core::nbt::Tag::Compound(map) = extra {
                map.insert(
                    "Fire".into(),
                    horizon_core::nbt::Tag::Short(
                        p.fire_ticks.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16,
                    ),
                );
                map.insert("Air".into(), horizon_core::nbt::Tag::Short(p.air as i16));
                // Entity.saveWithoutId: the `Invulnerable` flag always, its
                // `invulnerable_time` window only while open, like a mob's
                // save (arismoko/horizon#916).
                map.insert(
                    "Invulnerable".into(),
                    horizon_core::nbt::Tag::Byte(i8::from(p.health.permanently_invulnerable)),
                );
                if p.health.invulnerable_time > 0 {
                    map.insert(
                        "invulnerable_time".into(),
                        horizon_core::nbt::Tag::Int(p.health.invulnerable_time),
                    );
                } else {
                    map.remove(&"invulnerable_time".into());
                }
                // LivingEntity.addAdditionalSaveData: AttributeMap.pack.
                map.insert("attributes".into(), p.saved_attributes());
            }
            level.fall_distance = p.fall_distance;
            level.food = p.health.food.food;
            level.saturation = p.health.food.saturation;
            level.flying = p.flying;
        }
        level.inventory = self
            .inventory
            .iter()
            .enumerate()
            .filter_map(|(i, s)| {
                s.as_ref()
                    .map(|s| (i as u8, s.item.to_string(), s.count, s.damage))
            })
            .collect();
        level.inventory_components = self
            .inventory
            .iter()
            .enumerate()
            .filter_map(|(i, s)| Some((i as u8, s.as_ref()?.extra.clone()?)))
            .collect();
        if let Some(s) = &self.offhand {
            level
                .inventory
                .push((150, s.item.to_string(), s.count, s.damage));
            if let Some(extra) = s.extra.clone() {
                level.inventory_components.insert(150, extra);
            }
        }
        for (equipment_slot, s) in self.equipment.iter().enumerate() {
            if let (Some(s), Some(slot)) = (s, horizon_client::save::saved_slot(equipment_slot)) {
                level
                    .inventory
                    .push((slot, s.item.to_string(), s.count, s.damage));
                if let Some(extra) = s.extra.clone() {
                    level.inventory_components.insert(slot, extra);
                }
            }
        }
        // Vanilla stores armor in slots 100 (feet) to 103 (head).
        for (i, s) in self.armor.iter().enumerate() {
            if let Some(s) = s {
                level
                    .inventory
                    .push((103 - i as u8, s.item.to_string(), s.count, s.damage));
                if let Some(extra) = s.extra.clone() {
                    level.inventory_components.insert(103 - i as u8, extra);
                }
            }
        }
        level.dimension = self.dim.id().to_owned();
        // An open ender chest holds the live copy.
        if let Some(key) = self.ender_open
            && let Some(BlockEntity::Chest(items)) = self.world.block_entities.get(&key)
        {
            self.ender_items = items.clone();
        }
        level.ender_items = self.ender_items.clone();
        level.active_effects = self.potions.effects.saved();
    }
    /// The saved player's hands, armor, body, saddle and ender chest go in
    /// their fields; the 36 inventory slots come back.
    fn load_saved_inventory(&mut self, level: &Level) -> Vec<Option<ItemStack>> {
        let mut inventory = vec![None; 36];
        self.armor = [None, None, None, None];
        self.offhand = None;
        self.equipment = Default::default();
        self.ender_items = if level.ender_items.len() == 27 {
            level.ender_items.clone()
        } else {
            vec![None; 27]
        };
        for (slot, id, count, damage) in &level.inventory {
            let stack = ItemStack {
                item: Arc::from(id.as_str()),
                count: *count,
                damage: *damage,
                extra: level.inventory_components.get(slot).cloned(),
            };
            if (100..104).contains(slot) {
                self.armor[usize::from(103 - slot)] = Some(stack);
            } else if *slot == 150 {
                self.offhand = Some(stack);
            } else if let Some(equipment_slot) = horizon_client::save::equipment_slot(*slot) {
                self.equipment.set(equipment_slot, Some(stack));
            } else if let Some(cell) = inventory.get_mut(usize::from(*slot)) {
                *cell = Some(stack);
            }
        }
        inventory
    }
    /// `ChunkMap.saveChunksEagerly`: snapshot only while the tick and writer
    /// have room. NBT encoding, compression and durable writes stay on workers.
    fn save_chunks_eagerly(&mut self) {
        // No world open (`world_root` is set by `finish_open_world`): the save
        // is `Game::new`'s placeholder under the working directory.
        if self.world.carrier_fences.is_broken()

            || self.world_root.as_os_str().is_empty()
            || self.agent.hold_server
            || self.clock.sprinting()
            || self
                .server_lists
                .as_ref()
                .is_some_and(|lists| !lists.lock().unwrap().autosave)
        {
            return;
        }
        let deadline = self
            .server_tick_deadline
            .unwrap_or_else(|| Instant::now() + self.clock.tick_duration());
        let mut modified = std::mem::take(&mut self.world.modified);
        modified.save_eagerly(
            Instant::now(),
            || Instant::now() < deadline && self.save.active_chunk_writes() < 128,
            |key| self.world.chunks.contains_key(&key),
            |key| {
                // `saveChunkIfNeeded` saves only a LevelChunk, so a lit one; a
                // column still waiting for its light keeps its queue entry.
                let Some(chunk) = self.world.chunks.get(&key).filter(|c| c.lit) else {
                    return false;
                };
                let chunks = self.with_entities([chunk]);
                match self
                    .save
                    .save_later(&chunks, self.pack.assets.registry, world_biomes!(self))
                {
                    Ok(_) => true,
                    Err(e) => {
                        log::warn!("Horizon: failed to save changed chunks: {e}");
                        false
                    }
                }
            },
        );
        self.world.modified = modified;
    }
    fn render_frame(&mut self) -> Result<bool> {
        self.render_with(|renderer, frame| {
            horizon_client::spans::time("renderer", || renderer.render(frame))
        })
    }

}
/// `computeNextAutosaveInterval`'s arithmetic: `Math.max(100, (int)(rate * 300.0F))`.
fn autosave_interval(rate: f32) -> i64 {
    i64::from((rate * 300.0) as i32).max(100)
}
/// winit's events for a window's game, with any loop: a headless client (#4951) has no
/// winit loop and calls these from its own.
impl Game<'_> {
}
/// clouds.png as filled cells (`CloudRenderer.prepare`: alpha 10 or more).
fn load_clouds(root: &horizon_assets::resource_source::ResourceSource) -> (u32, u32, Vec<bool>) {
    root.read("assets/minecraft/textures/environment/clouds.png")
        .map_err(Into::into)
        .and_then(|bytes| horizon_client::hud::decode_bytes(&bytes))
        .map(|(w, h, px)| {
            (
                w,
                h,
                px.as_chunks::<4>().0.iter().map(|p| p[3] >= 10).collect(),
            )
        })
        .unwrap_or((1, 1, vec![false]))
}
/// misc/forcefield.png for the world border wall (square, power of two).
fn load_forcefield(root: &horizon_assets::resource_source::ResourceSource) -> (u32, u32, Vec<u8>) {
    root.read("assets/minecraft/textures/misc/forcefield.png")
        .map_err(Into::into)
        .and_then(|bytes| horizon_client::hud::decode_bytes(&bytes))
        .ok()
        .filter(|(w, h, _)| w == h && w.is_power_of_two())
        .unwrap_or((1, 1, vec![0; 4]))
}
impl<'a> Game<'a> {
    /// A game with no world open, over the loaded resources: the window's,
    /// or an integrated server's on its own thread.
    fn new(
        res: &'a Arc<Resources>,
        pack: PackResources,
        gui: UiResources,
        options: Options,
        settings: Settings,
        audio: Audio,
        prepared: Option<GameCpuResources>,
    ) -> Result<Self> {
        let phase = match Self::mod_phase_for_construction() {
            mod_api::__private::ModPhase::Packages if cfg!(test) => {
                mod_api::__private::ModPhase::World
            }
            mod_api::__private::ModPhase::Packages => mod_api::__private::ModPhase::Setup,
            phase => phase,
        };
        let mod_phase = mod_api::__private::PhaseOwner::new(phase);
        // A scoped hook on this thread asks this game's parts.
        mod_api::__private::mark_game_thread();
        let standalone_geometry = native_block_entity_draw::StandaloneGeometry::new(&pack, &gui);
        let loop_side = options.execution_role.loop_side();
        let root = options.resources.clone();
        let mod_sources = options.mods.clone();
        let registry = pack.assets.registry;
        // Until a world is opened the streamer is idle.
        let mut streaming = Streaming::new(
            Arc::new(TestTerrain::new(registry, res.biomes())?),
            pack.context.clone(),
            1,
        );
        streaming.paused = true;
        let level = Level::default();
        let save = Arc::new(SaveDir::new(
            std::env::current_dir()?
                .join("saves")
                .join(".horizon-title"),
            horizon_client::save::LevelShape::OVERWORLD,
        ));
        let prepared = prepared.map(Ok).unwrap_or_else(|| {
            GameCpuResources::load(
                &horizon_assets::resource_source::ResourceRoot::new(
                    root.clone(),
                    packs::cache_dir(),
                    pack.source.clone(),
                ),
                None,
            )
        })?;
        let world = Game::world_for_mods(res.facts().air, 0);
        let tutorial = tutorial::Tutorial::new(tutorial::Step::from_option(
            settings.file.int("tutorialStep"),
        ));
        let creative = options.creative;
        let inventory = vec![None; 36];
        let now = Instant::now();
        let look = options.look;
        let slow_frame_ms = options
            .spikes
            .unwrap_or(horizon_client::client_perf::SLOW_FRAME_MS);
        let particles = horizon_client::particles::ParticleEngine::new(
            &pack.source,
            &gui.atlas.sprites,
            &gui.atlas.translucent,
        );
        let resource_source = pack.source.clone();
        let mut game = Game {
            structure_spawns: horizon_client::mobs::spawn_selection::StructureSpawns::load(
                &options.resources,
            ),
            world_biomes: None,
            world_biomes_base: None,
            res: res.clone(),
            rest_loader: None,
            pending_entry: None,
            pending_pack_reload: false,
            pack_folder: None,
            pack_reload: None,
            server_packs: Default::default(),
            link_prompt: None,
            command_prompt: None,
            code_of_conduct: None,
            integrated_starting: None,
            collision: res.collision(),
            motion: res.motion(),
            health_resources: res.motion().health.clone(),
            damage_tag_generation: 1,
            data_map_registries: Vec::new(),
            pack,
            gui,
            instance_groups: Default::default(),
            standalone_geometry,
            fog: Default::default(),
            item_activation: Default::default(),
            weather_parameters: Default::default(),
            world,
            streaming,
            lod: None,
            lod_source_attempted: false,
            far_beacons: HashMap::new(),
            lod_observe: Default::default(),
            lod_level: None,
            lod_covered_at: None,
            view_arrival: Default::default(),
            options,
            player: None,
            spawn: None,
            clock: FixedStep::default(),
            previous_position: Vec3::ZERO,
            rope_rot_old: (0.0, 10.0),
            eye: 1.62,
            previous_eye: 1.62,
            yaw: look.map_or(level.rotation[0], |l| l.0),
            pitch: look.map_or(level.rotation[1], |l| l.1),
            inventory,
            selected: 0,
            carried: None,
            craft_grid: vec![None; 9],
            craft_size: 2,
            craft_table: None,
            tweaks: Default::default(),
            last_quick_moved: None,
            container: None,
            open_container_slots: None,
            station: None,
            potions: Default::default(),
            scratch_slot: None,
            mining: None,
            destroying: HashMap::new(),
            destroy_delay: 0,
            use_delay: 0,
            creative,
            mode: if creative {
                horizon_client::command::args::GameMode::Creative
            } else {
                horizon_client::command::args::GameMode::Survival
            },
            keys: Default::default(),
            physical_keys: HashSet::new(),
            physical_buttons: HashSet::new(),
            left: false,
            right: false,
            cursor: (0.0, 0.0),
            captured: false,
            capture_mode: None,
            regrab_on_refocus: false,
            view_at: [0, 0],
            split_auth: None,
            focused: true,
            unclaimed_modifiers: None,
            focus_after_run: None,
            focus_lost: None,
            modifiers: winit::keyboard::ModifiersState::empty(),
            fullscreen: horizon_client::window::FullscreenState::new(false),
            video_modes: Default::default(),
            mouse: MouseLook::default(),
            agent_pointer: false,
            was_playing: false,
            pause: false,
            debug: false,
            gpu_passes: false,
            gpu_pass_detail: false,
            screen: Screen::Playing,
            window: None,
            resize_state: None,
            renderer: None,
            last_device_recovery: None,
            entity_vertices: Vec::new(),
            dynamic_lights: Vec::new(),
            see_through_vertices: Vec::new(),
            shadow_only_vertices: Default::default(),
            entity_motion: Default::default(),
            entity_motion_posed: Default::default(),
            skin_models: Default::default(),
            skin_frame: Default::default(),
            glowing_vertices: Default::default(),
            pack_ids: None,
            started: now,
            previous: now,
            last_input: now,
            pad: Default::default(),
            frames: 0,
            kept_block_entities: Default::default(),
            live_scenes: Default::default(),
            text_patches: Default::default(),
            mod_view_distances: Default::default(),
            item_level_cache: Default::default(),
            world_frames: 0,
            ticks: 0,
            error: None,
            message: None,
            save,
            level,
            player_autosaves: Default::default(),
            session_lock: None,
            world_adopted: false,
            audio,
            loop_context: Arc::new(res.core.item_data.context().loop_context(loop_side, [])),
            setting_overrides: Default::default(),
            damage_hooks: native_hooks::DAMAGE_HOOKS,
            music: {
                // The manager starts with the saved option (MusicManager's constructor).
                let mut music = horizon_client::music::MusicManager::default();
                music.frequency = horizon_client::music::MusicFrequency::parse(
                    &settings.file.name("musicFrequency"),
                );
                music
            },
            // ToastManager.initializeMusicToast: Pause and Pause and Toast make the toast.
            ambience: Default::default(),
            cart_sounds: HashSet::new(),
            riding_cart_sounds: None,
            was_fall_flying: false,
            guardian_look_scratch: Vec::new(),
            dropped: Default::default(),
            next_entity: 0,
            ecs_effect_particles: HashMap::new(),
            ticked_entities_through: u32::MAX,
            contact_drops: 0..0,
            particles,
            pickups: Default::default(),
            sign_edit: None,
            book: None,
            lectern_pulses: Vec::new(),
            loading_progress: 0.0,
            creation_job: None,
            loading_progress_at: None,
            moving_pistons_stale: false,
            spawn_origin: None,
            spawn_hook_fired: false,
            spawn_hook_cancelled: false,
            spawn_spiral: Default::default(),
            player_spawn: None,
            spawn_wait: Vec::new(),
            ecs: horizon_client::ecs_bridge::EcsLevel::inert(),
            remote_entities: Default::default(),
            remote_index: Default::default(),
            creatures: Default::default(),
            sculk: Default::default(),
            raiding: Default::default(),
            attack_ticks: 100,
            attack_item: None,
            last_hurt_by_player: HashMap::new(),
            lead_killers: HashMap::new(),
            love_cause: HashMap::new(),
            died: Default::default(),
            be_anims: {
                let mut anims = horizon_client::block_entity_anim::BlockEntityAnims::default();
                anims.classify(registry);
                anims
            },
            using: None,
            use_duration: 0,
            use_start: Default::default(),
            crossbow_sounds: [false; 2],
            view: Default::default(),
            scope_scale: 0.5,
            title: None,
            native_world_request: None,
            armor: [None, None, None, None],
            offhand: None,
            equipment: Default::default(),
            hand: Hand::Main,
            shield_cooldown: 0,
            cooldowns: Default::default(),
            command_source: None,
            native_command_filter: false,
            titles: Default::default(),
            autosaving: false,
            // `computeNextAutosaveInterval` at the clock's 20 ticks a second.
            ticks_until_autosave: 6000,
            applied_modifiers: Vec::new(),
            experience: Experience::default(),
            orbs: Default::default(),
            xp_delay: 0,
            ordered: Default::default(),
            level_up_sound: Default::default(),
            swing: Default::default(),
            swings_started: 0,
            predicted_swings: 0,
            heights: Default::default(),
            sway: Default::default(),
            visible_hands: [(0, None), (0, None)],
            hands_replaced: [false; 2],
            clicks: Default::default(),
            hand_strength: Default::default(),
            settings,
            dragging: false,
            avatar: Default::default(),
            flash: 0,
            custom_sky: Default::default(),
            lightning_seen: [0; 2],
            sleeping: None,
            sleep_timer: 0,
            player_beds: Vec::new(),
            sleep_seen: (0, 0),
            dim: Dim::Overworld,
            world_root: PathBuf::new(),
            carrier_retention_snapshot: Vec::new(),
            owner_migration: None,
            world_flat: false,
            world_debug: false,
            server_trips: Vec::new(),
            level_teleports: Vec::new(),
            teleport_notes: Vec::new(),
            levels: Default::default(),
            level_pass: Default::default(),
            portals: Default::default(),
            respawn_config: None,
            end_portal_travel: false,
            win: None,
            end: Default::default(),
            block_ticks: Ticks::new(0),
            tick_scratch: TickScratch::default(),
            tick_chunks: Default::default(),
            tickets: Default::default(),
            random_tick_sections: Default::default(),
            level_events: Vec::new(),
            falling_vertices: Default::default(),
            falling: Default::default(),
            transient_blocks: Default::default(),
            paintings: Default::default(),
            painting_variants: prepared.painting_variants,
            item_frames: Default::default(),
            frame_model: prepared.frame_model,
            glow_frame_model: prepared.glow_frame_model,
            map_frame_model: prepared.map_frame_model,
            glow_map_frame_model: prepared.glow_map_frame_model,
            armor_stands: Default::default(),
            location_armor: Default::default(),
            cushions: Default::default(),
            others: Default::default(),
            armor_stand_assets: prepared.armor_stand_assets,
            waypoint_styles: prepared.waypoint_styles,
            waypoints: Default::default(),
            posed_rotation: None,
            posed_roll: 0.0,
            post_effects: Vec::new(),
            polytone_post_ticks: 0.0,
            spectated_post_effect: None,
            experience_display: Default::default(),
            boats: Default::default(),
            leash_boats: Vec::new(),
            boat_models: prepared.boat_models,
            crystal_model: prepared.crystal_model,
            cushion_model: prepared.cushion_model,
            entity_models: prepared.entity_models,
            carts: Default::default(),
            watched_vehicles: Vec::new(),
            jukebox_sounds: HashMap::new(),
            bolts: Vec::new(),
            maps: maps::Maps::default(),
            fight: dragon::Fight::default(),
            dragon: Default::default(),
            crystals: Default::default(),
            breath_clouds: Default::default(),
            dragon_model: prepared.dragon_model,
            map_patches: Vec::new(),
            name_tag_layouts: Default::default(),
            text_displays: Default::default(),
            interactions: Default::default(),
            remote_knots: Default::default(),
            knot_ids: Default::default(),
            text_display_vertices: 0,
            skin_downloads: RefCell::new({
                let mut downloads = horizon_client::skins::Downloader::new(
                    std::env::current_dir()
                        .unwrap_or_default()
                        .join("assets/skins"),
                );
                downloads.set_resource_root(resource_source.clone());
                downloads
            }),
            skin_profiles: RefCell::new(horizon_client::skins::Resolver::new()),
            skin_slots: horizon_client::skins::SkinSlots::new(horizon_client::hud::SKIN_SLOTS),
            cosmetics: Default::default(),
            cart_models: prepared.cart_models,
            riding: None,
            started_riding: Vec::new(),
            rotation_forced: false,
            posed_rider: None,
            pending_vehicle: None,
            ender_items: vec![None; 27],
            ender_open: None,
            pending: Default::default(),
            spill: Default::default(),
            recipe_ui: Default::default(),
            mount_menu: Default::default(),
            hook: Default::default(),
            remote_hooks: Vec::new(),
            tridents: Default::default(),
            rockets: Default::default(),
            ominous_spawners: Default::default(),
            creaking: Default::default(),
            spawners: Default::default(),
            spawn_scratch: Default::default(),
            trident_model: prepared.trident_model,
            arrow_model: prepared.arrow_model,
            bee_stinger_model: prepared.bee_stinger_model,
            tnts: Default::default(),
            hoppers: HashMap::new(),
            transactions: Default::default(),
            block_entity_tick_key: None,
            block_entity_tick_items: None,
            wasm_dispatch_failures:
                horizon_mod_runtime::dispatch_failures::dispatch_failure_counter(),
            contents_changing: Vec::new(),
            mod_container_viewers: HashMap::new(),
            item_columns: None,
            pickup_columns: None,
            pass_mobs: None,
            spawner_tick_positions: Vec::new(),
            trial_spawner_tick_positions: Vec::new(),
            loop_states: Default::default(),
            creative_screen: prepared.creative_screen,
            creative_ignore_text: false,
            control: None,
            agent_control_on: false,
            agent_last: None,
            alloc_mark: None,
            perf: Default::default(),
            pending_bench: None,
            flight: None,
            events: Default::default(),
            profile: profile_capture::Profile::new(slow_frame_ms),
            tick_script: tick_script::TickScript::from_env(),
            agent: agent::Agent::default(),
            occlusion: Default::default(),
            pending_uploads: Vec::new(),
            extras: Default::default(),
            pvp: Default::default(),
            mods: mod_sources,
            mod_jobs: Default::default(),
            recipe_tables: Default::default(),
            panel: Default::default(),
            native_shaders: Default::default(),
            native_input: Default::default(),
            native_lifecycle: Default::default(),
            resident_entities: Default::default(),
            lent_mobs: Default::default(),
            held_out: Vec::with_capacity(4),
            native_configs: Default::default(),
            mod_phase,
            native_net: Default::default(),
            sky_darken_memo: Default::default(),
            water_plane_memo: Default::default(),
            water_plane_scan: Default::default(),
            vehicle_mobs: Default::default(),
            persist: Default::default(),
            missing_professions: Default::default(),
            entity_attachments: Default::default(),
            entity_initialization: Default::default(),
            attachment_indexes: Default::default(),
            remote: None,
            realm_session: false,
            replay: Default::default(),
            lan: None,
            integrated: None,
            level_shapes: Default::default(),
            serving: None,
            world_options: None,
            options_to_world_options: false,
            command_block_screen: None,
            structure_templates: HashMap::new(),
            test_defs: std::cell::RefCell::new(HashMap::new()),
            test_defs_snapshot: false,
            test_highlights: HashMap::new(),
            game_tests: game_tests::GameTests::default(),
            #[cfg(feature = "friends")]
            debug_charts: Default::default(),
            gpu_utilization: 0.0,
            spawn_counts: HashMap::new(),
            spawn_policies: HashMap::new(),
            passages: Default::default(),
            death_focus: None,
            previous_mode: None,
            gamemode_switcher: None,
            user_input: Default::default(),
            saved_for_exit: false,
            client_quit: std::cell::Cell::new(ClientQuit::Running),
            pending_save: None,
            server_tick_deadline: None,
            world_icon: None,
            forced_hit: None,
            tick_hit: Default::default(),
            tick_mob: None,
            entity_reach: 3.0,
            block_reach: 4.5,
            client_targeting: Default::default(),
            guest_use: None,
            use_failed: false,
            block_use_original: Vec::new(),
            lan_events: Vec::new(),
            lan_songs: Vec::new(),
            lan_cushion_breaks: Vec::new(),
            lan_block_events: Vec::new(),
            lan_entity_events: Vec::new(),
            lan_critical_hits: Vec::new(),
            lan_kinetic_hits: Vec::new(),
            stabbed: HashMap::new(),
            stab_players: Vec::new(),
            player_stabs: Vec::new(),
            known_movement: Vec3::ZERO,
            kinetic_feedback: HashMap::new(),
            lan_global_events: Vec::new(),
            lid_openers: HashMap::new(),
            lan_acting: None,
            acting_abilities: None,
            native_commands: Default::default(),
            lan_building_boxes: Default::default(),
            save_after_guest_turn: false,
            lan_guest_swaps: Vec::new(),
            lan_interrupted_turns: Vec::new(),
            keep_owner_files: false,
            player_state: Default::default(),
            lan_break_pos: None,
            break_collect: None,
            guest_packets: Vec::new(),
            guest_cooldowns: Vec::new(),
            guest_entity_events: Vec::new(),
            guest_equipment_breaks: Vec::new(),
            guest_block_sounds: Vec::new(),
            guest_player_sounds: Vec::new(),
            bound_sounds: Vec::new(),
            lan_acting_name: None,
            lan_acting_uuid: None,
            tnt_refused: None,
            lan_acting_permission: None,
            lan_acting_spawn_protection: None,
            lan_acting_entity: None,
            lan_acting_fake: false,
            fake_action: Default::default(),
            chat_capture: None,
            admin_feedback: Vec::new(),
            console_broadcast: None,
            rcon_source: false,
            server_lists: None,
            world_tables: None,
            server_loot: Default::default(),
            server_items: None,
            cmds: Default::default(),
            reload_resources: Default::default(),
            ui: chat::UiState::new(),
            combat: Default::default(),
            guest_combat: Default::default(),
            mount: Default::default(),
            stats: Default::default(),
            score: Default::default(),
            fns: Default::default(),
            bosses: Default::default(),
            boss_overlay: Default::default(),
            exec: Default::default(),
            rand_sequences: Default::default(),
            brushing: Default::default(),
            border: Default::default(),
            border_section: None,
            border_state: Default::default(),
            tab_list: Default::default(),
            adv: Default::default(),
            restore_player_state: true,
            links_ui: Default::default(),
            camera_type: Default::default(),
            fangs: Default::default(),
            player_model: None,
        };
        game.reset_entity_residency();
        Ok(game)
    }
}

/// `key.hotbar.1` to `key.hotbar.9` in a menu.
const HOTBAR_DIGITS: [KeyCode; 9] = [
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit4,
    KeyCode::Digit5,
    KeyCode::Digit6,
    KeyCode::Digit7,
    KeyCode::Digit8,
    KeyCode::Digit9,
];
/// The other keys `InputConstants.Key.getNumericKeyValue` gives a digit.
const NUMERIC_KEYS: [KeyCode; 11] = [
    KeyCode::Digit0,
    KeyCode::Numpad0,
    KeyCode::Numpad1,
    KeyCode::Numpad2,
    KeyCode::Numpad3,
    KeyCode::Numpad4,
    KeyCode::Numpad5,
    KeyCode::Numpad6,
    KeyCode::Numpad7,
    KeyCode::Numpad8,
    KeyCode::Numpad9,
];

/// A title-shaped frame: black clear and panorama, only the HUD is drawn.
/// The window's `HWND`, for the boot screen's GDI paint (`horizon_threads`):
/// the raw handle read is safe, the paint is not.
#[cfg(target_os = "windows")]
fn hwnd(window: &Window) -> Option<isize> {
    use winit::raw_window_handle::{HasWindowHandle as _, RawWindowHandle};
    match window.window_handle() {
        Ok(raw) => match raw.as_raw() {
            RawWindowHandle::Win32(handle) => Some(handle.hwnd.get()),
            _ => None,
        },
        Err(_) => None,
    }
}

pub use launch_api::{UsageError, exit_code};

pub(crate) fn run_with(
    options: Options,
    native: horizon_mod_package::runtime::Runtime,
) -> Result<()> {
    let mut options = Options::read(options)?;
    options.mods.native = Arc::new(native);
    if options.execution_role == execution_role::ExecutionRole::DedicatedServer
        && !dedicated::preflight(&mut options)?
    {
        return Ok(());
    }
    native_hooks::install_block_contact_hooks();
    // Before any worker starts: on Linux they go behind the server thread
    // and the render thread.
    horizon_threads::put_helpers_behind();
    // A dedicated server too: vanilla's "Server Shutdown Thread" (`DedicatedServer.initServer`'s
    // shutdown hook) halts the server and saves its players and levels on SIGTERM, SIGINT or SIGHUP.
    if false
        || options.execution_role == execution_role::ExecutionRole::DedicatedServer
    {
        horizon_threads::catch_stop_signals();
    }
    let started = Instant::now();
    // A dedicated server times its spans always, so its slow ring holds the steps of a lag spike (microseconds a tick).
    if options.spikes.is_none()
        && options.execution_role == execution_role::ExecutionRole::DedicatedServer
    {
        options.spikes = Some(dedicated::SLOW_TICK_MS);
    }
    tick_profile::want_spans(tick_profile::SPANS_SPIKES, options.spikes.is_some());
    // A player who allows performance statistics also says what their slow frames spent their
    // time in: this thread, which draws the frames, times its steps (wall time only).
    if false
        && horizon_telemetry::Consent::load(&horizon_client::logging::game_home())
            .allows(horizon_telemetry::Kind::Perf)
    {
        horizon_client::spans::record_here(true);
    }
    let vanilla_root = options.resources.clone();
    // Saves from before 1.13 upgrade with vanilla's tables, prepared from the player's jar.
    horizon_world::datafix::set_resources(
        &std::path::absolute(&vanilla_root).unwrap_or_else(|_| vanilla_root.clone()),
    );
    if let Some(error) = missing_resources(&vanilla_root, horizon_hooks::engine::get_the_game()) {
        // The game build started by hand (the release's zip), not by the
        // launcher: a window says where to get it. Scripts and servers get
        // the error.
        if !options.offscreen && !synchronous_startup(&options) {
            log::error!("Horizon: {error}");
            let settings = Settings::load(&Game::options_path());
            return no_resources::show(&vanilla_root, &settings);
        }
        return Err(error.into());
    }
    // A window's start takes the last one's graphics device loss into its settings
    // (`gfx::safe_start`); held in memory too, for a hold its settings file didn't keep.
    let device_loss_hold = Game::options_path()
        .parent()
        .filter(|_| owns_device_loss_marker(&options))
        .and_then(horizon_client::gfx::safe_start::take);
    let mut early_settings = Settings::load(&Game::options_path());
    if device_loss_hold.is_some() {
        early_settings.horizon.safe_start = device_loss_hold;
    }
    options
        .mods
        .capture_code_switches(&early_settings.mods.disabled);
    options
        .mods
        .select(&std::env::current_dir()?, &early_settings.mods.enabled);
    let mod_phase = mod_api::__private::PhaseOwner::new(mod_api::__private::ModPhase::Packages);
    api_values::install_packages(options.mods.native.packages(), &options.mods.selected)?;
    mods::install_registration_data(&vanilla_root)?;
    {
        use mod_api::__private::game::RegistrationContextNew;
        let context = mod_api::RegistrationContext::new();
        let inventory: Vec<_> = context.mods().iter().cloned().collect();
        crate::entry::initialize_code(&inventory);
    }
    horizon_engine_slots::seal().expect("engine registration seals once at startup");
    arm_points!();
    mod_phase.advance(mod_api::__private::ModPhase::Registration)?;
    crate::game::part_admission::admit_parts(mod_api::__private::mods())?;
    // The structure types those mods registered, handed to the engine before
    // any world opens, as its StructureRegistry reads them.
    if horizon_world::worldgen::structure::mod_types::install_mod_structure_types(
        native_structure_types::installed_structure_types(),
    )
    .is_err()
    {
        log::warn!("Mod structure types already installed; keeping the first table");
    }
    // The levels they registered, below the data packs of every world opened.
    if horizon_world::worldgen::overrides::install_mod_dimensions(
        native_dimensions::installed_dimensions(),
    )
    .is_err()
    {
        log::warn!("Mod dimensions already installed; keeping the first table");
    }
    // Their chunk generators and biome sources, which those levels' and the packs' stems name,
    // and density function types, which the packs' density JSON names.
    native_chunk_generator::install();
    native_biome_source::install();
    native_density_function::install();
    mods::register_components(&vanilla_root)?;
    mods::register_standalone_models(options.execution_role);
    mods::register_layer_definitions(options.execution_role, &vanilla_root);
    mods::register_debug_entries(options.execution_role);
    mods::register_gui_layers(options.execution_role);
    mods::register_map_decoration_types();
    mods::register_range_select_item_model_properties(options.execution_role);
    mods::register_select_item_model_properties(options.execution_role);
    mods::register_resource_sources(options.execution_role);
    mods::register_enchantment_damage_effects();
    mods::register_client_presentations(options.execution_role);
    mods::register_meshing(options.execution_role);
    mods::register_particles(options.execution_role);
    mods::freeze_declarations()?;
    mod_phase.advance(mod_api::__private::ModPhase::Setup)?;
    // Minecraft.<init>: read the last start's flag, then mark this one as not
    // finished until onGameLoadFinished. A dedicated server has no options.txt.
    // Minecraft's constructor opens the narrator; a server has none.
    // Someone is playing: the anonymous ping behind the launcher's "N playing now", and the
    // official server in their list (servers.dat in the game folder, the working directory). Not
    // for a server, a benchmark, a run that quits by itself, a harness's fixture or a run that
    // horizon-ctl drives: those start in a source checkout and wrote the list there.
    if !options.offscreen && !synchronous_startup(&options) {
        let version = horizon_client::telemetry::version();
        let game_dir = horizon_client::logging::player_dir();
        horizon_hooks::engine::started(&horizon_hooks::Started::new(&version, &game_dir));
    }
    // A game window's hooks (Discord's presence): never for a server.
    activity::client_started(&options);
    // Snapshots of derived tables live in the game folder, never beside the
    // shared resources.
    Behaviours::set_cache_dir(std::env::current_dir()?.join(".horizon-cache"));
    if !synchronous_startup(&options) {
        return run_loading(options, early_settings, vanilla_root, None, started);
    }
    let mut pack_error = None;
    let selected_root = packs::startup_root(
        &vanilla_root,
        &early_settings.resource_packs,
        &[],
        &options.mods,
    )
    .or_else(|e| {
        log::warn!("Horizon: resource packs not applied: {e}");
        pack_error = Some(e.to_string());
        mods::content_root(&vanilla_root, &options.mods)
    })?;
    let mut loaded = load_resources(
        selected_root,
        pack_finders::data_packs(&options.mods),
        launch_world(&options),
        &vanilla_root,
        &early_settings,
        started,
        false,
        mods::LoadRole::startup(options.execution_role),
        &Progress::new(),
        &mut |_, _| {},
    )?;
    let gui = loaded.gui.ok_or("missing menu bundle")?;
    options.resources = loaded.root.clone();
    let audio = if true || options.headless {
        Audio::silent(&loaded.pack.source)
    } else {
        Audio::with_device(&loaded.pack.source, &early_settings.sound_device)
    };
    let resources = loaded.resources;
    let game = start_game(
        &resources,
        loaded.pack,
        gui,
        options,
        loaded.pack_error.or(pack_error),
        audio,
        vanilla_root,
        started,
        loaded.rest_loader,
        None,
        None,
        std::mem::take(&mut loaded.client_applies),
        loaded.entity_models,
    )?;
    if let Some(mut game) = game {
        // Minecraft.onGameLoadFinished: a start that made it this far is clean.
        run_event_loop(game)?;
    }
    Ok(())
}

/// The right and left arms of a player model layer (`PlayerModel.rightArm`
/// and `leftArm`, with their sleeves).
fn player_arms(
    root: &std::path::Path,
    key: &str,
    document: Option<&horizon_client::mobs::EntityModelDocument>,
) -> [Option<horizon_client::mobs::Part>; 2] {
    let model = match document {
        Some(document) => document.model(key),
        None => horizon_client::mobs::load_model(root, key),
    };
    ["right_arm", "left_arm"].map(|name| {
        model
            .as_ref()
            .and_then(|m| m.children.iter().find(|c| c.name == name).cloned())
    })
}

/// The most `Player.sleepCounter` reaches: 100 asleep, then up to here over the
/// waking fade.
const MAX_SLEEP_TIMER: u32 = 109;

// Player.tick: cap the sleeping fade at 100, then fade out over 10 waking ticks.
fn tick_sleep_timer(timer: u32, sleeping: bool) -> u32 {
    if sleeping {
        (timer + 1).min(100)
    } else if (1..MAX_SLEEP_TIMER).contains(&timer) {
        timer + 1
    } else {
        0
    }
}

/// Where a ray entering `pos` first meets the block's outline boxes, if it
/// does: a hollow block (torch, fence, flower) lets the ray through.
fn outline_stop(
    origin: [f64; 3],
    dir: [f64; 3],
    reach: f64,
    pos: [i32; 3],
    face: [i32; 3],
    boxes: impl FnOnce() -> Vec<[f64; 6]>,
) -> Option<Hit> {
    let (t, face) = interaction_shapes::entry(origin, dir, &boxes(), reach, face)?;
    Some(Hit {
        pos,
        face,
        point: std::array::from_fn(|i| origin[i] + dir[i] * t),
        inside: false,
    })
}
/// `VoxelShape.clip` of a fluid source's shape for a ray that starts in the
/// source's own block `cell`: the face and point it hits, in `pick_fluid`'s
/// `Hit` terms. `FlowingFluid.getShape` is the whole block under the same
/// fluid and `getOwnHeight` (8/9) tall otherwise. A start inside the shape
/// (tested 1e-7 of the way along) hits at the start, on the face opposite
/// `Direction.getApproximateNearest(dir)`; from above it, a ray going down
/// enters through the top while still in this block.
fn clip_start_source(
    eye: [f64; 3],
    dir: [f64; 3],
    reach: f64,
    cell: [i32; 3],
    same_above: bool,
) -> Option<([i32; 3], [f64; 3])> {
    let height = if same_above {
        1.0
    } else {
        f64::from(8.0f32 / 9.0f32)
    };
    let probe_y = eye[1] + dir[1] * reach * 1.0e-7;
    if probe_y - f64::from(cell[1]) < height {
        // Direction.values() order; the first of equal dot products wins.
        let normals: [[i32; 3]; 6] = [
            [0, -1, 0],
            [0, 1, 0],
            [0, 0, -1],
            [0, 0, 1],
            [-1, 0, 0],
            [1, 0, 0],
        ];
        let mut nearest = normals[2];
        let mut best = f64::NEG_INFINITY;
        for n in normals {
            let dot =
                f64::from(n[0]) * dir[0] + f64::from(n[1]) * dir[1] + f64::from(n[2]) * dir[2];
            if dot > best {
                best = dot;
                nearest = n;
            }
        }
        return Some((nearest.map(|v| -v), eye));
    }
    if dir[1] >= 0.0 {
        return None;
    }
    let t = (f64::from(cell[1]) + height - eye[1]) / dir[1];
    if t > reach {
        return None;
    }
    let point = [
        eye[0] + dir[0] * t,
        eye[1] + dir[1] * t,
        eye[2] + dir[2] * t,
    ];
    (point[0].floor() as i32 == cell[0] && point[2].floor() as i32 == cell[2])
        .then_some(([0, 1, 0], point))
}

fn reset_world_progress(adv: &mut advancements::AdvState, stats: &mut stats::Stats) {
    *adv = Default::default();
    *stats = Default::default();
}

pub(crate) use closest_space::closest_space_nudge;
#[cfg(test)]
use inventory_input::drop_whole_stack;
use subtitles::{Subtitles, in_game_ui, subtitles};

use hud_render::{block_outline_drawn, debug_crosshair_drawn, is_game_master_block};
#[cfg(test)]
mod test_neighbourhood {
    //! Shared fixture step for tests that tick entities on prepared data.
    use super::*;

    impl Game<'_> {
        /// Loads the chunks within 2 columns of `(cx, cz)` that are not loaded yet.
        /// Entity ticking waits for every chunk in that 5x5 neighbourhood
        /// (`ChunkMap.prepareEntityTickingChunk`), so a fixture that ticks mobs,
        /// items or TNT in a chunk calls this after setting up its own chunks.
        pub(crate) fn load_ticking_neighbourhood(&mut self, cx: i32, cz: i32) {
            for z in cz - 2..=cz + 2 {
                for x in cx - 2..=cx + 2 {
                    if self.world.chunks.contains_key(&(x, z)) {
                        continue;
                    }
                    self.world.chunks.insert(
                        (x, z),
                        std::sync::Arc::new(horizon_client::chunk::Chunk::empty(x, z)),
                    );
                    // A chunk that is already registered keeps its ticks.
                    let _ = self
                        .block_ticks
                        .add_empty_chunk(horizon_core::position::ChunkPos::new(x, z));
                }
            }
        }
    }
}

fn write_level_for_role(
    level: &mut Level,
    dir: &std::path::Path,
    role: execution_role::ExecutionRole,
) -> Result<()> {
    if role.writes_singleplayer_player() {
        level.write(dir)
    } else {
        level.write_without_player(dir)
    }
}

/// A server's data load with no mods and no settings, for loaders under test.
#[cfg(test)]
fn test_server_load() -> horizon_world::native_load_context::LoadContext {
    use horizon_world::native_load_context::{DataLoadOrigin, LoadContext, LoadInputs};
    LoadContext::new(
        DataLoadOrigin::ServerDataLoad,
        Arc::new(LoadInputs::new(Default::default(), Default::default())),
        [],
    )
}

#[cfg(test)]
mod nested_orb_spawn_tests {
    #[test]
    #[ignore = "daily"]
    fn experience_pickup_keeps_nested_orbs_until_the_next_ordered_pass() {
        crate::game::native_entity_refs::tests::with_game(|game| {
            game.world.chunks.insert(
                (0, 0),
                std::sync::Arc::new(horizon_client::chunk::Chunk::empty(0, 0)),
            );
            game.block_ticks
                .add_empty_chunk(horizon_core::position::ChunkPos::new(0, 0))
                .unwrap();
            for (x, value) in [(7.5, 1), (9.5, 3), (11.5, 7)] {
                game.add_orb([x, 80.0, 3.5], value);
            }
            let _ = game.orbs.remove(1);
            let mut expected: Vec<_> = game.orbs.iter().map(|o| (o.id, o.join)).collect();
            crate::game::entity::taken_pass_tests::with_spawn(
                |g| g.add_orb([3.5, 80.0, 3.5], 1),
                || game.tick_experience(),
            );
            assert_eq!(game.orbs.len(), 3);
            let join = game.orbs[2].join;
            expected.push((game.orbs[2].id, join));
            assert_eq!(
                game.orbs.iter().map(|o| (o.id, o.join)).collect::<Vec<_>>(),
                expected
            );
            assert_eq!(game.orbs[2].age, 0);
            game.ordered.announced = horizon_client::join_seq::JoinSeq::MAX;
            game.tickets.update(None);
            game.begin_ordered_ticks();
            game.run_ordered_ticks(horizon_client::join_seq::JoinSeq::MAX, false)
                .unwrap();
            let spawned = game.orbs.iter().find(|o| o.join == join).unwrap();
            assert_eq!(spawned.age, 1);
            assert_eq!(
                game.orbs.iter().map(|o| (o.id, o.join)).collect::<Vec<_>>(),
                expected
            );
        });
    }
}

#[cfg(test)]
pub(crate) fn in_own_process(name: &str, body: impl FnOnce()) {
    const OWN_PROCESS: &str = "HORIZON_TEST_OWN_PROCESS";
    if std::env::var_os(OWN_PROCESS).is_some_and(|v| v == name) {
        return body();
    }
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args([name, "--exact", "--include-ignored", "--test-threads=1"])
        .env(OWN_PROCESS, name)
        .output()
        .unwrap();
    let report = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        out.status.success() && report.contains("test result: ok. 1 passed;"),
        "{name} failed in its own process:\n{report}"
    );
}

impl Game<'_> {
    fn presence_status(&self) -> horizon_client::social::PresenceStatus {
        use horizon_client::social::PresenceStatus;
        if self.integrated.is_some() {
            // `getSingleplayerServer().getMultiplayerScope()`: LAN while the world is
            // opened to LAN (a pause-menu Open to LAN or an invite code).
            if self.lan.is_some() {
                PresenceStatus::PlayingHostedServer
            } else {
                PresenceStatus::PlayingOffline
            }
        } else {
            PresenceStatus::Online
        }
    }
}

#[cfg(test)]
#[path = "game/interface/tests/sign_chain_tests.rs"]
mod sign_chain_tests;

#[cfg(test)]
#[path = "game/interface/tests/sign_use_tests.rs"]
mod sign_use_tests;

#[cfg(test)]
#[path = "game/interface/tests/camera_roll_tests.rs"]
mod camera_roll_tests;

/// For each block state, whether its block entity draws off screen (1: beacons, end gateways,
/// structure and test instance blocks, `BlockEntityRenderer.shouldRenderOffScreen`) and far
/// (2: end gateways), by its name. Worked out once for these facts and reused while they live.
fn block_entity_reach(resources: &Resources) -> Arc<[u8]> {
    resources
        .block_entity_reach
        .get_or_init(|| {
            resources
                .facts()
                .states
                .iter()
                .map(|state| match &*state.name {
                    "minecraft:end_gateway" => 3,
                    "minecraft:beacon"
                    | "minecraft:structure_block"
                    | "minecraft:test_instance_block" => 1,
                    _ => 0,
                })
                .collect()
        })
        .clone()
}
