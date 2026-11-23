//! A server's levels, one per dimension (`MinecraftServer.levels`).
//!
//! The game's tick code reads one level: the one in place, held in `Game`'s
//! own fields (the world, its streaming and save, block ticks, entities).
//! A server (the dedicated server and single player's server thread) keeps
//! the other dimensions' levels here and ticks each one with players or forced
//! chunks by swapping it into place (`enter_level`), the way `as_guest` swaps a player
//! in, so the tick code runs unchanged, one level at a time.
//!
//! Shared by every level, as vanilla's derived levels share the
//! Overworld's level data: the game time, the day time and the weather, the
//! game rules, the scoreboard, the maps and the entities waiting to arrive
//! through a portal.

use super::commands::Target;
use super::entity_residency::{define_resident_access, resident_owners};
use super::{
    Dim, Game, commands,
    creaking::CreakingState, dragon, fireworks, lightning, machines,
    ominous_item_spawner, persist, raids::Raiding, sculk::SculkState, tnt, trident,
};
use myriade_engine::join_seq::{
    Joined,
    residency::{
        Journal, Kind, Owner,
        table::{EntityLookup, Location, SourceEntity},
    },
};
use myriade_engine::spans::time as span;
use myriade_engine::{
    Result,
    block_entity_anim::BlockEntityAnims,
    random_tick_sections::RandomTickSections,
    save::SaveDir,
    streaming::Streaming,
    tick_chunk_membership::TickChunkMembership,
    ticket_levels::{ChunkTickets, PortalTicket},
    vanillagen::VanillaTerrain,
    world::World,
};
use horizon_world::level::{LevelEvent, Ticks};
use myriade_types::DimensionKey;
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

/// A borrowed counting view: no resident-table scans and no allocation for gauges.
pub(super) struct EntityCountView {
    pub(super) fixed: [(&'static str, usize); 17],
    /// The entities the level's ECS world holds (`EcsLevel::count`).
    pub(super) ecs: usize,
}
impl EntityCountView {
    pub(super) fn total(&self) -> usize {
        self.fixed.iter().map(|(_, n)| n).sum::<usize>() + self.ecs
    }
    pub(super) fn registry_counts(&self, ecs_kinds: &[(&'static str, usize)]) -> Vec<(String, u32)> {
        let mut counts = std::collections::BTreeMap::<String, u32>::new();
        for &(kind, n) in ecs_kinds {
            *counts.entry(kind.to_owned()).or_default() += n as u32;
        }
        for &(id, n) in &self.fixed {
            if n > 0 {
                *counts.entry(id.to_owned()).or_default() += n as u32;
            }
        }
        counts.into_iter().collect()
    }
}

// Both Game and Parked own these typed families. Keep their instrumentation in one list.
macro_rules! entity_count_view {
    ($level:expr) => {
        crate::game::levels::EntityCountView {
            fixed: [
                ("minecraft:item", $level.ecs.item_ids().len()),
                ("minecraft:experience_orb", $level.ecs.species_count("minecraft:experience_orb")),
                ("minecraft:boat", $level.ecs.boat_ids().len()),
                ("minecraft:falling_block", $level.ecs.species_count("minecraft:falling_block")),
                ("minecraft:painting", $level.ecs.species_count("minecraft:painting")),
                ("minecraft:item_frame", $level.ecs.species_count("minecraft:item_frame") + $level.ecs.species_count("minecraft:glow_item_frame")),
                ("minecraft:armor_stand", $level.ecs.species_count("minecraft:armor_stand")),
                ("minecraft:trident", $level.ecs.species_count("minecraft:trident")),
                ("minecraft:firework_rocket", $level.ecs.species_count("minecraft:firework_rocket")),
                ("minecraft:tnt", $level.ecs.species_count("minecraft:tnt")),
                ("minecraft:end_crystal", $level.ecs.species_count("minecraft:end_crystal")),
                ("minecraft:leash_knot", $level.ecs.species_count("minecraft:leash_knot")),
                ("minecraft:ominous_item_spawner", $level.ecs.species_count("minecraft:ominous_item_spawner")),
                ("minecraft:cushion", $level.ecs.species_count("minecraft:cushion")),
                ("minecraft:area_effect_cloud", $level.ecs.species_count("minecraft:area_effect_cloud")),
                ("minecraft:evoker_fangs", $level.ecs.species_count("minecraft:evoker_fangs")),
                // Multipart hitboxes are not independently admitted world entities.
                (
                    "minecraft:ender_dragon",
                    $level.ecs.species_count("minecraft:ender_dragon"),
                ),
            ],
            ecs: $level.ecs.entity_count(),
        }
    };
}
pub(super) use entity_count_view;

/// Server ticks a level other than the Overworld stays loaded with no player
/// in it, before it is saved and closed (vanilla unloads a level's chunks
/// once no ticket holds them; the Overworld stays loaded).
const IDLE_TICKS: u32 = 300;

/// A level that is not in place: every per-level field of `Game`.
pub(super) struct Parked {
    pub(super) world: World,
    streaming: Streaming,
    locator: Option<Arc<VanillaTerrain>>,
    save: Arc<SaveDir>,
    block_ticks: Ticks,
    tick_chunks: TickChunkMembership,
    tickets: ChunkTickets,
    random_tick_sections: RandomTickSections,
    level_events: Vec<LevelEvent>,
    /// Entity portal processes and cooldowns belong to this level's entities.
    pub(super) portal_states: HashMap<u32, myriade_engine::nether_portal::PortalState>,
    /// Which of this level's chunks may hold a portal (`Game::find_portals`), kept per level:
    /// one cache asked by two levels in turn read every palette of both again each tick.
    pub(super) portal_sections: myriade_engine::portal_sections::PortalSections,
    paintings: myriade_engine::join_seq::JoinList<myriade_engine::entities::Painting>,
    item_frames: myriade_engine::join_seq::JoinList<myriade_engine::entities::ItemFrame>,
    hoppers: HashMap<(i32, i32, i32), machines::HopperState>,
    pub(super) ecs: myriade_engine::ecs_bridge::EcsLevel,
    persist: persist::EntityMemory,
    lectern_pulses: Vec<((i32, i32, i32), i32)>,
    lid_openers: HashMap<(i32, i32, i32), i32>,
    lan_events: Vec<super::QueuedLevelEvent>,
    lan_block_events: Vec<([i32; 3], i32, i32, u32)>,
    lan_entity_events: Vec<(i32, i8)>,
    lan_critical_hits: Vec<(Option<u32>, i32, bool)>,
    /// The hub's tracked entities of this level (`LanHost::states`), as its last tick described
    /// them: what its players' moves are checked against before this tick describes them again.
    lan_states: myriade_engine::lan::entity::States,
    /// Only the per-level parts of these are used (`swap_level`).
    anims: BlockEntityAnims,
    pub(super) raiding: Raiding,
    sculk: SculkState,
    creaking: CreakingState,
    /// Natural spawning's candidate cache: each level keeps its own players' squares.
    spawn_scratch: super::spawn_level::SpawnScratch,
    /// Server ticks since a player was last in the level.
    idle: u32,
    /// The level's own world border (`ServerLevel.getWorldBorder`), and what its players were
    /// last told of it (`LanHost::border_sent`).
    border: myriade_engine::worldborder::WorldBorder,
    border_sent: Option<myriade_engine::lan::border::Sent>,
}

impl Parked {
    resident_owners!(define_resident_access, self);
}

impl Game<'_> {
    /// Resolve through the exact owner, never by trying dimension swaps.
    pub(super) fn locate_server_resident_owner(
        &self,
        at: Location,
    ) -> Option<SourceEntity<(Target, bool)>> {
        if matches!(
            at.owner.kind,
            myriade_engine::join_seq::residency::Kind::Player
                | myriade_engine::join_seq::residency::Kind::Hook
        ) {
            return self.locate_specialized_resident_owner(at);
        }
        if at.owner.dimension == self.dim.key() {
            // An open transaction's removal reads as gone already.
            return self.locate_resident_owner(at).map(|mut source| {
                source.value.1 &= !self.removal_pending(&source.value.0);
                source
            });
        }
        let dim = match at.owner.dimension {
            DimensionKey::OVERWORLD => Dim::Overworld,
            DimensionKey::NETHER => Dim::Nether,
            DimensionKey::END => Dim::End,
            key => Dim::Extra(key),
        };
        // The transactions stay with the game when a level is parked: its pending removals too.
        self.levels
            .parked
            .get(&dim)?
            .locate_resident_owner(at)
            .map(|mut source| {
                source.value.1 &= !self.removal_pending_in(dim, &source.value.0);
                source
            })
    }

    /// Whether `target` counts as removed in the level in place: a mod removed it, or an open
    /// transaction did and its root commit has not run the removal yet.
    pub(in crate::game) fn target_removed(&self, target: &Target) -> bool {
        self.persist.removals.contains(target)
            || self.persist.destroying_vehicles.contains(target)
            || self.removal_pending(target)
    }

    pub(in crate::game) fn resident_source_target_visible(
        &self,
        dimension: DimensionKey,
        target: Target,
    ) -> bool {
        let player = match target {
            Target::Guest(conn) => Some(conn),
            Target::Player => self.lan_acting,
            _ => None,
        };
        if player
            .and_then(|conn| self.command_guest(conn))
            .is_some_and(|guest| !guest.body_present())
        {
            return false;
        }
        if dimension == self.dim.key() {
            return !self.target_removed(&target);
        }
        let dim = match dimension {
            DimensionKey::OVERWORLD => Dim::Overworld,
            DimensionKey::NETHER => Dim::Nether,
            DimensionKey::END => Dim::End,
            key => Dim::Extra(key),
        };
        self.levels.parked.get(&dim).is_none_or(|parked| {
            !parked.persist.removals.contains(&target)
                && !parked.persist.destroying_vehicles.contains(&target)
                && !self.removal_pending_in(dim, &target)
        })
    }

    pub(in crate::game) fn server_resident_entity(&self, id: u32) -> EntityLookup<Target> {
        let found = self
            .resident_entities
            .table
            .borrow_mut()
            .server_entity_lookup(id, |at| self.locate_server_resident_owner(at));
        match found {
            EntityLookup::Found {
                location,
                value: (value, true),
            } => EntityLookup::Found { location, value },
            EntityLookup::Found {
                value: (_, false), ..
            }
            | EntityLookup::Missing => EntityLookup::Missing,
            EntityLookup::Taken => EntityLookup::Taken,
        }
    }
    /// Find the canonical level without swapping or probing physical families.
    pub(super) fn resident_target_dimension(&self, target: Target) -> Option<Dim> {
        let id = self.resident_target_id(target)?;
        let found = match self.server_resident_entity(id) {
            EntityLookup::Found { location, value }
                if value == target
                    || matches!(
                        (value, target),
                        (Target::Player, Target::Guest(_)) | (Target::Guest(_), Target::Player)
                    ) =>
            {
                location
            }
            _ => return None,
        };
        Some(match found.owner.dimension {
            DimensionKey::OVERWORLD => Dim::Overworld,
            DimensionKey::NETHER => Dim::Nether,
            DimensionKey::END => Dim::End,
            key => Dim::Extra(key),
        })
    }

    /// A new epoch invalidates every old producer before rebinding retained levels.
    pub(super) fn reset_entity_residency(&mut self) {
        let table = self.resident_entities.table.get_mut();
        table.reset();
        let journal = table.producer();
        self.bind_resident_owners(&journal, self.dim.key(), true);
        self.bind_specialized_resident_owners(&journal);
        for (dim, parked) in &mut self.levels.parked {
            parked.bind_resident_owners(&journal, dim.key(), true);
        }
    }
}

/// Which passes of the server tick the level in place runs: the first
/// reads the network (`Hub::poll`), the last ends the tick for every player
/// (`Hub::end_tick`). A tick with one level is both. `running` is the
/// tick rate manager's answer for this server tick, asked once before the
/// levels (`MinecraftServer.tickServer`); `None` outside `tick_server`.
#[derive(Clone, Copy)]
pub(super) struct Pass {
    pub first: bool,
    pub last: bool,
    pub running: Option<bool>,
    /// Set once this level's player packets were handled before block ticks.
    pub packets_drained: bool,
}

impl Default for Pass {
    fn default() -> Self {
        Self {
            first: true,
            last: true,
            running: None,
            packets_drained: false,
        }
    }
}

/// How long a server tick took and what it spent the time in.
pub(super) struct TickTiming {
    pub wall_ms: f32,
    pub cpu_ms: Option<f32>,
    /// Spans recorded inside the tick only, in observation order.
    pub raw: Vec<myriade_engine::spans::Span>,
}

/// The levels not in place, by dimension.
#[derive(Default)]
pub(super) struct Levels {
    pub(super) parked: HashMap<Dim, Parked>,
    /// `max-chained-neighbor-updates` of a dedicated server, for the levels it opens;
    /// `None` is the default every other server runs with.
    max_chained_neighbor_updates: Option<i32>,
}

impl Levels {
    /// Sets the neighbour update chain limit of the levels made from now on.
    pub(super) fn set_max_chained_neighbor_updates(&mut self, max: i32) {
        self.max_chained_neighbor_updates = Some(max);
    }

    /// The block ticks of a new level, with the server's neighbour update chain limit.
    pub(super) fn new_ticks(&self, random_tick_seed: i32) -> Ticks {
        let mut ticks = Ticks::new(random_tick_seed);
        if let Some(max) = self.max_chained_neighbor_updates {
            ticks.set_max_chained_neighbor_updates(max);
        }
        ticks
    }

    /// Clear every parked level.
    pub(super) fn clear(&mut self) {
        self.parked.clear();
    }

    /// A parked level, without opening or loading it.
    pub(super) fn parked_world(&self, dim: Dim) -> Option<&World> {
        self.parked.get(&dim).map(|level| &level.world)
    }

    /// The world of the level not in place whose dimension is `dimension`
    /// (`minecraft:the_nether`), found without collecting the parked levels.
    pub(super) fn parked_world_of(&self, dimension: &str) -> Option<&World> {
        self.parked
            .iter()
            .find(|(dim, _)| dim.id() == dimension)
            .map(|(_, level)| &level.world)
    }

    /// The world of the level not in place whose dimension is `dimension`, to
    /// change.
    pub(super) fn parked_world_mut_of(&mut self, dimension: &str) -> Option<&mut World> {
        self.parked
            .iter_mut()
            .find(|(dim, _)| dim.id() == dimension)
            .map(|(_, level)| &mut level.world)
    }

    /// The streaming of the level not in place whose dimension is `dimension`.
    pub(super) fn parked_streaming_of(&self, dimension: &str) -> Option<&Streaming> {
        self.parked
            .iter()
            .find(|(dim, _)| dim.id() == dimension)
            .map(|(_, level)| &level.streaming)
    }

    /// A parked level's existing generator query worker, without switching levels.
    pub(super) fn parked_locator_of(&self, dimension: &str) -> Option<Arc<VanillaTerrain>> {
        self.parked_of(dimension)?.locator.clone()
    }

    /// The streaming of the level not in place whose dimension is `dimension`, to change.
    pub(super) fn parked_streaming_of_mut(&mut self, dimension: &str) -> Option<&mut Streaming> {
        self.parked
            .iter_mut()
            .find(|(dim, _)| dim.id() == dimension)
            .map(|(_, level)| &mut level.streaming)
    }

    /// `ServerLevel.setChunkForced`: force a column in an already open level without changing the acting
    /// player's level; the same queue and load barrier serve the active level.
    pub(super) fn force_parked_chunk(
        &mut self,
        dimension: &str,
        x: i32,
        z: i32,
        forced: bool,
    ) -> Option<bool> {
        let level = self.parked_mut_of(dimension)?;
        Some(
            level
                .streaming
                .set_forced_loaded(&mut level.world, x, z, forced),
        )
    }

    pub(super) fn parked_anims(&self, dim: Dim) -> Option<&BlockEntityAnims> {
        self.parked.get(&dim).map(|level| &level.anims)
    }

    /// The worlds of the levels not in place.
    /// A parked level's world border.
    pub(super) fn parked_border(
        &self,
        dim: Dim,
    ) -> Option<&myriade_engine::worldborder::WorldBorder> {
        self.parked.get(&dim).map(|level| &level.border)
    }

    pub(super) fn install_profession_save_snapshots(
        &self,
        table: Arc<myriade_engine::mobs::villager::ResolvedProfessions>,
    ) {
        for level in self.parked.values() {
            level.save.install_profession_snapshot(table.clone());
        }
    }

    pub(super) fn parked_worlds(&self) -> impl Iterator<Item = &World> {
        self.parked.values().map(|level| &level.world)
    }

    /// The streamers of the levels not in place.
    pub(super) fn parked_streamings(
        &self,
    ) -> impl Iterator<Item = &myriade_engine::streaming::Streaming> {
        self.parked.values().map(|level| &level.streaming)
    }

    /// The level not in place, in `dim`.
    pub(super) fn parked_mut(&mut self, dim: Dim) -> Option<&mut Parked> {
        self.parked.get_mut(&dim)
    }

    /// A parked level by its exact dimension identity, without swapping it in.
    pub(super) fn parked_of(&self, dimension: &str) -> Option<&Parked> {
        self.parked
            .iter()
            .find(|(dim, _)| dim.id() == dimension)
            .map(|(_, level)| level)
    }
    pub(super) fn parked_mut_of(&mut self, dimension: &str) -> Option<&mut Parked> {
        self.parked
            .iter_mut()
            .find(|(dim, _)| dim.id() == dimension)
            .map(|(_, level)| level)
    }

    /// Whether a parked level's ECS entities hold any of these UUIDs (arismoko/horizon#792's
    /// dimension-transfer check: a destination may not already hold a row
    /// of the travelling tree's UUIDs). Missing on the bridge: `EcsLevel::holds_any_uuid`.
    pub(in crate::game) fn parked_general_holds_any_uuid(
        &self,
        dim: Dim,
        uuids: &std::collections::HashSet<[i32; 4]>,
    ) -> bool {
        self.parked
            .get(&dim)
            .is_some_and(|parked| parked.ecs.holds_any_uuid(uuids))
    }

    /// The entities of a parked level, counted as `Game::entity_count` counts the one in place.
    pub(super) fn parked_entity_count(&self, dim: Dim) -> Option<usize> {
        let l = self.parked.get(&dim)?;
        Some(
            l.ecs.entity_count()
                + l.ecs.species_count("minecraft:falling_block")
                + l.paintings.len()
                + l.item_frames.len()
                + l.ecs.species_count("minecraft:armor_stand")
                + l.ecs.species_count("minecraft:cushion")
                + l.ecs.species_count("minecraft:trident")
                + l.ecs.species_count("minecraft:firework_rocket")
                + l.ecs.species_count("minecraft:tnt"),
        )
    }

    pub(super) fn parked_perf_views(
        &self,
    ) -> impl Iterator<Item = (Dim, &World, EntityCountView, Vec<(&'static str, usize)>)> {
        self.parked.iter().map(|(&dim, level)| {
            (
                dim,
                &level.world,
                entity_count_view!(level),
                level.ecs.count_by_kind(),
            )
        })
    }

    /// The ECS entities of every parked level, by kind (a scan for the reports): the minecarts and boats among them are counted here,
    /// by kind, as the level in place's are.
    pub(super) fn parked_ecs_kinds(&self) -> impl Iterator<Item = (&'static str, usize)> + '_ {
        self.parked.values().flat_map(|l| l.ecs.count_by_kind())
    }

    /// A parked level's loose entities kept in lists, for `horizon-report`: items, experience orbs,
    /// minecarts (no list: ECS entities) and boats.
    pub(super) fn parked_entity_counts(&self, dim: Dim) -> Option<[usize; 4]> {
        self.parked.get(&dim).map(|l| {
            [
                l.ecs.species_count("minecraft:item"),
                l.ecs.species_count("minecraft:experience_orb"),
                0,
                l.ecs.boat_ids().len(),
            ]
        })
    }

    /// The levels that are not in place, by dimension.
    /// The forced chunks of every level not in place, released.
    pub(super) fn release_parked_forced_chunks(&mut self) {
        for level in self.parked.values_mut() {
            level.streaming.clear_forced();
        }
    }

    pub(super) fn parked_dims(&self) -> Vec<Dim> {
        self.parked.keys().copied().collect()
    }
}

impl Game<'_> {
    /// A new level for `dim`, opened from the save (`MinecraftServer.createLevels`).
    pub(super) fn open_level(&self, dim: Dim) -> Result<Parked> {
        let (streaming, save, locator) = super::world_loading::make_streaming(
            &self.res,
            &self.pack,
            &self.options,
            &self.settings,
            &self.world_root,
            dim,
            self.level_shapes.get(dim.id()),
            self.level.seed,
            &self.level.enabled_features,
            self.world_biomes
                .as_ref()
                .unwrap_or_else(|| self.res.biomes())
                .clone(),
        )?;
        save.install_profession_snapshot(self.stations().professions.clone());
        let mut world = World::new(self.res.facts().air, self.level.seed);
        world.shape = self.level_shapes.get(dim.id());
        world.fast_lava = self.dimension_type_of(dim).fast_lava;
        world.infiniburn = Some(self.resolve_level_infiniburn(self.dimension_type_of(dim)));
        let tickets = ChunkTickets::with_saved_portal(save.load_portal_tickets());
        Ok(Parked {
            world,
            streaming,
            locator,
            save,
            block_ticks: self.levels.new_ticks(self.level.seed as i32),
            tick_chunks: Default::default(),
            tickets,
            random_tick_sections: Default::default(),
            level_events: Vec::new(),
            portal_states: HashMap::new(),
            portal_sections: Default::default(),
            paintings: Default::default(),
            item_frames: Default::default(),
            hoppers: HashMap::new(),
            ecs: self.new_ecs_level(dim, self.level.seed),
            persist: Default::default(),
            lectern_pulses: Vec::new(),
            lid_openers: HashMap::new(),
            lan_events: Vec::new(),
            lan_block_events: Vec::new(),
            lan_entity_events: Vec::new(),
            lan_critical_hits: Vec::new(),
            lan_states: Default::default(),
            anims: Default::default(),
            raiding: Default::default(),
            sculk: Default::default(),
            creaking: Default::default(),
            spawn_scratch: Default::default(),
            idle: 0,
            border: self.load_border(dim),
            border_sent: None,
        })
    }

    /// `MinecraftServer.prepareLevels` activates every level's saved tickets:
    /// a level holding saved `PORTAL` tickets opens with the world and ticks
    /// while they hold it (`shouldKeepDimensionActive`).
    pub(super) fn open_saved_portal_levels(&mut self) -> Result<()> {
        let back = self.dim;
        for dim in [Dim::Overworld, Dim::Nether, Dim::End] {
            if dim == back || self.levels.parked.contains_key(&dim) {
                continue;
            }
            let save = SaveDir::new(
                myriade_engine::save::dimension_dir(&self.world_root, dim.id()),
                self.level_shapes.get(dim.id()),
            );
            if !save.load_portal_tickets().is_empty() {
                self.enter_level(dim)?;
                self.enter_level(back)?;
            }
        }
        Ok(())
    }

    /// `MinecraftServer.prepareLevels`: before the first tick, the chunks the
    /// open levels' restored tickets hold (forced chunks, a native mod's) are
    /// loaded (`ChunkLoadCounter`). The first tick then finds them ticking
    /// and unpacks their saved block and fluid ticks at the game time they
    /// were saved at (`startTickingChunk`), not at whichever tick they finish
    /// streaming in.
    pub(super) fn prepare_levels(&mut self) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(600);
        self.wait_for_ticket_chunks(deadline);
        let back = self.dim;
        for dim in [Dim::Overworld, Dim::Nether, Dim::End] {
            if dim != back && self.levels.parked.contains_key(&dim) {
                self.enter_level(dim)?;
                self.wait_for_ticket_chunks(deadline);
            }
        }
        if self.dim != back {
            self.enter_level(back)?;
        }
        Ok(())
    }

    /// Stream the level in place until each chunk its tickets make block
    /// ticking is a ticking chunk (lit, with its neighbours loaded).
    fn wait_for_ticket_chunks(&mut self, deadline: Instant) {
        // Forced chunks are checked two chunks out (their ticket block ticks one).
        // A `PORTAL` ticket's chunks are streamed as an area of their own (the
        // network tick does it, which hasn't run yet), whose chunks out to its
        // radius can become ticking chunks: those are waited for.
        let portal = self.portal_ticket_areas();
        for area in &portal {
            if !self.streaming.extra.contains(area) {
                self.streaming.extra.push(*area);
            }
        }
        loop {
            self.update_chunk_tickets();
            let levels = self.tickets.levels();
            let streaming = &self.streaming;
            let ready = streaming
                .forced
                .iter()
                .map(|&c| (c, 2))
                .chain(portal.iter().copied())
                .all(|((cx, cz), reach)| {
                    (cz - reach..=cz + reach).all(|z| {
                        (cx - reach..=cx + reach).all(|x| {
                            !levels.loaded_ticking(x, z) || super::ticking_chunk(&self.world, x, z)
                        })
                    })
                });
            if ready {
                return;
            }
            if Instant::now() >= deadline {
                log::warn!("Horizon: the chunks of the saved tickets did not load");
                return;
            }
            self.update_world();
            self.streaming.wait(Duration::from_millis(5));
        }
    }

    /// Exchange the level in place with `p`.
    fn swap_level(&mut self, p: &mut Parked) {
        use std::mem::swap;
        swap(&mut self.world, &mut p.world);
        // The carrier first-use gate belongs to the world, not the
        // level: a level swap keeps it.
        self.world.carrier_data_existed_at_open = p.world.carrier_data_existed_at_open;
        swap(&mut self.streaming, &mut p.streaming);
        let mut locator = self.end.locator();
        swap(&mut locator, &mut p.locator);
        self.end.set_locator(locator);
        self.share_spawn_conditions();
        swap(&mut self.save, &mut p.save);
        swap(&mut self.block_ticks, &mut p.block_ticks);
        swap(&mut self.tick_chunks, &mut p.tick_chunks);
        swap(&mut self.tickets, &mut p.tickets);
        swap(&mut self.random_tick_sections, &mut p.random_tick_sections);
        swap(&mut self.level_events, &mut p.level_events);
        swap(&mut self.portals.entities, &mut p.portal_states);
        swap(&mut self.portals.sections, &mut p.portal_sections);
        swap(&mut self.hoppers, &mut p.hoppers);
        swap(&mut self.ecs, &mut p.ecs);
        swap(&mut self.persist, &mut p.persist);
        swap(&mut self.lectern_pulses, &mut p.lectern_pulses);
        swap(&mut self.lid_openers, &mut p.lid_openers);
        swap(&mut self.lan_events, &mut p.lan_events);
        swap(&mut self.lan_block_events, &mut p.lan_block_events);
        swap(&mut self.lan_entity_events, &mut p.lan_entity_events);
        swap(&mut self.lan_critical_hits, &mut p.lan_critical_hits);
        let (a, b) = (&mut self.be_anims, &mut p.anims);
        swap(&mut a.chests, &mut b.chests);
        swap(&mut a.books, &mut b.books);
        swap(&mut a.shulkers, &mut b.shulkers);
        swap(&mut a.bells, &mut b.bells);
        swap(&mut a.conduits, &mut b.conduits);
        swap(&mut a.skulls, &mut b.skulls);
        swap(&mut a.pots, &mut b.pots);
        swap(&mut a.gateways, &mut b.gateways);
        swap(&mut a.gateway_servers, &mut b.gateway_servers);
        swap(&mut a.conduit_servers, &mut b.conduit_servers);
        self.raiding.swap_level(&mut p.raiding);
        self.sculk.swap_level(&mut p.sculk);
        self.creaking.swap_level(&mut p.creaking);
        swap(&mut self.spawn_scratch, &mut p.spawn_scratch);
        swap(&mut self.border, &mut p.border);
        // What only the client draws starts over.
    }

    /// Put `target`'s level in place, opening it first if it is not loaded.
    /// The level that was in place is kept with the others; the shared
    /// clock and weather go with the swap.
    pub(super) fn enter_level(&mut self, target: Dim) -> Result<()> {
        // A client holds no levels.
        if target != self.dim {
            let (mut p, opened) = match self.levels.parked.remove(&target) {
                Some(p) => (p, false),
                None => (self.open_level(target)?, true),
            };
            p.world.game_time = self.world.game_time;
            // The level's day time is its default clock's; a level without one follows the
            // Overworld's.
            let clock = self.level_clock_name(target);
            p.world.day_time = self.switch_clock(&clock);
            p.world.end_clock = self.world.end_clock;
            p.world.weather = self.world.weather;
            // The game rules and the difficulty are the server's, not a
            // level's: natural regeneration holds in every dimension.
            p.world.health_rules = self.world.health_rules;
            // The sculk tables' watched blocks are built once per game.
            p.world.watched = std::mem::take(&mut self.world.watched);
            self.swap_level(&mut p);
            // The tracked entities go with their level: a level's moves, read before its tick
            // describes its entities again, would otherwise meet the last level's (the anticheat
            // missed the boat under a player while anyone was in another dimension).
            let states = self.lan.as_mut().map(|h| &mut h.states);
            if let Some(states) = states {
                std::mem::swap(states, &mut p.lan_states);
            }
            if let Some(h) = self.lan.as_mut() {
                std::mem::swap(&mut h.border_sent, &mut p.border_sent);
            }
            p.idle = 0;
            let left = std::mem::replace(&mut self.dim, target);
            self.levels.parked.insert(left, p);
            if opened {
                // The new streaming's radius and meshing options.
                // ServerLevel's constructor reads the level's raids.
                self.read_raids();
            }
        }
        if let Some(h) = self.lan.as_mut() {
            h.hub.set_dimension(target.id());
        }
        self.sync_hub_clocks();
        Ok(())
    }

    /// Follow the direct mount of a travelling player up to the root of its vehicle tree, in the parked level's ECS world: every
    /// vehicle and rider there is an ECS entity, so the chain is the seats'. None when the root is gone or being removed (a later
    /// player in the same travelling stack finds no root: the first arrival already moved it).
    pub(super) fn parked_vehicle_root(&self, from: Dim, mut id: u32) -> Option<u32> {
        let p = self.levels.parked.get(&from)?;
        for _ in 0..64 {
            let Some(parent) = p.ecs.vehicle_of(id) else { break };
            id = parent;
        }
        let removed = p.persist.removals.iter().chain(p.persist.destroying_vehicles.iter()).any(|target| matches!(
            target, commands::Target::Mob(n) | commands::Target::Boat(n) | commands::Target::Cart(n) if *n == id
        ));
        (p.ecs.entity_of(id).is_some() && !removed).then_some(id)
    }

    /// Takes the ECS vehicle tree rooted at `root` out of the parked level `from`: the ids of the tree (the root first, each vehicle
    /// before its riders) and the root's saved form, whose `Passengers` carry the riders. The tree is despawned there; per-entity
    /// state lives in components, so nothing else of the source is moved.
    pub(super) fn take_parked_ecs_tree(
        &mut self,
        from: Dim,
        root: u32,
    ) -> Option<(Vec<u32>, horizon_core::nbt::Compound)> {
        let p = self.levels.parked.get_mut(&from)?;
        let saved = p.ecs.entity_nbt(p.ecs.entity_of(root)?)?;
        let tree = p.ecs.ride_tree(root);
        for id in &tree {
            if let Some(e) = p.ecs.entity_of(*id) {
                p.ecs.despawn(e);
            }
        }
        Some((tree, saved))
    }

    /// Far terrain for a client in `dim`: the level in place may be another
    /// one, the server ticking its levels in turn.
    pub(super) fn lod_source_in(
        &self,
        dim: Dim,
    ) -> Option<Arc<dyn myriade_engine::streaming::LodSource>> {
        if dim == self.dim {
            return Some(self.streaming.lod_source());
        }
        let parked = self.levels.parked.get(&dim)?;
        Some(parked.streaming.lod_source())
    }

    /// Put `dim`'s level in place for a command whose source is there
    /// (`execute in`, `execute at` a player in another level), opening it if
    /// nobody is in it; returns the level to put back afterwards. Only a
    /// server holds other levels.
    pub(super) fn enter_command_level(&mut self, dim: Dim) -> Option<Dim> {
        if dim == self.dim {
            return None;
        }
        let back = self.dim;
        match self.enter_level(dim) {
            Ok(()) => Some(back),
            Err(e) => {
                log::error!("could not open {}: {e}", dim.id());
                None
            }
        }
    }

    /// One server tick (`MinecraftServer.tickServer`): each level with
    /// players or forced chunks in turn (`tickChildren`), its chunks streamed
    /// and loaded and its world ticked. The first pass reads the network and runs the
    /// tick's shared parts (the clock, the weather, functions, the autosave);
    /// the next ones only their level. While `paused`, only the connections
    /// run. The native server tick hook brackets it (`ServerTickStart` before
    /// the first level, `ServerTickEnd` after the last).
    pub(super) fn tick_server(&mut self, paused: bool) -> Result<()> {
        self.with_residency_tick(|game| game.tick_server_body(paused))
    }

    fn tick_server_body(&mut self, paused: bool) -> Result<()> {
        // `tickRateManager.tick()` once per server tick, before any level
        // (each `ServerLevel.tick` only reads `runsNormally`), and the native
        // `ServerTickStart` before that level work (NeoForge's
        // `ServerTickEvent.Pre`). A paused pass and a held server tick nothing.
        let running = if !paused {
            self.clock.check_sprint_completion();
            if let Some(report) = self.clock.take_sprint_report() {
                // ServerTickRateManager.finishTickSprint: the server's sendSuccess.
                self.console_broadcast = Some(true);
                self.sprint_report(&report);
                self.console_broadcast = None;
                self.send_admin_feedback(None);
                self.admin_feedback.clear();
                // Then onTickRateChanged, its mods outside the server's broadcast.
                self.on_tick_rate_changed();
            }
            Some(self.clock.tick_world_runs())
        } else {
            None
        };
        let dims = self.levels_to_tick();
        let order = dims.clone();
        let n = dims.len();
        let mut result = Ok(());
        for (k, dim) in dims.into_iter().enumerate() {
            // Each level's share of the tick (the first one also reads the network and runs the
            // tick's shared parts): `horizon-report`'s `level_*` spans.
            horizon_threads::work::set_dim(match dim {
                Dim::Overworld => 1,
                Dim::Nether => 2,
                Dim::End => 3,
                Dim::Extra(_) => 0,
            });
            let _level_span = myriade_engine::spans::guard(match dim {
                Dim::Overworld => "level_overworld",
                Dim::Nether => "level_the_nether",
                Dim::End => "level_the_end",
                Dim::Extra(_) => "level_extra",
            });
            if let Err(e) = self.enter_level(dim) {
                result = Err(e);
                break;
            }
            self.level_pass = Pass {
                first: k == 0,
                last: k + 1 == n,
                running,
                packets_drained: false,
            };
            myriade_engine::spans::time("dedicated_streaming", || {
                self.update_dedicated_streaming();
            });
            myriade_engine::spans::time("update_world", || self.update_world());
            let tick_span = myriade_engine::spans::guard("level_tick");
            result = {
                    if paused {
                        self.tick_lan(true);
                        Ok(())
                    } else if k == 0 {
                        self.tick()
                    } else {
                        let running = running.unwrap_or_else(|| self.clock.world_runs());
                        self.tick_world(running)
                    }
                };
            drop(tick_span);
            if result.is_err() {
                break;
            }
            #[cfg(feature = "state-digest")]
            self.state_digest_tick(dim, running);
            // An entity that crossed into a level that already ticked this
            // server tick is in it when the tick ends (`addDuringTeleport`).
            if running == Some(true) {
                let _span = myriade_engine::spans::guard("late_portal_arrivals");
                let mut moved = false;
                for &earlier in &order[..k] {
                    if !self.arrival_chunks(earlier).is_empty() && self.enter_level(earlier).is_ok()
                    {
                        self.land_portal_arrivals();
                        moved = true;
                    }
                }
                if moved && k + 1 < n {
                    self.enter_level(dim)?;
                }
            }
        }
        // The levels only `Loaded` areas name: streamed and sent, never ticked.
        if result.is_ok() {
            result = self.run_view_passes(&order);
        }
        if let Some(&first) = order.first()
            && self.dim != first
        {
            let _ = self.enter_level(first);
        }
        self.level_pass = Pass::default();
        result?;
        let closed = {
            let _span = myriade_engine::spans::guard("close_idle_levels");
            self.close_idle_levels()
        };
        closed?;
        Ok(())
    }

    /// `tick_server_timed`, handing the tick's timing to `record` while spans are on.
    pub(super) fn tick_server_spans(
        &mut self,
        paused: bool,
        record: impl FnOnce(TickTiming),
    ) -> Result<()> {
        let started = Instant::now();
        // The game carried on past any turn a caught panic interrupted before this tick.
        self.lan_interrupted_turns.clear();
        horizon_threads::work::tick_start();
        let result = self.tick_server_recorded(paused, record);
        horizon_threads::work::tick_end(started.elapsed().as_nanos() as u64);
        result
    }

    fn tick_server_recorded(
        &mut self,
        paused: bool,
        record: impl FnOnce(TickTiming),
    ) -> Result<()> {
        if self.options.spikes.is_none() && !myriade_engine::spans::recording_here() {
            return self.tick_server(paused);
        }
        // Discard between-tick work (such as the chunk pump) before timing this tick.
        myriade_engine::spans::take_raw();
        let (started, cpu) = (Instant::now(), myriade_engine::spans::thread_cpu());
        let result = self.tick_server(paused);
        let wall = started.elapsed().as_secs_f32() * 1000.0;
        let cpu = myriade_engine::spans::thread_cpu()
            .zip(cpu)
            .map(|(end, start)| (end - start).as_secs_f32() * 1000.0);
        // Taken every tick: nothing else takes this thread's spans.
        let raw = myriade_engine::spans::take_raw();
        // The spans go to the slow ring; whose packets or events took the tick stays in the owner's log.
        let notes = myriade_engine::spans::take_notes();
        if let Some(threshold) = self.options.spikes
            && wall > threshold
            && !notes.is_empty()
        {
            log::info!(
                "Horizon: {wall:.1} ms tick was busy with: {}",
                slowest_steps(&notes)
            );
        }
        record(TickTiming {
            wall_ms: wall,
            cpu_ms: cpu,
            raw,
        });
        result
    }

    /// What `Game::tick` runs for a server's first level, after the window's lead.
    pub(super) fn tick_first_level(&mut self) -> Result<()> {
        // Only a server ticks its own world from here: its players are guests, swapped in for
        // their turns (`as_guest`), and the window's player is a client of it.
        self.ticks += 1;
        // `/tick freeze` stops the world but not the player.
        let running = match self.level_pass.running {
            Some(running) => running,
            None => self.clock.world_runs(),
        };
        if running {
            // `MinecraftServer.tickChildren`: `commandFunctions` runs before
            // any level ticks, so a tick function reads the game time and the
            // day clock from before this tick. Vanilla's SavedData is loaded
            // long before this point; the lazy scoreboard/storage read has to
            // be done by then too, or a load tag writes over what it restores.
            self.load_scoreboard();
            span("tick_functions", || self.tick_functions());
            self.world.game_time += 1;
            if self
                .session
                .rules
                .bool_at(myriade_engine::gamerules::vanilla::ADVANCE_WEATHER)
            {
                let before = (self.world.weather.raining, self.world.weather.thundering);
                self.world.weather.tick_timers(&mut self.world.random);
                if before != (self.world.weather.raining, self.world.weather.thundering) {
                    self.ecs_weather_changed();
                }
            }
            self.world.weather.tick_levels();
        } else if self.clock.still() {
            // Under `HORIZON_STILL` a frozen world's rain and thunder sit at their levels, as a
            // shader pack's smoothed values do, so `/weather rain` shows full rain without
            // stepping the world (and its mobs) through the fade.
            self.world.weather.settle_levels();
        }
        let _ = span("player_autosaves", || self.finish_player_autosaves(false));
        // MinecraftServer.tickServer's countdown, then autoSave: three
        // hundred seconds of ticks at the rate the server runs at now.
        self.ticks_until_autosave -= 1;
        self.autosaving = self.ticks_until_autosave <= 0;
        if self.autosaving {
            span("autosave", || self.save_game_with(false))?;
            self.ticks_until_autosave = self.next_autosave_interval();
        }
        if running {
            self.advance_clock();
            // `ServerLevel.tickTime`, once the clock has moved on.
            span("scheduled_functions", || self.run_scheduled_functions());
        }
        self.tick_totem();
        self.update_totem_ready();
        span("tick_command_extras", || self.tick_command_extras());
        self.tick_world(running)
    }

    /// The end of `MinecraftServer.runServer`'s loop for a server thread:
    /// the next tick starts one tick length after this one, or straight away
    /// while the sprint runs (`checkShouldSprintThisTick`). A server more
    /// than two seconds behind skips the ticks it missed: that returns how far
    /// behind it was, for the overload warning.
    pub(super) fn wait_for_next_server_tick(
        &mut self,
        next: &mut Instant,
        paused: bool,
    ) -> Option<Duration> {
        let now = Instant::now();
        if !paused && self.clock.sprinting() {
            *next = now;
            return None;
        }
        *next += self.clock.tick_duration();
        if *next > now {
            // The chunks loading (the players', force-loaded ones, a
            // `/pregen` task) keep the workers fed until the next tick.
            self.pump_chunks(*next, paused);
            let now = Instant::now();
            if *next > now {
                std::thread::sleep(*next - now);
            }
        } else if now - *next > Duration::from_secs(2) {
            let behind = now - *next;
            *next = now;
            return Some(behind);
        }
        None
    }

    /// The levels this server tick runs, in `MinecraftServer.levels` order
    /// (the Overworld, the Nether, the End): every level with a player in
    /// it, forced chunks or a `/pregen` task, or the Overworld while idle.
    pub(super) fn levels_to_tick(&self) -> Vec<Dim> {
        let guests = self
            .lan
            .as_ref()
            .map_or(&[][..], |h| h.hub.guests.as_slice());
        let mut dims: Vec<Dim> = Dim::all()
            .into_iter()
            .filter(|d| {
                // `/forceload`ed chunks.
                let held = |s: &Streaming| !s.forced.is_empty();
                let forced = if *d == self.dim {
                    held(&self.streaming)
                } else {
                    self.levels
                        .parked
                        .get(d)
                        .is_some_and(|p| held(&p.streaming))
                };
                guests.iter().any(|g| g.dimension == d.id() && g.participates_in_player_simulation())
                    || self.player_bed_levels().any(|b| b == *d)
                    || forced
                    || self.pregen_level() == Some(*d)
                    // A pending portal arrival keeps its destination level
                    // running so it can land and tick (the portal ticket).
                    || !self.arrival_chunks(*d).is_empty()
                    // A portal ticket keeps its dimension active
                    // (`TicketType.shouldKeepDimensionActive`).
                    || !self.portal_tickets_in(*d).is_empty()
                    // So does a viewed `Ticking` area (arismoko/horizon#3716), for its chunks only.
                    || self.viewed_level(*d) == Some(true)
            })
            .collect();
        if dims.is_empty() {
            dims.push(Dim::Overworld);
        }
        dims
    }

    /// The portal tickets of `dim`'s level (`placePortalTicket`, or saved).
    pub(super) fn portal_tickets_in(&self, dim: Dim) -> &[PortalTicket] {
        if dim == self.dim {
            self.tickets.portal_tickets()
        } else {
            self.levels
                .parked
                .get(&dim)
                .map_or(&[], |p| p.tickets.portal_tickets())
        }
    }

    /// The areas the level in place's portal tickets keep streamed: around each
    /// exit its block-ticking chunks and their neighbours, the ring vanilla
    /// keeps `FULL` (level 33), so the outer block-ticking ring is a ticking
    /// chunk too; and at least the chunks its expiry waits for
    /// (`portal_ticket_can_expire`).
    pub(super) fn portal_ticket_areas(&self) -> Vec<((i32, i32), i32)> {
        let block_ticking = i32::from(myriade_engine::ticket_levels::BLOCK_TICKING);
        self.tickets
            .portal_tickets()
            .iter()
            .map(|t| (t.chunk, (block_ticking - t.level).max(1)))
            .collect()
    }

    /// Count the parked levels' empty ticks, and save and close the ones
    /// (other than the Overworld) nobody has been in for a while.
    pub(super) fn close_idle_levels(&mut self) -> Result<()> {
        let mut occupied = self.levels_to_tick();
        // A level a viewed area names, ticked or not, or one a client still holds chunks of.
        occupied.extend(Dim::all().into_iter().filter(|d| self.needs_view_pass(*d)));
        // A vehicle waiting there for its rider's arrival in another level.
        let waiting: Vec<Dim> = self
            .levels
            .parked
            .keys()
            .copied()
            .filter(|d| self.vehicle_waits_in(*d))
            .collect();
        let mut closing = Vec::new();
        for (dim, p) in &mut self.levels.parked {
            if occupied.contains(dim) || waiting.contains(dim) {
                p.idle = 0;
                continue;
            }
            p.idle += 1;
            if *dim != Dim::Overworld && p.idle >= IDLE_TICKS {
                closing.push(*dim);
            }
        }
        if closing.is_empty() {
            return Ok(());
        }
        for dim in closing {
            self.close_parked_level(dim)?;
        }
        Ok(())
    }

    /// Save the parked level `dim` and drop it: the mods hear its chunks and the level unload. A
    /// level a mod opened at run time stays known, and opens again when entered, as any other.
    pub(super) fn close_parked_level(&mut self, dim: Dim) -> Result<()> {
        let back = self.dim;
        self.enter_level(dim)?;
        // Vanilla saves the level's chunks before their unload hooks.
        if let Err(why) = self.save_level(true) {
            // Not saved: the level stays parked, and the level in place is the one before.
            self.enter_level(back)?;
            return Err(why);
        }
        self.enter_level(back)?;
        self.levels.parked.remove(&dim);
        Ok(())
    }

    /// A server's world opened: level.dat is read and no chunk is loaded. The
    /// mods hear its saved mod list's differences (NeoForge's
    /// `ModMismatchEvent`), then the level it opens in.
    pub(super) fn native_world_opened(&mut self, dim: Dim) {
        self.native_level_opened(dim);
    }

    /// The mods hear `dim`'s level open (`LevelEvent.Load`), on a server.
    pub(super) fn native_level_opened(&mut self, dim: Dim) {
        let journal = self.resident_entities.table.borrow().producer();
        if dim == self.dim {
            self.bind_resident_owners(&journal, dim.key(), true);
        } else if let Some(parked) = self.levels.parked.get_mut(&dim) {
            parked.bind_resident_owners(&journal, dim.key(), true);
        }
    }

    /// Save every parked level (`MinecraftServer.saveAllChunks`); the level
    /// in place is the caller's to save.
    pub(super) fn save_parked_levels(&mut self, wait: bool) -> Result<()> {
        if self.levels.parked.is_empty() {
            return Ok(());
        }
        let back = self.dim;
        let dims: Vec<Dim> = self.levels.parked.keys().copied().collect();
        for dim in dims {
            self.enter_level(dim)?;
            self.save_level(wait)?;
        }
        self.enter_level(back)
    }
}

/// The steps a slow tick noted as its slowest (`spans::note`), whose packets or which player's event.
fn slowest_steps(notes: &[(f32, String)]) -> String {
    let list: Vec<String> = notes
        .iter()
        .map(|(ms, what)| format!("{what} {ms:.1}"))
        .collect();
    list.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A game on the prepared resources in `HORIZON_TEST_RESOURCES`, with no
    /// world opened and commands allowed.
    fn prepared_game() -> Game<'static> {
        use crate::game::{Audio, Options, Progress, Settings, load_resources};
        let root = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").unwrap());
        let settings = Settings::default();
        let loaded = load_resources(
            horizon_assets::resource_source::ResourceRoot::directory(root.clone()),
            Vec::new(),
            None,
            &root,
            &settings,
            Instant::now(),
            crate::game::execution_role::ExecutionRole::DedicatedServer,
            &Progress::new(),
        )
        .unwrap();
        let mut options = Options::parse(std::iter::empty()).unwrap().unwrap();
        options.resources = root.clone();
        options.execution_role = crate::game::execution_role::ExecutionRole::DedicatedServer;
        // The game keeps its resources for the whole test.
        let resources: &'static _ = Box::leak(Box::new(loaded.resources));
        let mut game = Game::new(
            resources,
            loaded.pack,
            options,
            settings,
        )
        .unwrap();
        game.build_registries();
        game.set_allow_commands(true);
        game
    }

    /// `WorldBorderCommand.setSize` from `/worldborder add` with a time: the
    /// new change lasts the ticks still running plus the argument
    /// (`getLerpTime() + time`), where `/worldborder set` replaces it.
    #[test]
    #[ignore = "daily"]
    fn worldborder_add_extends_a_change_in_flight() {
        let mut game = prepared_game();
        assert!(game.run_command("worldborder set 100 1000"));
        assert_eq!(game.border.remaining_ticks(), 1000);
        // Part way through the change, where the size is between its ends.
        for _ in 0..100 {
            game.border.tick();
        }
        assert_eq!(game.border.remaining_ticks(), 900);
        let from = game.border.size();
        assert!(from < 59_999_968.0 && from > 100.0);
        // The new change starts from that size and lasts the ticks still
        // running plus the argument, not just the argument.
        assert!(game.run_command("worldborder add -50 5s"));
        assert_eq!(game.border.target_size(), from - 50.0);
        assert_eq!(game.border.remaining_ticks(), 1000);
        // `set` starts over from the size the border has now.
        assert!(game.run_command("worldborder set 200 40"));
        assert_eq!(game.border.target_size(), 200.0);
        assert_eq!(game.border.remaining_ticks(), 40);
    }

    /// `ServerLevel.getWorldBorder`: `/worldborder` in one level leaves the others' alone.
    #[test]
    #[ignore = "daily"]
    fn each_level_keeps_its_own_world_border() {
        let mut game = prepared_game();
        assert!(game.run_command("worldborder set 1000"));
        assert!(game.run_command("execute in minecraft:the_nether run worldborder set 100"));
        assert_eq!(game.dim, Dim::Overworld);
        assert_eq!(game.border.size(), 1000.0);
        let nether = game.levels.parked_border(Dim::Nether).unwrap();
        assert_eq!(nether.size(), 100.0);
    }

    /// `Game::find_portals` asks every tick whether a palette of the level's chunks may hold a
    /// portal (`PortalSections::any`). A level keeps its answers while another level ticks: its
    /// unchanged chunks' palettes are not read again, though the other level has chunks at the
    /// same x and z (a server ticks the Overworld and the Nether one after the other).
    #[test]
    #[ignore = "daily"]
    fn a_level_keeps_its_portal_palettes_while_another_ticks() {
        let mut game = prepared_game();
        let columns = |game: &mut Game| {
            for x in 0..4 {
                for z in 0..4 {
                    let chunk = myriade_engine::chunk::Chunk::empty(x, z);
                    game.world.chunks.insert((x, z), Arc::new(chunk));
                }
            }
        };
        // Asked as `level_may_hold_portal` asks, counting the palette entries read.
        let reads = |game: &mut Game| {
            let mut reads = 0;
            game.portals.sections.any(&game.world.chunks, |_| {
                reads += 1;
                false
            });
            reads
        };
        columns(&mut game);
        assert!(reads(&mut game) > 0, "the first ask reads the palettes");
        game.enter_level(Dim::Nether).unwrap();
        columns(&mut game);
        reads(&mut game);
        game.enter_level(Dim::Overworld).unwrap();
        assert_eq!(
            reads(&mut game),
            0,
            "the Overworld's palettes were read again"
        );
    }

    struct LogCapture;
    static LOGGED: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

    impl log::Log for LogCapture {
        fn enabled(&self, _: &log::Metadata) -> bool {
            true
        }
        fn log(&self, record: &log::Record) {
            LOGGED.lock().unwrap().push(record.args().to_string());
        }
        fn flush(&self) {}
    }

    /// 300 slow ticks in ten seconds log no Horizon slow-tick line, and the ring holds the last 256.
    #[test]
    #[ignore = "daily"]
    fn slow_ticks_enter_the_ring_and_not_the_log() {
        use crate::game::{Audio, Options, Progress, Settings, load_resources};
        use myriade_engine::server_perf::{ServerPerf, TickDetail, WorldCounts};
        log::set_logger(&LogCapture).expect("no other logger in this test run");
        log::set_max_level(log::LevelFilter::Info);
        let root = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").unwrap());
        let settings = Settings::default();
        let loaded = load_resources(
            horizon_assets::resource_source::ResourceRoot::directory(root.clone()),
            Vec::new(),
            None,
            &root,
            &settings,
            Instant::now(),
            crate::game::execution_role::ExecutionRole::DedicatedServer,
            &Progress::new(),
        )
        .unwrap();
        let mut options = Options::parse(std::iter::empty()).unwrap().unwrap();
        options.resources = root.clone();
        options.execution_role = crate::game::execution_role::ExecutionRole::DedicatedServer;
        options.spikes = Some(0.0);
        let mut game = Game::new(
            &loaded.resources,
            loaded.pack,
            options,
            settings,
        )
        .unwrap();
        game.build_registries();
        let mut perf = ServerPerf::new(0.0, 0.0);
        for i in 0..300 {
            let mut detail = None;
            let started = Instant::now();
            game.tick_server_spans(false, |timing| {
                detail = Some(TickDetail {
                    cpu_ms: timing.cpu_ms,
                    spans: timing.raw,
                });
            })
            .unwrap();
            let wall_ms = started.elapsed().as_secs_f32() * 1000.0;
            perf.tick(
                f64::from(i) / 30.0,
                wall_ms,
                detail,
                WorldCounts::default,
                Vec::new,
            );
        }
        let kept: Vec<u64> = perf.slow_records().map(|r| r.counter).collect();
        assert_eq!(kept, (45..=300).collect::<Vec<u64>>());
        let logged = LOGGED.lock().unwrap();
        assert!(
            logged.iter().all(|line| !line.contains("slow tick")),
            "{logged:?}"
        );
    }

    /// A game on a scratch world folder, the way `dedicated.rs` opens one
    /// (`load_world` with an absolute folder).
    fn open_game<'a>(
        resources: &'a mut Option<std::sync::Arc<crate::game::Resources>>,
        root: &std::path::Path,
        home: &std::path::Path,
        world: &std::path::Path,
    ) -> Game<'a> {
        use crate::game::{Audio, Options, Progress, Settings, load_resources};
        let settings = Settings::default();
        let loaded = load_resources(
            horizon_assets::resource_source::ResourceRoot::directory(root.to_path_buf()),
            Vec::new(),
            None,
            root,
            &settings,
            Instant::now(),
            crate::game::execution_role::ExecutionRole::DedicatedServer,
            &Progress::new(),
        )
        .unwrap();
        // The game keeps a reference to its resources for the whole test.
        let resources = resources.insert(loaded.resources);
        let mut options = Options::parse(std::iter::empty()).unwrap().unwrap();
        options.resources = root.to_path_buf();
        options.game_dir = Some(home.to_path_buf());
        options.execution_role = crate::game::execution_role::ExecutionRole::DedicatedServer;
        let mut game = Game::new(
            resources,
            loaded.pack,
            options,
            settings,
        )
        .unwrap();
        game.build_registries();
        game.set_allow_commands(true);
        let folder = world.to_string_lossy().into_owned();
        game.load_world(
            crate::game::world_loading::WorldRequest::new(
                &folder,
                Some("h260"),
                Some(260),
                true,
                None,
                std::sync::Arc::new(crate::game::api_values::game_rule_registry()),
            )
            .unwrap(),
        )
        .unwrap();
        game
    }

    /// Expected values from the official 26.3 dedicated server restarted on the
    /// same world: each level's `world_border` SavedData is read when the level
    /// is made, so the Overworld's saved border is in place for the first
    /// command, and a first `/worldborder set` is not overwritten later.
    #[test]
    #[ignore = "daily"]
    fn a_reopened_world_has_each_levels_saved_border_from_the_start() {
        let root = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").unwrap());
        let home = std::path::PathBuf::from(std::env::var_os("HORIZON_HOME").unwrap()).join("h260");
        let world = home.join("saves/h260");
        // A new world each run: a border saved by the last one would already be 50.
        let _ = std::fs::remove_dir_all(&home);
        {
            let mut resources = None;
            let mut game = open_game(&mut resources, &root, &home, &world);
            // A server ticks before it saves.
            game.tick_world(true).unwrap();
            assert!(game.run_command("worldborder set 50"));
            assert!(game.run_command("execute in minecraft:the_nether run worldborder set 100"));
            game.save_game().unwrap();
        }
        let mut resources = None;
        let mut game = open_game(&mut resources, &root, &home, &world);
        let mut failures = Vec::new();
        if game.border.size() != 50.0 {
            failures.push(format!("in place after the open: {}", game.border.size()));
        }
        assert!(game.run_command("worldborder set 70"));
        game.tick_world(true).unwrap();
        assert!(game.run_command("execute in minecraft:the_nether run worldborder get"));
        assert!(game.run_command("execute in minecraft:the_end run worldborder get"));
        assert_eq!(game.dim, Dim::Overworld);
        let sizes = (
            game.border.size(),
            game.levels.parked_border(Dim::Nether).map(|b| b.size()),
            game.levels.parked_border(Dim::End).map(|b| b.size()),
        );
        if sizes != (70.0, Some(100.0), Some(59_999_968.0)) {
            failures.push(format!("after the first set and a tick: {sizes:?}"));
        }
        assert!(failures.is_empty(), "reopened borders: {failures:#?}");
    }

    /// Added controls (H260, after the baseline): a new world, and a read and a
    /// set before the first tick that survive a tick, a save and another reopen.
    #[test]
    #[ignore = "daily"]
    fn reopened_borders_keep_through_a_tick_and_a_save_and_a_new_world_has_defaults() {
        let root = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").unwrap());
        let home = std::path::PathBuf::from(std::env::var_os("HORIZON_HOME").unwrap())
            .join("h260-controls");
        // A new world each run, as the first open expects.
        let _ = std::fs::remove_dir_all(&home);
        let world = home.join("saves/h260");
        let sizes = |game: &mut Game<'_>| {
            assert!(game.run_command("execute in minecraft:overworld run worldborder get"));
            assert!(game.run_command("execute in minecraft:the_nether run worldborder get"));
            assert!(game.run_command("execute in minecraft:the_end run worldborder get"));
            let parked = |d| game.levels.parked_border(d).map(|b| b.size());
            let size = |d: Dim| {
                if game.dim == d {
                    Some(game.border.size())
                } else {
                    parked(d)
                }
            };
            (size(Dim::Overworld), size(Dim::Nether), size(Dim::End))
        };
        const DEFAULT: f64 = 59_999_968.0;
        let mut failures = Vec::new();
        {
            let mut resources = None;
            let mut game = open_game(&mut resources, &root, &home, &world);
            // A new world: every level at the default.
            if game.border.size() != DEFAULT {
                failures.push(format!("new world in place: {}", game.border.size()));
            }
            game.tick_world(true).unwrap();
            let got = sizes(&mut game);
            if got != (Some(DEFAULT), Some(DEFAULT), Some(DEFAULT)) {
                failures.push(format!("new world: {got:?}"));
            }
            assert!(game.run_command("worldborder set 50"));
            assert!(game.run_command("execute in minecraft:the_nether run worldborder set 100"));
            game.save_game().unwrap();
        }
        {
            let mut resources = None;
            let mut game = open_game(&mut resources, &root, &home, &world);
            // Before the first tick: the read, then the set.
            if game.border.size() != 50.0 {
                failures.push(format!("first read before a tick: {}", game.border.size()));
            }
            assert!(game.run_command("worldborder set 70"));
            if game.border.size() != 70.0 {
                failures.push(format!("first set before a tick: {}", game.border.size()));
            }
            game.tick_world(true).unwrap();
            let got = sizes(&mut game);
            if got != (Some(70.0), Some(100.0), Some(DEFAULT)) {
                failures.push(format!("after a tick: {got:?}"));
            }
            game.save_game().unwrap();
        }
        {
            let mut resources = None;
            let mut game = open_game(&mut resources, &root, &home, &world);
            let got = sizes(&mut game);
            if got != (Some(70.0), Some(100.0), Some(DEFAULT)) {
                failures.push(format!("after the save and another reopen: {got:?}"));
            }
        }
        assert!(failures.is_empty(), "reopened borders: {failures:#?}");
    }

    /// `ImposterProtoChunk(…, false)`: a new chunk's features read a full
    /// neighbour but their ordinary `setBlockState` writes do not change it,
    /// while `BulkSectionAccess` (ores) writes its sections. Expected values
    /// from the official 26.3 server on the constructed 1.20.4 world
    /// (`H077_OLD_WORLD`: full old chunks, their partial ring removed), where
    /// chunks 13..14 at z 0 generate next to the full chunk (12, -1).
    #[test]
    #[ignore = "needs HORIZON_TEST_RESOURCES, a scratch HORIZON_HOME and the constructed H077_OLD_WORLD"]
    fn features_write_a_full_neighbour_only_through_its_sections() {
        let root = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").unwrap());
        let home = std::path::PathBuf::from(std::env::var_os("HORIZON_HOME").unwrap()).join("h078");
        let world = home.join("saves/h078");
        let source = std::path::PathBuf::from(std::env::var_os("H077_OLD_WORLD").unwrap());
        fn copy(from: &std::path::Path, to: &std::path::Path) {
            std::fs::create_dir_all(to).unwrap();
            for entry in std::fs::read_dir(from).unwrap().flatten() {
                let target = to.join(entry.file_name());
                if entry.file_type().unwrap().is_dir() {
                    copy(&entry.path(), &target);
                } else if entry.file_name() != "session.lock" {
                    std::fs::copy(entry.path(), target).unwrap();
                }
            }
        }
        copy(&source, &world);
        let mut resources = None;
        let mut game = open_game(&mut resources, &root, &home, &world);
        // The world's level.dat has cheats off: the open read it after open_game's setting.
        game.set_allow_commands(true);
        assert!(game.run_command("forceload add 208 0 239 15"));
        let mut loaded = false;
        // As the dedicated server's loop: a tick, then the chunk streaming.
        for _ in 0..2000 {
            game.tick_world(true).unwrap();
            game.update_world();
            std::thread::sleep(std::time::Duration::from_millis(10));
            if game.run_command_quietly("execute if loaded 215 64 8") {
                loaded = true;
                break;
            }
        }
        assert!(loaded, "the new chunk (13, 0) was not generated");
        game.save_game().unwrap();
        let mut failures = Vec::new();
        for (p, want) in [
            // A spruce tree and a mushroom of the new chunks: ordinary writes.
            ([207, 68, -10], "minecraft:air"),
            ([207, 69, -6], "minecraft:air"),
            ([202, 69, -5], "minecraft:air"),
            // A gold ore blob: section writes.
            ([207, -32, -12], "minecraft:deepslate_gold_ore"),
        ] {
            let got = game.block_name(p);
            if &*got != want {
                failures.push(format!("{p:?}: {got}, expected {want}"));
            }
        }
        assert!(
            failures.is_empty(),
            "the full chunk (12, -1): {failures:#?}"
        );
    }

    /// `BlendingData.pack` of an old chunk whose border columns a new chunk's
    /// generation calculated (`getOrUpdateBlendingData`), as the official 26.3
    /// server saves it: a constructed 1.20.4 world (full old chunks, their
    /// partial ring removed, `H077_OLD_WORLD`) where chunk (13, 0) is new.
    #[test]
    #[ignore = "needs HORIZON_TEST_RESOURCES, a scratch HORIZON_HOME and the constructed H077_OLD_WORLD"]
    fn an_old_chunk_saves_the_heights_its_new_neighbour_calculated() {
        use horizon_core::nbt::Tag;
        let root = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").unwrap());
        let home = std::path::PathBuf::from(std::env::var_os("HORIZON_HOME").unwrap()).join("h077");
        let world = home.join("saves/h077");
        let source = std::path::PathBuf::from(std::env::var_os("H077_OLD_WORLD").unwrap());
        fn copy(from: &std::path::Path, to: &std::path::Path) {
            std::fs::create_dir_all(to).unwrap();
            for entry in std::fs::read_dir(from).unwrap().flatten() {
                let target = to.join(entry.file_name());
                if entry.file_type().unwrap().is_dir() {
                    copy(&entry.path(), &target);
                } else if entry.file_name() != "session.lock" {
                    std::fs::copy(entry.path(), target).unwrap();
                }
            }
        }
        copy(&source, &world);
        {
            let mut resources = None;
            let mut game = open_game(&mut resources, &root, &home, &world);
            // The world's level.dat has cheats off: the open read it after open_game's setting.
            game.set_allow_commands(true);
            assert!(game.run_command("forceload add 208 0 239 15"));
            let mut loaded = false;
            // As the dedicated server's loop: a tick, then the chunk streaming.
            for _ in 0..2000 {
                game.tick_world(true).unwrap();
                game.update_world();
                std::thread::sleep(std::time::Duration::from_millis(10));
                if game.run_command_quietly("execute if loaded 215 64 8") {
                    loaded = true;
                    break;
                }
            }
            assert!(loaded, "the new chunk (13, 0) was not generated");
            game.save_game().unwrap();
        }
        let save = myriade_engine::save::SaveDir::new(
            myriade_engine::save::dimension_dir(&world, "minecraft:overworld"),
            myriade_engine::chunk::LevelShape::OVERWORLD,
        );
        let blending = |x, z| {
            save.raw_chunk(x, z)
                .and_then(|r| r.tag.get("blending_data").cloned())
        };
        let heights = |tag: &Tag| match tag.get("heights") {
            Some(Tag::List(list)) => Some(
                list.iter()
                    .map(|v| match v {
                        Tag::Float(bits) => *bits,
                        other => panic!("height {other:?}"),
                    })
                    .collect::<Vec<u32>>(),
            ),
            _ => None,
        };
        let sections = |tag: &Tag| {
            (
                tag.get("min_section").cloned(),
                tag.get("max_section").cloned(),
            )
        };
        let mut failures = Vec::new();
        let mut want = vec![f32::MAX.to_bits(); 12];
        want.extend([70.0f32, 70.0, 69.0, 68.0].map(f32::to_bits));
        match blending(12, 0) {
            Some(tag) => {
                if heights(&tag) != Some(want.clone()) {
                    failures.push(format!("(12, 0) heights {:?}", heights(&tag)));
                }
                if sections(&tag) != (Some(Tag::Int(-4)), Some(Tag::Int(20))) {
                    failures.push(format!("(12, 0) sections {:?}", sections(&tag)));
                }
            }
            None => failures.push("(12, 0) has no blending_data".into()),
        }
        // No calculation reached these: the compound stays, without heights.
        for (x, z) in [(11, 0), (0, 0)] {
            match blending(x, z) {
                Some(tag) => {
                    if heights(&tag).is_some()
                        || sections(&tag) != (Some(Tag::Int(-4)), Some(Tag::Int(20)))
                    {
                        failures.push(format!("({x}, {z}) {tag:?}"));
                    }
                }
                None => failures.push(format!("({x}, {z}) has no blending_data")),
            }
        }
        assert!(failures.is_empty(), "saved blending data: {failures:#?}");
    }

    /// Added control (H077, after the baseline): the exact cache state the save
    /// reads. After the generation of (13, 0), the Overworld folder's lookup
    /// holds the calculated data of the edge chunk (12, 0), with the official
    /// server's heights; an old neighbour without new sides holds no height,
    /// and a far chunk no entry.
    #[test]
    #[ignore = "needs HORIZON_TEST_RESOURCES, a scratch HORIZON_HOME and the constructed H077_OLD_WORLD"]
    fn the_overworld_cache_holds_the_edge_chunks_calculated_heights() {
        let root = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").unwrap());
        let home =
            std::path::PathBuf::from(std::env::var_os("HORIZON_HOME").unwrap()).join("h077-cache");
        let world = home.join("saves/h077");
        let source = std::path::PathBuf::from(std::env::var_os("H077_OLD_WORLD").unwrap());
        fn copy(from: &std::path::Path, to: &std::path::Path) {
            std::fs::create_dir_all(to).unwrap();
            for entry in std::fs::read_dir(from).unwrap().flatten() {
                let target = to.join(entry.file_name());
                if entry.file_type().unwrap().is_dir() {
                    copy(&entry.path(), &target);
                } else if entry.file_name() != "session.lock" {
                    std::fs::copy(entry.path(), target).unwrap();
                }
            }
        }
        copy(&source, &world);
        let mut resources = None;
        let mut game = open_game(&mut resources, &root, &home, &world);
        game.set_allow_commands(true);
        assert!(game.run_command("forceload add 208 0 239 15"));
        let mut loaded = false;
        for _ in 0..2000 {
            game.tick_world(true).unwrap();
            game.update_world();
            std::thread::sleep(std::time::Duration::from_millis(10));
            if game.run_command_quietly("execute if loaded 215 64 8") {
                loaded = true;
                break;
            }
        }
        assert!(loaded, "the new chunk (13, 0) was not generated");
        assert_eq!(game.dim, Dim::Overworld);
        let mut want = vec![f32::MAX.to_bits(); 12];
        want.extend([70.0f32, 70.0, 69.0, 68.0].map(f32::to_bits));
        let edge = game
            .save
            .calculated_blending(12, 0)
            .expect("(12, 0) was calculated");
        let (min, max, heights) = edge.pack();
        assert_eq!((min, max), (-4, 20));
        assert_eq!(heights.map(|h| h.map(f32::to_bits).to_vec()), Some(want));
        // (11, 0) borders no new chunk: the generation's calculation adds no height
        // to it (it may have an entry), so it packs without heights, as vanilla's.
        let inner = game.save.calculated_blending(11, 0);
        assert!(
            inner.as_ref().is_none_or(|data| data.pack().2.is_none()),
            "{inner:?}"
        );
        // (0, 0) is far from the new chunks: nothing calculated it.
        assert!(game.save.calculated_blending(0, 0).is_none());
    }
}

impl Game<'_> {
    pub(in crate::game) fn level_min_y(&self) -> i32 {
        self.world.shape.min_y
    }

    pub(in crate::game) fn eye_in_water(&self, x: f64, eye_y: f64, z: f64) -> bool {
        let (bx, by, bz) = (x.floor() as i32, eye_y.floor() as i32, z.floor() as i32);
        let f = self.res.facts().get(self.world.block(bx, by, bz));
        if f.fluid != myriade_engine::blocks::Fluid::Water {
            return false;
        }
        let above = self.res.facts().get(self.world.block(bx, by + 1, bz));
        let height = if above.fluid == myriade_engine::blocks::Fluid::Water {
            1.0
        } else {
            f64::from(f.fluid_level) / 9.0
        };
        eye_y <= f64::from(by) + height
    }
}
