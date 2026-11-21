//! LevelLoadingScreen: "Loading terrain..." over the chunk status map and
//! the server's load progress while a world opens or a portal is crossed.
/// `Minecraft.doWorldLoad`: `max(5, 3) + ChunkLevel.RADIUS_AROUND_FULL_CHUNK + 1`.
pub const STATUS_RADIUS: i32 = 8;
/// `LevelLoadProgressTracker.EXPECTED_PLAYER_CHUNKS`: the 7x7 chunks
/// `PrepareSpawnTask` waits for around the player.
const PLAYER_CHUNKS: usize = 49;
/// `LevelLoadProgressTracker.PREPARE_SERVER_WEIGHT`.
const PREPARE_WEIGHT: usize = 10;

/// `ChunkStatus`, as far as the chunk status map tells them apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Empty,
    StructureStarts,
    StructureReferences,
    Biomes,
    Terrain,
    Features,
    InitializeLight,
    Light,
    Spawn,
    Full,
}
impl Status {
    /// `LevelLoadingScreen.COLORS` (RGB).
    pub fn color(self) -> u32 {
        match self {
            Self::Empty => 0x545454,
            Self::StructureStarts => 0x999999,
            Self::StructureReferences => 0x5F6191,
            Self::Biomes => 0x80B252,
            Self::Terrain => 0x303572,
            Self::Features => 0x21C600,
            Self::InitializeLight => 0xCCCCCC,
            Self::Light => 0xFFE0A0,
            Self::Spawn => 0xF26060,
            Self::Full => 0xFFFFFF,
        }
    }
}

/// `LevelLoadingScreen.Reason`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    NetherPortal,
    EndPortal,
    Other,
}

/// `LevelLoadTracker.CLIENT_WAIT_TIMEOUT_MS`.
pub const CLIENT_WAIT_TIMEOUT_MS: u64 = 30_000;
/// `LevelLoadTracker.LEVEL_LOAD_CLOSE_DELAY_MS`: how long a new world's loading
/// screen stays once the level is ready.
pub const LEVEL_LOAD_CLOSE_DELAY_MS: u64 = 500;

/// `ClientPacketListener.determineLevelLoadingReason`.
pub fn reason_for_respawn(player_died: bool, dimension: &str, old_dimension: &str) -> Reason {
    const NETHER: &str = "minecraft:the_nether";
    const END: &str = "minecraft:the_end";
    if player_died {
        Reason::Other
    } else if dimension == NETHER || old_dimension == NETHER {
        Reason::NetherPortal
    } else if dimension == END || old_dimension == END {
        Reason::EndPortal
    } else {
        Reason::Other
    }
}

/// `LevelLoadTracker.ClientState`, with the times in milliseconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ClientState {
    /// `WaitingForServer`: until the loading start game event.
    WaitingForServer { timeout_after: u64 },
    /// `WaitingForPlayerChunk`: until the section around the player is shown.
    WaitingForPlayerChunk {
        player_section_ready: bool,
        timeout_after: u64,
    },
    /// `ClientLevelReady`.
    Ready { ready_at: u64 },
}

/// What `WaitingForPlayerChunk.isReady` asks of the player.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerCheck {
    /// `ClientLevel.isOutsideBuildHeight(camera y)`.
    pub outside_build_height: bool,
    pub spectator: bool,
    pub alive: bool,
}

/// The client half of `LevelLoadTracker`: what the loading screen waits for
/// after a login or a respawn, and the screen it is shown on.
#[derive(Clone, Copy, Debug)]
pub struct LevelLoadTracker {
    state: Option<ClientState>,
    transition_screen: Option<usize>,
    close_delay_ms: u64,
    /// `LevelLoadingScreen.reason`.
    pub reason: Reason,
}
impl LevelLoadTracker {
    pub fn new(close_delay_ms: u64) -> Self {
        Self {
            state: None,
            transition_screen: None,
            close_delay_ms,
            reason: Reason::Other,
        }
    }
    /// `startClientLoad`, for the screen `reason` (`startWaitingForNewLevel`).
    pub fn start_client_load(&mut self, now: u64, reason: Reason) {
        self.state = Some(ClientState::WaitingForServer {
            timeout_after: now + CLIENT_WAIT_TIMEOUT_MS,
        });
        self.reason = reason;
        self.transition_screen = None;
    }
    /// The tracker's work is over and the screen has closed
    /// (`ClientPacketListener.tick` drops it once the level is ready).
    pub fn finish(&mut self) {
        self.state = None;
        self.transition_screen = None;
    }
    /// Retain the presentation selected once at dimension entry, until this load finishes.
    pub fn set_transition_screen(&mut self, screen: Option<usize>) {
        if self.is_active() {
            self.transition_screen = screen;
        }
    }
    /// The selected presentation, absent for vanilla or after completion.
    pub fn transition_screen(&self) -> Option<usize> {
        self.transition_screen
    }
    /// A load is under way, so its screen is up.
    pub fn is_active(&self) -> bool {
        self.state.is_some()
    }
    /// `loadingPacketsReceived`: `LEVEL_CHUNKS_LOAD_START` came.
    pub fn loading_packets_received(&mut self) {
        if let Some(ClientState::WaitingForServer { timeout_after }) = self.state {
            self.state = Some(ClientState::WaitingForPlayerChunk {
                player_section_ready: false,
                timeout_after,
            });
        }
    }
    /// `getPlayerCompiledSectionCallback` is not null: the renderer is to say
    /// when the player's section is shown.
    pub fn wants_player_section(&self) -> bool {
        matches!(self.state, Some(ClientState::WaitingForPlayerChunk { .. }))
    }
    /// The callback: the player's section is compiled and visible.
    pub fn player_section_ready(&mut self) {
        if let Some(ClientState::WaitingForPlayerChunk {
            player_section_ready,
            ..
        }) = &mut self.state
        {
            *player_section_ready = true;
        }
    }
    /// `tickClientLoad`.
    pub fn tick_client_load(&mut self, now: u64, player: PlayerCheck) {
        if let Some(ClientState::WaitingForPlayerChunk {
            player_section_ready,
            timeout_after,
        }) = self.state
        {
            let ready = if now > timeout_after {
                log::warn!(
                    "Timed out while waiting for the client to load chunks, letting the player into the world anyway"
                );
                true
            } else if !player.outside_build_height && !player.spectator && player.alive {
                player_section_ready
            } else {
                true
            };
            if ready {
                self.state = Some(ClientState::Ready { ready_at: now });
            }
        }
    }
    /// `isLevelReady`.
    pub fn is_level_ready(&self, now: u64) -> bool {
        matches!(self.state, Some(ClientState::Ready { ready_at }) if now >= ready_at + self.close_delay_ms)
    }
}

/// `LevelLoadListener.Stage`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    StartServer,
    PrepareGlobalSpawn,
    LoadInitialChunks,
    LoadPlayerChunks,
}

/// `LevelLoadProgressTracker` (with `includePlayerChunks`).
#[derive(Debug, Default)]
struct ProgressTracker {
    total_weight: usize,
    finalized_weight: usize,
    segment_weight: usize,
    segment_fraction: f32,
    progress: f32,
}
impl ProgressTracker {
    fn tracks(stage: Stage) -> bool {
        matches!(stage, Stage::LoadInitialChunks | Stage::LoadPlayerChunks)
    }
    fn begin_segment(&mut self, weight: usize) {
        self.segment_weight = weight;
        self.segment_fraction = 0.0;
        self.update_progress();
    }
    fn finish_segment(&mut self) {
        self.finalized_weight += self.segment_weight;
        self.segment_weight = 0;
        self.update_progress();
    }
    fn update_progress(&mut self) {
        self.progress = if self.total_weight == 0 {
            0.0
        } else {
            (self.finalized_weight as f32 + self.segment_fraction * self.segment_weight as f32)
                / self.total_weight as f32
        };
    }
    fn start(&mut self, stage: Stage, total_chunks: usize) {
        match stage {
            Stage::LoadInitialChunks => {
                self.total_weight = PREPARE_WEIGHT + total_chunks + PLAYER_CHUNKS;
                self.begin_segment(PREPARE_WEIGHT);
                self.finish_segment();
                self.begin_segment(total_chunks);
            }
            Stage::LoadPlayerChunks => self.begin_segment(PLAYER_CHUNKS),
            _ => {}
        }
    }
    fn update(&mut self, stage: Stage, current: usize, total: usize) {
        if Self::tracks(stage) {
            self.segment_fraction = if total == 0 {
                0.0
            } else {
                current as f32 / total as f32
            };
            self.update_progress();
        }
    }
    fn finish(&mut self, stage: Stage) {
        if Self::tracks(stage) {
            self.finish_segment();
        }
    }
}

/// The integrated server's side of `LevelLoadTracker`: what its
/// `LevelLoadListener` reported (`serverProgress`, `hasProgress`) and the
/// chunk status map around the load focus (`ChunkLoadStatusView`), which the
/// server thread writes and the window's loading screen reads.
#[derive(Debug, Default)]
pub struct ServerLoadView {
    tracker: std::sync::Mutex<ProgressTracker>,
    /// `serverStage != null`.
    has_progress: std::sync::atomic::AtomicBool,
    /// `statusView.get(x, z)` for x and z in `0..=2 * STATUS_RADIUS`.
    cells: std::sync::Mutex<Vec<Option<Status>>>,
}
impl ServerLoadView {
    /// `LevelLoadListener.start`.
    pub fn start(&self, stage: Stage, total_chunks: usize) {
        Self::lock(&self.tracker).start(stage, total_chunks);
        // `serverStage = stage`, whatever the stage.
        self.has_progress
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
    /// `LevelLoadListener.update`.
    pub fn update(&self, stage: Stage, current: usize, total: usize) {
        Self::lock(&self.tracker).update(stage, current, total);
    }
    /// `LevelLoadListener.finish`.
    pub fn finish(&self, stage: Stage) {
        Self::lock(&self.tracker).finish(stage);
    }
    /// `LevelLoadTracker.hasProgress`.
    pub fn has_progress(&self) -> bool {
        self.has_progress.load(std::sync::atomic::Ordering::Relaxed)
    }
    /// `LevelLoadTracker.serverProgress`.
    pub fn server_progress(&self) -> f32 {
        Self::lock(&self.tracker).progress
    }
    /// The map `ChunkLoadStatusView.get` reads, replaced as a whole.
    pub fn set_status(&self, cells: Vec<Option<Status>>) {
        *Self::lock(&self.cells) = cells;
    }
    /// `ChunkLoadStatusView.get`.
    pub fn status(&self, x: i32, z: i32) -> Option<Status> {
        let diameter = (STATUS_RADIUS * 2 + 1) as usize;
        let (x, z) = (usize::try_from(x).ok()?, usize::try_from(z).ok()?);
        if x >= diameter || z >= diameter {
            return None;
        }
        Self::lock(&self.cells)
            .get(x * diameter + z)
            .copied()
            .flatten()
    }
    fn lock<T>(m: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
        m.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// `LevelLoadProgressTracker.get` once the server is prepared, with `ready`
/// of the player's 49 chunks loaded (no forced chunks to load first).
pub fn progress(ready: usize) -> f32 {
    let total = PREPARE_WEIGHT + PLAYER_CHUNKS;
    (PREPARE_WEIGHT + ready.min(PLAYER_CHUNKS)) as f32 / total as f32
}

/// `LevelLoadingScreen.tick`: the bar eases a fifth of the way each tick.
pub fn smooth(smoothed: f32, progress: f32) -> f32 {
    smoothed + (progress - smoothed) * 0.2
}

#[cfg(test)]
mod tracker_tests {
    use super::*;

    const ALIVE: PlayerCheck = PlayerCheck {
        outside_build_height: false,
        spectator: false,
        alive: true,
    };

    #[test]
    fn server_progress_follows_the_listener_like_the_progress_tracker() {
        let view = ServerLoadView::default();
        assert!(!view.has_progress() && view.server_progress() == 0.0);
        // A stage the tracker ignores still counts as progress being made.
        view.start(Stage::StartServer, 0);
        assert!(view.has_progress() && view.server_progress() == 0.0);
        // prepareLevels with no forced chunks: the 10 of preparing, of 59.
        view.start(Stage::LoadInitialChunks, 0);
        view.finish(Stage::LoadInitialChunks);
        assert_eq!(view.server_progress(), 10.0 / 59.0);
        // PrepareSpawnTask: the 49 player chunks.
        view.start(Stage::LoadPlayerChunks, 49);
        view.update(Stage::LoadPlayerChunks, 20, 49);
        assert_eq!(view.server_progress(), (10.0 + 20.0) / 59.0);
        assert_eq!(view.server_progress(), progress(20));
        view.update(Stage::LoadPlayerChunks, 49, 49);
        view.finish(Stage::LoadPlayerChunks);
        assert_eq!(view.server_progress(), 1.0);
    }

    #[test]
    fn the_status_map_is_read_from_the_north_west_corner() {
        let view = ServerLoadView::default();
        assert_eq!(view.status(0, 0), None);
        let diameter = (STATUS_RADIUS * 2 + 1) as usize;
        let mut cells = vec![None; diameter * diameter];
        cells[3 * diameter + 5] = Some(Status::Full);
        view.set_status(cells);
        assert_eq!(view.status(3, 5), Some(Status::Full));
        assert_eq!(view.status(5, 3), None);
        assert_eq!(view.status(-1, 0), None);
        assert_eq!(view.status(diameter as i32, 0), None);
    }

    #[test]
    fn the_load_waits_for_the_server_then_the_players_section() {
        let mut tracker = LevelLoadTracker::new(0);
        assert!(!tracker.is_active());
        tracker.start_client_load(1_000, Reason::NetherPortal);
        assert!(tracker.is_active() && tracker.reason == Reason::NetherPortal);
        // Nothing is asked of the renderer before the start event, and a
        // section shown early counts for nothing.
        assert!(!tracker.wants_player_section());
        tracker.player_section_ready();
        tracker.tick_client_load(1_050, ALIVE);
        assert!(!tracker.is_level_ready(1_050));
        tracker.loading_packets_received();
        assert!(tracker.wants_player_section());
        tracker.tick_client_load(1_100, ALIVE);
        assert!(!tracker.is_level_ready(1_100));
        tracker.player_section_ready();
        tracker.tick_client_load(1_150, ALIVE);
        assert!(tracker.is_level_ready(1_150));
        tracker.finish();
        assert!(!tracker.is_active() && !tracker.is_level_ready(2_000));
    }

    #[test]
    fn a_player_who_cannot_see_the_section_does_not_wait_for_it() {
        for player in [
            PlayerCheck {
                outside_build_height: true,
                ..ALIVE
            },
            PlayerCheck {
                spectator: true,
                ..ALIVE
            },
            PlayerCheck {
                alive: false,
                ..ALIVE
            },
        ] {
            let mut tracker = LevelLoadTracker::new(0);
            tracker.start_client_load(0, Reason::Other);
            tracker.loading_packets_received();
            tracker.tick_client_load(10, player);
            assert!(tracker.is_level_ready(10));
        }
    }

    #[test]
    fn the_wait_times_out_after_thirty_seconds_but_only_once_the_server_has_started() {
        let mut tracker = LevelLoadTracker::new(0);
        tracker.start_client_load(0, Reason::Other);
        tracker.tick_client_load(CLIENT_WAIT_TIMEOUT_MS + 1, ALIVE);
        assert!(!tracker.is_level_ready(CLIENT_WAIT_TIMEOUT_MS + 1));
        tracker.loading_packets_received();
        tracker.tick_client_load(CLIENT_WAIT_TIMEOUT_MS, ALIVE);
        assert!(!tracker.is_level_ready(CLIENT_WAIT_TIMEOUT_MS));
        tracker.tick_client_load(CLIENT_WAIT_TIMEOUT_MS + 1, ALIVE);
        assert!(tracker.is_level_ready(CLIENT_WAIT_TIMEOUT_MS + 1));
    }

    #[test]
    fn a_new_worlds_screen_stays_for_the_close_delay() {
        let mut tracker = LevelLoadTracker::new(LEVEL_LOAD_CLOSE_DELAY_MS);
        tracker.start_client_load(0, Reason::Other);
        tracker.loading_packets_received();
        tracker.player_section_ready();
        tracker.tick_client_load(100, ALIVE);
        assert!(!tracker.is_level_ready(599));
        assert!(tracker.is_level_ready(600));
    }

    #[test]
    fn a_respawn_names_its_screen_by_the_dimensions() {
        let (overworld, nether, end) = (
            "minecraft:overworld",
            "minecraft:the_nether",
            "minecraft:the_end",
        );
        assert_eq!(
            reason_for_respawn(false, nether, overworld),
            Reason::NetherPortal
        );
        assert_eq!(
            reason_for_respawn(false, overworld, nether),
            Reason::NetherPortal
        );
        assert_eq!(reason_for_respawn(false, end, overworld), Reason::EndPortal);
        assert_eq!(reason_for_respawn(false, end, nether), Reason::NetherPortal);
        assert_eq!(
            reason_for_respawn(false, overworld, overworld),
            Reason::Other
        );
        // A death never shows a portal.
        assert_eq!(reason_for_respawn(true, nether, overworld), Reason::Other);
    }
}
