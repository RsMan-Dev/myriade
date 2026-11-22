//! The integrated server (`IntegratedServer`): single player's world runs on
//! its own "Server thread", as vanilla's does, and the window joins it as a
//! player through memory, so a world tick never holds up a frame.
//!
//! The window asks its server for what only the server knows (Open to LAN,
//! the world's options) with [`Integrated::call`], as vanilla's client calls
//! into its `IntegratedServer`.
//!
//! Every single-player world runs this way; LAN guests and dedicated servers
//! use the same server code.
use super::{
    Dim, Game, Options, PackResources, Resources, Settings,
    execution_role::ExecutionRole,
    lan::{
        LanSettings, Owner,
        sync::{SyncPreload, preload_sync_data},
    },
    world_loading::{WorldLoader, WorldRequest},
};
use horizon_client::level_loading::{STATUS_RADIUS, Stage};
use horizon_client::spans::startup;
use horizon_client::{
    Result,
    audio::Audio,
    lan::{Lan, MAX_PLAYERS, NetConfig, OwnerProfile, hub::HubConfig},
};
use super::world_options::WorldOptions;
use horizon_protocol::connection::Pipe;
use horizon_protocol::{crypto::offline_uuid, text::translatable};
use std::{
    net::TcpListener,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicI32, AtomicU16, AtomicU32, Ordering},
        mpsc::{Receiver, RecvTimeoutError, Sender, channel, sync_channel},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

/// How long the window waits for the server thread to answer a `call`.
const CALL_TIMEOUT: Duration = Duration::from_secs(10);
/// The error of a `call` without an answer: the server stopped, or it was
/// busy past `CALL_TIMEOUT` (the log says which).
pub(super) const NO_ANSWER: &str =
    "the integrated server stopped or did not answer within 10 s (the game log says which)";

/// The server thread holds its `Game` by value and runs every tick on it.
/// It reserves the same guest limit and host headroom as the main thread.
const SERVER_THREAD_STACK: usize =
    horizon_guest_stack::GUEST_STACK_BYTES + horizon_guest_stack::HOST_HEADROOM;

/// The server thread's stack size: [`SERVER_THREAD_STACK`], or what `RUST_MIN_STACK` asks for
/// when that is larger. std reads the variable only for a thread spawned without a size, and
/// harnesses raise it for this thread (`tools/test_wind_burst_hook.py`, the parity tools).
fn server_thread_stack() -> usize {
    static SIZE: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *SIZE.get_or_init(|| stack_for(std::env::var("RUST_MIN_STACK").ok().as_deref()))
}

/// [`SERVER_THREAD_STACK`], or `min_stack` (a byte count, as std parses `RUST_MIN_STACK`) when
/// that is larger.
fn stack_for(min_stack: Option<&str>) -> usize {
    min_stack
        .and_then(|bytes| bytes.parse::<usize>().ok())
        .map_or(SERVER_THREAD_STACK, |bytes| bytes.max(SERVER_THREAD_STACK))
}

/// What the window tells its server thread.
#[derive(Default)]
struct Flags {
    #[cfg(test)]
    fail_tick: AtomicBool,
    /// `Minecraft.pause`: a single-player pause screen is open.
    paused: AtomicBool,
    stop: AtomicBool,
    finished: AtomicBool,
    failure: Mutex<Option<String>>,
    /// `IntegratedServer.getPort`: the port it is open to the LAN on, 0
    /// while it is not.
    published: AtomicU16,
    /// `options.renderDistance` and `options.simulationDistance`, which the
    /// server follows while it runs (IntegratedServer.tickServer).
    render_distance: AtomicI32,
    simulation_distance: AtomicI32,
    /// `options.entityDistanceScaling` (an `f32`'s bits), which
    /// `IntegratedServer.getScaledTrackingDistance` reads on every call.
    entity_distance: AtomicU32,
    /// The PvP page's legacy combat rules, which the server's hits follow from the next tick.
    legacy_rules: AtomicBool,
    /// `MinecraftServer.isCurrentlySaving`: the server is autosaving.
    saving: AtomicBool,
    /// Splitscreen players joined through memory (offline uuids): let in although the world is
    /// not open to the LAN, and the game no longer pauses while they play.
    split_players: Mutex<Vec<u128>>,
    /// The generators of the levels the server has run, by dimension: what
    /// F3's chunk generation entry asks `ServerChunkCache.getGenerator`.
    terrain: Mutex<
        std::collections::HashMap<&'static str, Arc<horizon_client::vanillagen::VanillaTerrain>>,
    >,
    /// What the server's `LevelLoadListener` reports, for the window's
    /// `LevelLoadingScreen` (`LevelLoadTracker.serverProgress/statusView`).
    load: Arc<horizon_client::level_loading::ServerLoadView>,
    /// What the world's upgrade has done (`UpgradeProgress`): the window shows it on
    /// `FileFixerProgressScreen` while the file fixers run, and its Cancel stops them.
    upgrade: Arc<horizon_world::datafix::progress::UpgradeProgress>,
}

impl Flags {
    fn emergency_save(&self, server: std::thread::ThreadId, timeout: Duration) -> bool {
        if std::thread::current().id() == server || self.finished.load(Ordering::Acquire) {
            return true;
        }
        log::warn!("Horizon: saving the world before the crash");
        self.stop.store(true, Ordering::Relaxed);
        let started = Instant::now();
        while !self.finished.load(Ordering::Acquire) {
            let Some(left) = timeout.checked_sub(started.elapsed()) else {
                log::warn!(
                    "Horizon: the world did not save within {} s",
                    timeout.as_secs()
                );
                return false;
            };
            std::thread::sleep(left.min(Duration::from_millis(10)));
        }
        true
    }
}

/// Work the window hands its server thread (`MinecraftServer.execute`).
type Task = Box<dyn FnOnce(&mut Game<'_>) + Send>;

/// The window's handle on its server thread.
pub(super) struct Integrated {
    flags: Arc<Flags>,
    thread: Option<JoinHandle<()>>,
    emergency_save: Option<horizon_client::crash::EmergencySave>,
    tasks: Sender<Task>,
    /// The world's name (`level.dat`'s `LevelName`), for the pause menu.
    pub world_name: String,
    pub save_folder: String,
    pub rule_registry: Arc<horizon_client::gamerules::RuleRegistry>,
}
impl Integrated {
    /// Queue every change, even if the currently published tree still has the requested state:
    /// an earlier update may be waiting for a busy server thread.
    fn queue_pregen_option(&self, enabled: bool) {
        let _ = self.tasks.send(Box::new(move |g| {
            use horizon_client::command::vanilla;
            if g.session.dispatcher.0.find("pregen").is_some() != enabled {
                vanilla::set_horizon_commands(enabled);
                g.session.dispatcher = crate::game::command_text::SessionDispatcher(std::sync::Arc::new(
                    vanilla::session_dispatcher(&g.level.rule_registry, &g.level.enabled_features),
                ));
                g.restore_native_commands();
                g.resend_commands_filtered();
            }
        }));
    }
    /// The generator of the server's level in `dimension`, once it has run.
    pub fn terrain(
        &self,
        dimension: &str,
    ) -> Option<Arc<horizon_client::vanillagen::VanillaTerrain>> {
        self.flags.terrain.lock().ok()?.get(dimension).cloned()
    }
    /// The server's load progress and chunk status map.
    pub fn load_view(&self) -> &Arc<horizon_client::level_loading::ServerLoadView> {
        &self.flags.load
    }
    /// The world's upgrade while the server opens it.
    pub fn upgrade(&self) -> &horizon_world::datafix::progress::UpgradeProgress {
        &self.flags.upgrade
    }
    /// `MinecraftServer.isCurrentlySaving`.
    pub fn is_saving(&self) -> bool {
        self.flags.saving.load(Ordering::Relaxed)
    }
    /// `IntegratedServer.tickServer` skips the world while the game is paused.
    pub fn set_paused(&self, paused: bool) {
        let split = !self.flags.split_players.lock().unwrap().is_empty();
        self.flags.paused.store(paused && !split, Ordering::Relaxed);
    }
    /// Whether splitscreen players are joined: while any is, the game runs even behind a
    /// pause menu, so no window may stop ticking for one (`set_paused`, `pauses_game`).
    pub fn has_split_players(&self) -> bool {
        !self.flags.split_players.lock().unwrap().is_empty()
    }
    /// Splitscreen: `name` left; the game pauses again once every such player has. The owner's
    /// permissions it was given go with it (a LAN player could otherwise join under that name).
    pub fn leave_split(&self, name: &str) {
        let uuid = horizon_protocol::crypto::offline_uuid(name);
        self.flags
            .split_players
            .lock()
            .unwrap()
            .retain(|&u| u != uuid);
        self.call(move |g| {
            if let Some(host) = g.lan.as_mut() {
                host.hub.ops.remove(&uuid);
            }
        });
    }
    /// Splitscreen: a connection through memory for another player on this machine, who gets
    /// the owner's permissions. `None` once the server has stopped.
    pub fn join_split(&self, name: &str) -> Option<horizon_protocol::connection::Pipe> {
        let uuid = horizon_protocol::crypto::offline_uuid(name);
        self.flags.split_players.lock().unwrap().push(uuid);
        let pipe = self
            .call(move |g| {
                let host = g.lan.as_mut()?;
                let owner = host.owner.as_ref()?.uuid;
                let level = host.hub.ops.get(&owner).copied().unwrap_or(0);
                host.hub.ops.insert(uuid, level);
                host.hub.lan.connect_local().ok()
            })
            .flatten();
        if pipe.is_none() {
            self.leave_split(name);
        }
        pipe
    }
    /// The window's render and simulation distances and its Entity Distance
    /// option, for the server to follow.
    pub fn set_distances(&self, render: i32, simulation: i32, entity: f32) {
        self.flags.render_distance.store(render, Ordering::Relaxed);
        self.flags
            .simulation_distance
            .store(simulation, Ordering::Relaxed);
        self.flags
            .entity_distance
            .store(entity.to_bits(), Ordering::Relaxed);
    }
    /// The window's legacy combat rules option, for the server's hits.
    pub fn set_legacy_rules(&self, on: bool) {
        self.flags.legacy_rules.store(on, Ordering::Relaxed);
    }
    /// `IntegratedServer.getPort` while `isPublished`.
    pub fn published(&self) -> Option<u16> {
        Some(self.flags.published.load(Ordering::Relaxed)).filter(|&p| p != 0)
    }
    /// `executeBlocking`: run `f` on the server thread between two ticks and
    /// wait for its answer; `None` once the server has stopped, or when it has
    /// not answered within `CALL_TIMEOUT` (`f` still runs there later). The log
    /// tells the two apart.
    pub fn call<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Game<'_>) -> T + Send + 'static,
    ) -> Option<T> {
        let (tx, rx) = sync_channel(1);
        self.tasks
            .send(Box::new(move |g| {
                let _ = tx.send(f(g));
            }))
            .ok()?;
        match rx.recv_timeout(CALL_TIMEOUT) {
            Ok(answer) => Some(answer),
            Err(RecvTimeoutError::Timeout) => {
                log::warn!(
                    "Horizon: the server thread did not answer within {} s (running: {}, \
                     autosaving: {})",
                    CALL_TIMEOUT.as_secs(),
                    !self.stopped(),
                    self.is_saving()
                );
                None
            }
            Err(RecvTimeoutError::Disconnected) => None,
        }
    }
    /// `execute`: run `f` on the server thread between two ticks, without
    /// waiting for it. False once the server has stopped.
    pub fn post(&self, f: impl FnOnce(&mut Game<'_>) + Send + 'static) -> bool {
        self.tasks.send(Box::new(f)).is_ok()
    }
    /// `IntegratedServer.halt(true)`: returns once the world is saved and let go.
    pub fn stop(mut self) -> Result<()> {
        self.halt()
    }
    /// Stop on the server thread; the window polls completion while drawing.
    pub fn request_stop(&self) {
        self.flags.stop.store(true, Ordering::Relaxed);
    }
    pub fn stopped(&self) -> bool {
        self.thread
            .as_ref()
            .is_none_or(|thread| thread.is_finished())
    }
    fn halt(&mut self) -> Result<()> {
        self.request_stop();
        self.emergency_save.take();
        if let Some(thread) = self.thread.take() {
            thread
                .join()
                .map_err(|_| "the integrated server panicked")?;
        }
        match self
            .flags
            .failure
            .lock()
            .map_err(|_| "server status poisoned")?
            .as_ref()
        {
            Some(error) => Err(error.clone().into()),
            None => Ok(()),
        }
    }
}
impl Drop for Integrated {
    fn drop(&mut self) {
        if let Err(error) = self.halt() {
            log::error!("Horizon: integrated server shutdown: {error}");
        }
    }
}

/// The server thread's side of the window's handle (`Game::serving`).
pub(super) struct Serving {
    flags: Arc<Flags>,
    tasks: Receiver<Task>,
    /// The window's profile.
    pub(super) owner: u128,
    /// Open to LAN or closed from inside a player's command, while the hub
    /// is out of its place: applied after the tick.
    pending: Option<Publish>,
    /// Where the load focus is and how far the player's chunks are.
    load: LoadPhase,
}
/// The part of `LevelLoadListener` after `prepareLevels`: `PrepareSpawnTask`'s
/// `LOAD_PLAYER_CHUNKS`.
#[derive(Default)]
struct LoadPhase {
    /// `updateFocus`: the spawn chunk, once the stage started.
    focus: Option<(i32, i32)>,
    /// `finish(LOAD_PLAYER_CHUNKS)`.
    done: bool,
}
impl Serving {
    /// The window reads this as `MinecraftServer.isCurrentlySaving`.
    pub(super) fn set_saving(&self, saving: bool) {
        self.flags.saving.store(saving, Ordering::Relaxed);
    }
}
enum Publish {
    Open(TcpListener, LanSettings),
    Close,
}

/// What the server thread hands the window once the world is open: its end
/// of the connection and the world's name, or why it failed.
type Ready = std::result::Result<(Pipe, String), String>;

/// An integrated server loading its world: the window keeps drawing its
/// loading screen and joins once the server is ready, as vanilla's client
/// does while its server prepares (`Minecraft.doWorldLoad`), instead of
/// waiting for it frozen for the whole load.
pub(super) struct Starting {
    pub(super) handle: Integrated,
    ready: Receiver<Ready>,
}

impl Game<'_> {

    /// `Minecraft.hasSingleplayerServer`: a world of this window's own, run
    /// in the frame or on its server thread.
    pub(super) fn has_singleplayer_server(&self) -> bool {
        true || self.integrated.is_some()
    }
    /// `Minecraft.isMultiplayerServer`: on another's server, or a world of one's own open to the
    /// network.
    pub(super) fn is_multiplayer_server(&self) -> bool {
        !self.has_singleplayer_server() || self.is_published()
    }
    /// `IntegratedServer.isPublished`: the world is open to the network.
    pub(super) fn is_published(&self) -> bool {
        self.lan.is_some()
            || self
                .integrated
                .as_ref()
                .is_some_and(|s| s.published().is_some())
    }
    /// Splitscreen: players joined through memory play on while one opens a pause menu;
    /// the world keeps ticking for them (`set_paused`), so this window must keep ticking too.
    pub(super) fn has_split_players(&self) -> bool {
        self.integrated
            .as_ref()
            .is_some_and(|s| s.has_split_players())
    }
    /// `MinecraftServer.stopServer`: the players are saved and sent away,
    /// then the world is saved.
    pub(super) fn stop_integrated(&mut self, crashed: bool) -> Result<()> {
        // Abort background terrain generation/decoration on shutdown, matching vanilla behavior.
        self.streaming.abort_generation();
        // The close request: lifecycle observation stops.
        self.native_lifecycle.begin_close();
        self.native_configs_server_stop();
        self.native_players_leaving();
        // IntegratedServer.stopServer: teardownPublishedState.
        self.stop_serving();
        // PlayerList.removeAll: the owner and every guest leave the game.
        if !crashed {
            self.award_leave_game_to_all();
            self.save_leaving_guests();
        }
        if let Some(host) = self.lan.as_mut() {
            let conns: Vec<u32> = host.hub.guests.iter().map(|g| g.conn).collect();
            for conn in conns {
                host.hub.kick(
                    conn,
                    translatable("multiplayer.disconnect.server_shutdown", &[]),
                );
            }
            let has_remote_guests = host.hub.guests.iter().any(|g| g.conn != 0);
            if has_remote_guests {
                std::thread::sleep(Duration::from_millis(50));
            }
        }
        self.finish_server_save(crashed)
    }
}

/// The server thread's side.
impl Game<'_> {
    /// IntegratedServer.teardownPublishedState, also reached when a shutdown
    /// callback unwinds before the normal stop path can retire the server.
    pub(in crate::game) fn stop_serving(&mut self) {
        if let Some(s) = self.serving.take() {
            s.flags.published.store(0, Ordering::Relaxed);
        }
        // What a mod switched on for this server (packet counting, mob sampling) ends with it.
        self.stop_mod_diagnostics();
    }

    /// Whether this game is an integrated server's.
    pub(super) fn serving(&self) -> bool {
        self.serving.is_some()
    }
    /// The window's agent runs a command as single player's console does
    /// (`as_console`, as without a server thread): as the world's owner, in
    /// the owner's level, at the owner permission level. The feedback goes to
    /// the owner's chat and is returned.
    pub(super) fn owner_console_command(&mut self, command: &str) -> Vec<String> {
        // Test-only, as on the dedicated console: sync the randoms on this server thread, in the owner's level.
        #[cfg(feature = "state-digest")]
        if let Some(rest) = command.trim_start_matches('/').strip_prefix("digest-sync")
            && (rest.is_empty() || rest.starts_with(char::is_whitespace))
        {
            let args: Vec<&str> = rest.split_whitespace().collect();
            // The owner's level is the one to sync. It must be the level in place: syncing any other one, or
            // the level in place after a failed switch, would report success for the wrong level.
            let owner = self.serving.as_ref().map(|s| s.owner);
            let owner_dim = self.lan.as_ref().and_then(|h| {
                h.hub.guests.iter().find(|g| Some(g.uuid) == owner).map(|g| g.dimension.clone())
            });
            let Some(owner_dim) = owner_dim else {
                return vec!["digest-sync: the owner is not in the world yet".to_owned()];
            };
            if super::Dim::from_id(&owner_dim) != self.dim {
                return vec![format!(
                    "digest-sync: the owner is in {owner_dim}, not the level in place; nothing changed"
                )];
            }
            return vec![match self.state_digest_sync(&args) {
                Ok(()) => {
                    self.state_digest_rank_joins();
                    "digest-sync done".to_owned()
                }
                Err(e) => format!("digest-sync: {e}"),
            }];
        }
        let Some(owner) = self.serving.as_ref().map(|s| s.owner) else {
            return Vec::new();
        };
        let Some(host) = self.lan.as_ref() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        if let Some(g) = host.hub.guests.iter().find(|g| g.uuid == owner && g.placed) {
            let conn = g.conn;
            // Commands.performCommand runs in the player's level.
            let (back, theirs) = (self.dim, super::Dim::from_id(&g.dimension));
            let entered = theirs == back || self.enter_level(theirs).is_ok();
            if entered && let Some(swap) = self.enter_guest_in_dimension(conn, back) {
                let command = command.strip_prefix('/').unwrap_or(command).to_owned();
                let lines = self.as_console(|g| {
                    g.chat_capture = Some(Vec::new());
                    // A command a native mod registered goes to the mods
                    // first, as the owner's own chat does (the console is
                    // level 4, so every registered usage is allowed).
                    if !g.perform_native_command(conn, 4, command.trim()) {
                        g.run_command(&command);
                    }
                    g.chat_capture.take().unwrap_or_default()
                });
                self.leave_guest(swap);
                if let Some(host) = self.lan.as_mut() {
                    host.hub.sync_all_effects();
                }
                // A teleport into another level happens before its feedback is
                // sent, and what the command earned is announced before it too.
                self.run_level_teleports();
                self.announce_guest_advancements();
                for line in lines {
                    out.push(line.plain());
                    if let Some(host) = self.lan.as_ref() {
                        host.hub.send_to(
                            conn,
                            &horizon_protocol::packets::play::Clientbound::SystemChat(
                                horizon_protocol::packets::play::SystemChat {
                                    content: horizon_client::lan::data::component_nbt(
                                        &line.component(),
                                    ),
                                    overlay: false,
                                },
                            ),
                        );
                    }
                }
                self.send_admin_feedback(Some(conn));
            }
            if theirs != back
                && let Err(e) = self.enter_level(back)
            {
                log::error!("could not reopen {}: {e}", back.id());
            }
        }
        out
    }

    /// Run `f` with the owner's level in place (the server holds one per
    /// dimension), then put back the level that was.
    pub(super) fn in_owner_level<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        let owner = self.serving.as_ref().map(|s| s.owner);
        let dim = self.lan.as_ref().and_then(|h| {
            let g = h.hub.guests.iter().find(|g| Some(g.uuid) == owner)?;
            Some(super::Dim::from_id(&g.dimension))
        });
        let back = dim.and_then(|d| self.enter_command_level(d));
        let out = f(self);
        if let Some(back) = back {
            self.enter_command_level(back);
        }
        out
    }

    /// The window's requests, between two ticks.
    fn run_tasks(&mut self) {
        let tasks: Vec<Task> = match &self.serving {
            Some(s) => s.tasks.try_iter().collect(),
            None => return,
        };
        for task in tasks {
            task(self);
            self.apply_publish();
        }
    }
    /// `IntegratedServer.getPort` while published.
    pub(super) fn published_port(&self) -> Option<u16> {
        let s = self.serving.as_ref()?;
        Some(s.flags.published.load(Ordering::Relaxed)).filter(|&p| p != 0)
    }
    /// `IntegratedServer.publishServer`: listen on `settings.port` (0 picks
    /// one) and announce the world. The socket is bound at once so a busy
    /// port fails here; the hub starts listening once it is at hand.
    pub(super) fn publish(&mut self, settings: LanSettings) -> std::result::Result<u16, String> {
        if self.options.developer_single_player {
            return Err(horizon_client::lang::get(
                "horizon.developer_offline.single_player",
            ));
        }
        let Some(s) = self.serving.as_mut() else {
            return Err("not an integrated server".into());
        };
        if s.flags.published.load(Ordering::Relaxed) != 0 {
            return Err("already published".into());
        }
        let listener = Lan::bind(None, settings.port).map_err(|e| e.to_string())?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        log::info!("Published LAN server on port {port}");
        s.flags.published.store(port, Ordering::Relaxed);
        s.pending = Some(Publish::Open(listener, LanSettings { port, ..settings }));
        self.apply_publish();
        Ok(port)
    }
    /// `IntegratedServer.unpublishServer`: stop listening and send every
    /// player but the owner away. False when it was not published.
    pub(super) fn unpublish(&mut self) -> bool {
        let Some(s) = self.serving.as_mut() else {
            return false;
        };
        if s.flags.published.swap(0, Ordering::Relaxed) == 0 {
            return false;
        }
        s.pending = Some(Publish::Close);
        self.apply_publish();
        true
    }
    /// IntegratedServer.tickServer: the server follows the window's render
    /// and simulation distances when they change (and Horizon's legacy combat rules).
    fn follow_distances(&mut self, flags: &Flags) {
        // IntegratedServer.getScaledTrackingDistance reads the option each time.
        self.settings.entity_distance =
            f32::from_bits(flags.entity_distance.load(Ordering::Relaxed));
        self.set_pvp_legacy_rules(flags.legacy_rules.load(Ordering::Relaxed));
        let Some(host) = self.lan.as_mut() else {
            return;
        };
        let view = flags.render_distance.load(Ordering::Relaxed).max(2);
        let old = host.hub.config.view_distance;
        if view != old {
            log::info!("Changing view distance to {view}, from {old}");
            host.hub.set_view_distance(view);
            self.settings.render_distance = view;
        }
        let simulation = flags.simulation_distance.load(Ordering::Relaxed).max(2);
        let old = host.hub.config.simulation_distance;
        if simulation != old {
            log::info!("Changing simulation distance to {simulation}, from {old}");
            host.hub.set_simulation_distance(simulation);
            self.settings.simulation_distance = simulation;
        }
    }

    fn apply_publish(&mut self) {
        if self.lan.is_none() {
            return;
        }
        let Some(s) = self.serving.as_mut() else {
            return;
        };
        let (Some(pending), owner) = (s.pending.take(), s.owner) else {
            return;
        };
        let allow_commands = self.allow_commands();
        let Some(host) = self.lan.as_mut() else {
            return;
        };
        match pending {
            Publish::Open(listener, settings) => {
                if let Err(e) = host.hub.lan.listen_on(listener, true) {
                    log::error!("Horizon: Open to LAN: {e}");
                }
                host.settings = settings;
                host.hub.config.force_game_mode = settings.force_game_mode;
                // IntegratedServer.setGuestCommandAccess, then updatePermissions.
                host.hub.config.guest_commands = settings.guest_commands && allow_commands;
                host.hub.resend_commands();
            }
            Publish::Close => {
                host.hub.lan.stop_listening();
                log::info!(
                    "Unpublishing integrated server (was on port {})",
                    host.settings.port
                );
                let others: Vec<u32> = host
                    .hub
                    .guests
                    .iter()
                    .filter(|g| g.uuid != owner)
                    .map(|g| g.conn)
                    .collect();
                for conn in others {
                    host.hub.kick(
                        conn,
                        translatable("multiplayer.disconnect.server_shutdown", &[]),
                    );
                }
            }
        }
    }
    /// `IntegratedServer.setPersonalGameType`: the owner's game mode.
    pub(super) fn set_owner_mode(&mut self, mode: i32) {
        let Some(owner) = self.serving.as_ref().map(|s| s.owner) else {
            return;
        };
        if let Some(host) = self.lan.as_mut()
            && let Some(conn) = host
                .hub
                .guests
                .iter()
                .find(|g| g.uuid == owner)
                .map(|g| g.conn)
        {
            host.hub
                .set_game_mode(conn, mode, &self.world, Some(self.collision));
        }
    }
    /// `IntegratedServer.setWorldAllowCommands`: the owner is an operator
    /// while the world allows commands; everyone's level is sent again.
    pub(super) fn owner_allow_commands(&mut self, allow: bool) {
        let Some((owner, flags)) = self.serving.as_ref().map(|s| (s.owner, s.flags.clone())) else {
            return;
        };
        let published = self.published_port().is_some();
        if let Some(host) = self.lan.as_mut() {
            let level = if allow { 4 } else { 0 };
            host.hub.ops.insert(owner, level);
            // Splitscreen seats have the owner's permissions (`join_split`): they follow it.
            for &seat in flags.split_players.lock().unwrap().iter() {
                host.hub.ops.insert(seat, level);
            }
            if published {
                host.hub.config.guest_commands = allow && host.settings.guest_commands;
            }
            host.hub.resend_commands();
        }
    }
}

/// Component oracles supply their own synthetic levels and entity phases.
/// They still execute on a server Game, never the client's world.
pub(super) enum ComponentHarness {
    Sculk(String),
    /// The hot-path benchmark (`hot_paths`), which builds its own levels the same way.
    HotPaths(String),
}

impl Game<'_> {
    pub(super) fn run_component_harness(&self, fixture: ComponentHarness) -> Result<()> {
        let (res, pack, settings) = (self.res.clone(), self.pack.clone(), self.settings.clone());
        let mut options = self.options.clone();
        options.execution_role = ExecutionRole::IntegratedServer;
        options.dedicated = Some(Vec::new());
        let result = std::thread::Builder::new()
            .name(horizon_client::crash::INTEGRATED_SERVER_THREAD.into())
            .stack_size(server_thread_stack())
            .spawn(move || {
                let run = || -> Result<()> {
                    let mut server = server_game(&res, pack, options, settings)?;
                    // The oracle supplies an already-open synthetic world rather
                    // than running the normal config and startup lifecycle.
                    server
                        .mod_phase
                        .advance(mod_api::__private::ModPhase::Config)?;
                    server
                        .mod_phase
                        .advance(mod_api::__private::ModPhase::World)?;
                    match fixture {
                        ComponentHarness::Sculk(spec) => server.sculk_fixture(&spec),
                        ComponentHarness::HotPaths(output) => server.hot_paths(&output),
                    }
                };
                run().map_err(|e| e.to_string())
            })?
            .join()
            .map_err(|_| "the component harness server panicked")?;
        result.map_err(Into::into)
    }
}

fn server_game(
    res: &Arc<Resources>,
    pack: PackResources,
    options: Options,
    settings: Settings,
) -> Result<Game<'_>> {
    let audio = Audio::default();
    Game::new(res, pack, options, settings, audio)
}

impl Game<'_> {
    /// The server thread has already delivered its report: written to disk, or printed
    /// when it couldn't be written. The flag is set either way, since a dead server must
    /// not keep the game open. Join through the normal halt path, so a tick-loop crash
    /// finishes the server's own stop and save first, then exit without returning an
    /// error that would create another report. A panic inside a posted task happens
    /// outside the tick loop's catch, so the join doesn't promise a save there.
    pub(super) fn exit_after_server_crash(&mut self) {
        // Only the owner ends the process, as in `idle`: a splitscreen guest owns no
        // server, and exiting from its pass would skip the owner's halt.
        if self.options.split_guest || !horizon_client::crash::server_crashed() {
            return;
        }
        // A crash while the world is still loading leaves the server in `integrated_starting`.
        let starting = self
            .integrated_starting
            .take()
            .map(|starting| starting.handle);
        for mut server in self.integrated.take().into_iter().chain(starting) {
            if let Err(error) = server.halt() {
                log::error!("Horizon: crashed integrated server: {error}");
            }
        }
        std::process::exit(101);
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use mod_api::prelude::*;

    #[test]
    #[ignore = "daily"]
    fn posted_native_panic_writes_one_server_report() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().to_path_buf();
        let previous = std::panic::take_hook();
        horizon_client::crash::install(home.clone(), "client");
        let (tasks, receiver) = channel::<Task>();
        let mut server = Integrated {
            emergency_save: None,
            flags: Arc::new(Flags::default()),
            thread: None,
            tasks,
            world_name: String::new(),
            save_folder: String::new(),
            rule_registry: Arc::new(horizon_client::gamerules::RuleRegistry::vanilla()),
        };
        assert!(server.post(|_| {
            let _guard = horizon_client::crash::enter_native_mod(horizon_client::crash::ModCall {
                id: "example:posted",
                hook: "server_tick",
                part: None,
            });
            panic!("posted integrated panic 3494");
        }));
        server.thread = Some(
            std::thread::Builder::new()
                .name(horizon_client::crash::INTEGRATED_SERVER_THREAD.into())
                .stack_size(server_thread_stack())
                .spawn(move || {
                    crate::game::native_entity_refs::tests::with_game(|game| {
                        receiver.recv().unwrap()(game);
                    });
                })
                .unwrap(),
        );
        assert_eq!(
            server.halt().unwrap_err().to_string(),
            "the integrated server panicked"
        );
        std::panic::set_hook(previous);
        let reports: Vec<_> = std::fs::read_dir(home.join("crash-reports"))
            .unwrap()
            .map(|p| p.unwrap().path())
            .collect();
        assert_eq!(reports.len(), 1);
        // `MinecraftServer.runServer` saves a server report even under a client install.
        let file = reports[0].file_name().unwrap().to_string_lossy();
        assert!(file.ends_with("-server.txt"), "{file}");
        let text = std::fs::read_to_string(&reports[0]).unwrap();
        assert!(text.contains("Description: Exception in server tick loop"));
        assert!(text.contains("Server thread"));
        assert!(text.contains("posted integrated panic 3494"));
        assert!(text.contains("in native mod example:posted, hook server_tick"));
    }

    /// The server thread keeps guest headroom, and a larger `RUST_MIN_STACK` still
    /// reaches it (arismoko/horizon#2430).
    #[test]
    fn the_server_thread_stack_keeps_guest_headroom_and_larger_overrides() {
        let minimum = 136 * 1024 * 1024;
        assert_eq!(super::stack_for(None), minimum);
        assert_eq!(
            super::stack_for(Some("1048576")),
            minimum,
            "smaller asks keep guest headroom"
        );
        assert_eq!(super::stack_for(Some("268435456")), 256 * 1024 * 1024);
        assert_eq!(super::stack_for(Some("16777216")), minimum);
        assert_eq!(
            super::stack_for(Some("lots")),
            minimum,
            "an unparsed value is ignored"
        );
    }

    /// Observe both teardownPublishedState and the dropped server task queue.
    pub(in crate::game) fn install_serving(
        game: &mut Game<'_>,
        published: bool,
    ) -> impl Fn() -> bool + Send + Sync + 'static {
        let flags = Arc::new(Flags::default());
        flags
            .published
            .store(u16::from(published), Ordering::Relaxed);
        let (tasks, receiver) = channel();
        game.serving = Some(Serving {
            flags: flags.clone(),
            tasks: receiver,
            owner: 0,
            pending: None,
            load: LoadPhase::default(),
        });
        move || {
            flags.published.load(Ordering::Relaxed) == 0 && tasks.send(Box::new(|_| {})).is_err()
        }
    }

    #[test]
    fn pregen_toggle_back_is_queued_while_server_is_busy() {
        let (tasks, waiting_server) = channel();
        let server = Integrated {
            emergency_save: None,
            flags: Arc::new(Flags::default()),
            thread: None,
            tasks,
            world_name: String::new(),
            save_folder: String::new(),
            rule_registry: Arc::new(horizon_client::gamerules::RuleRegistry::vanilla()),
        };
        // The receiver deliberately does not run either task between the updates. In particular,
        // the dispatcher can still show the old value when the owner toggles back.
        server.queue_pregen_option(true);
        server.queue_pregen_option(false);
        assert!(
            waiting_server.try_recv().is_ok(),
            "enable must reach the server"
        );
        assert!(waiting_server.try_recv().is_ok(), "disable must follow it");
        assert!(waiting_server.try_recv().is_err());
    }

    #[test]
    fn shutdown_request_does_not_join_a_server_still_saving() {
        let flags = Arc::new(Flags::default());
        let (release, wait) = channel();
        let (tasks, _) = channel();
        let server = Integrated {
            emergency_save: None,
            flags: flags.clone(),
            thread: Some(std::thread::spawn(move || wait.recv().unwrap())),
            tasks,
            world_name: String::new(),
            save_folder: String::new(),
            rule_registry: Arc::new(horizon_client::gamerules::RuleRegistry::vanilla()),
        };
        server.request_stop();
        assert!(flags.stop.load(Ordering::Relaxed));
        assert!(!server.stopped());
        release.send(()).unwrap();
        server.stop().unwrap();
    }

    #[test]
    fn shutdown_reports_a_failed_server_save() {
        let flags = Arc::new(Flags::default());
        let saving = flags.clone();
        let (tasks, _) = channel();
        let server = Integrated {
            emergency_save: None,
            flags,
            thread: Some(std::thread::spawn(move || {
                *saving.failure.lock().unwrap() = Some("disk write failed".into());
            })),
            tasks,
            world_name: String::new(),
            save_folder: String::new(),
            rule_registry: Arc::new(horizon_client::gamerules::RuleRegistry::vanilla()),
        };
        assert_eq!(server.stop().unwrap_err().to_string(), "disk write failed");
    }
}
