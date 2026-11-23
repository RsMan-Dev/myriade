//! The startup's resource load: the stages the loading screen shows, the
//! scoped threads each step runs on, and the core, gameplay and pack tables
//! read from the prepared resources.
use super::*;

pub(super) const STAGE_LINES: [&str; 10] = [
    "Loading menu",
    "Loading assets",
    "Loading block facts",
    "Loading biomes",
    "Loading pack tables",
    "Loading collision",
    "Loading items",
    "Loading recipes",
    "Loading world resources",
    "World resources ready",
];

/// The background loader's checkpoint, read by the loading screen each frame.
#[derive(Clone)]
pub(super) struct Progress {
    pub(super) value: Arc<AtomicU32>,
    pub(super) lines: &'static [&'static str],
}
impl Progress {
    pub(super) fn new() -> Self {
        Self::with(&STAGE_LINES)
    }
    pub(super) fn with(lines: &'static [&'static str]) -> Self {
        Self {
            value: Arc::new(AtomicU32::new(0)),
            lines,
        }
    }
    pub(super) fn set(&self, stage: u32) {
        self.value.store(stage, Ordering::Relaxed);
    }
}

/// Harness and dedicated runs keep the fully-synchronous startup so their
/// output stays byte-identical; only an ordinary launch shows the window first.
pub(super) fn synchronous_startup(options: &Options) -> bool {
    options.control.is_some()
        || true
        || std::env::var_os("HORIZON_SCREEN_BUILD_FIXTURE").is_some()
        || std::env::var_os("HORIZON_DISPENSE_FIXTURE").is_some()
        || std::env::var_os("HORIZON_SCULK_FIXTURE").is_some()
        || std::env::var_os("HORIZON_HOT_PATHS").is_some()
        || std::env::var_os("HORIZON_NET_REPORT").is_some()
}

/// Whether this run owns the player's device-loss marker (`gfx::safe_start`): a visible client
/// window an ordinary launch started. Scripted, offscreen, server and split-screen guest runs
/// sharing the folder neither write it nor take it.
pub(super) fn owns_device_loss_marker(options: &Options) -> bool {
    !synchronous_startup(options)
}

/// The local world a client's launch opens at once (`--world`,
/// `--quickPlaySingleplayer`), whose generator data can load with the game's.
pub(super) fn launch_world(_options: &Options) -> Option<&str> {
    None
}

/// A scoped thread with a name, for one step of the resource load.
///
/// Named because the heap census and the startup trace both fall back to the
/// OS thread name: unnamed, every loader's retained memory and every span it
/// ran landed in one `unnamed thread` bucket. That bucket was 330 MiB of a
/// playing game's resident set -- the largest of all of them -- with nothing
/// to say which loader held it, and 13 of the trace's 17 lanes were
/// indistinguishable.
pub(super) fn named_scope<'scope, T: Send + 'scope>(
    scope: &'scope std::thread::Scope<'scope, '_>,
    name: &str,
    f: impl FnOnce() -> T + Send + 'scope,
) -> std::thread::ScopedJoinHandle<'scope, T> {
    horizon_guest_stack::builder()
        // Linux keeps 15 bytes of a thread name: a short prefix keeps them apart
        // ("horizon-load-core" and "horizon-load-collision" would both read
        // "horizon-load-co").
        .name(format!("load-{name}"))
        .spawn_scoped(scope, f)
        // `Scope::spawn`, which this replaces, panics when the OS refuses a
        // thread. Keeping that: a resource load cannot go on without it.
        .expect("resource loader thread")
}

/// A step of the resource load: on its own scoped thread, or, when the
/// settings allow a single worker, run where it is joined.
pub(super) enum Step<'scope, T> {
    Thread(std::thread::ScopedJoinHandle<'scope, T>),
    Later(Box<dyn FnOnce() -> T + Send + 'scope>),
}
impl<'scope, T: Send + 'scope> Step<'scope, T> {
    /// `name` names the thread when one is spawned ([`named_scope`]).
    pub(super) fn spawn(
        scope: &'scope std::thread::Scope<'scope, '_>,
        parallel: bool,
        name: &str,
        f: impl FnOnce() -> T + Send + 'scope,
    ) -> Self {
        if parallel {
            Self::Thread(named_scope(scope, name, f))
        } else {
            Self::Later(Box::new(f))
        }
    }
    pub(super) fn join(self) -> T {
        match self {
            Self::Thread(handle) => handle.join().unwrap(),
            Self::Later(f) => f(),
        }
    }
}

/// The world bundle's core tables from one resource root, before
/// `load_resources` binds them into the pack tables and `WorldCore`.
pub(super) struct CoreTables {
    pub(super) item_data: Arc<horizon_world::item_defaults::ItemData>,
    pub(super) item_defaults: Arc<horizon_world::item_defaults::ItemDefaults>,
    pub(super) assets: Assets,
    pub(super) gameplay: GameplayTables,
    pub(super) biomes: Arc<Biomes>,
    pub(super) environment: Environment,
    pub(super) collision: Arc<BlockCollisionRegistry<'static>>,
}
/// The block facts and the tables read from them.
pub(super) struct GameplayTables {
    pub(super) behaviour_json: serde_json::Value,
    pub(super) facts: BlockFacts,
    pub(super) gameplay_projection: Vec<myriade_engine::blocks::GameplayProjection>,
    pub(super) light: LightTables,
    pub(super) motion: MotionResources,
}

/// [`CoreTables`] at `root`. Every table reads the block registry; past it,
/// the block models and sprites (then the item defaults), the face visibility,
/// the collisions, the biomes and the environment need no block facts and load
/// beside them. The outer error fails the load; the inner one is the packs'
/// (the registry or the block models), which `load_resources` rolls back from.
pub(super) fn load_core(
    resources: &horizon_assets::resource_source::ResourceRoot,
    additional_data: &[Arc<myriade_engine::datapack::DataPack>],
    load_role: mods::LoadRole,
    parallel: bool,
    progress: &Progress,
    launch_world: Option<&str>,
) -> std::result::Result<std::result::Result<CoreTables, String>, String> {
    let role = load_role.execution;
    let root = resources.prepared();
    let assets = startup::time("core.assets", || Assets::load(root));
    let assets = match assets {
        Ok(assets) => assets,
        Err(e) => return Ok(Err(e.to_string())),
    };
    progress.set(1);
    let item_data =
        Arc::new(mods::load_item_data(root, role, load_role.owning).map_err(|e| e.to_string())?);
    let registry = assets.registry;
    let (item_defaults, gameplay, biomes, environment, collision) = std::thread::scope(
        |scope| {
            let assets = &assets;
            let item_data = &item_data;
            let item_defaults = Step::spawn(scope, parallel, "item_defaults", move || {
                startup::time("core.item_defaults", || horizon_world::item_defaults::shared(root))
                    .map_err(|e| e.to_string())
            });
            let biomes = Step::spawn(scope, parallel, "biomes", || {
                let biomes = Arc::new(
                    startup::time("core.biomes", || {
                        Biomes::load_for_server(resources)
                    })
                    .map_err(|e| e.to_string())?,
                );
                // A launch that names a world: its data preload needs only the
                // block registry and the biomes, so it starts here rather than
                // after the block models and facts (~250 ms later).
                if let Some(folder) = launch_world {
                    world_loading::preload_launch_world(
                        folder,
                        root,
                        resources.data_changes(),
                        registry,
                        biomes.clone(),
                        item_data.clone(),
                        additional_data,
                    );
                }
                Ok::<_, String>(biomes)
            });
            let environment = Step::spawn(scope, parallel, "environment", || {
                startup::time("core.environment", || Environment::load(resources.source()))
                    .map_err(|e| e.to_string())
            });
            let collision = Step::spawn(scope, parallel, "collision", || {
                startup::time("core.collision", || collisions::load(root, registry))
                    .map_err(|e| e.to_string())
            });
            let gameplay = gameplay_tables(root, assets, parallel, progress);
            (
                item_defaults.join(),
                gameplay,
                biomes.join(),
                environment.join(),
                collision.join(),
            )
        },
    );
    // The packs' failure first: it rolls back whatever else failed.
    let item_defaults = match item_defaults {
        Ok(defaults) => defaults,
        Err(e) => return Ok(Err(e)),
    };
    let gameplay = gameplay?;
    progress.set(3);
    Ok(Ok(CoreTables {
        item_data,
        item_defaults,
        gameplay,
        biomes: biomes?,
        environment: environment?,
        collision: Arc::new(collision?),
        assets,
    }))
}
/// The block facts, then the light tables beside the motion tables, which
/// both read them.
pub(super) fn gameplay_tables(
    root: &std::path::Path,
    assets: &Assets,
    parallel: bool,
    progress: &Progress,
) -> std::result::Result<GameplayTables, String> {
    let facts_step = startup::step("core.block_facts");
    // Read whole, then parsed from the buffer: `serde_json::from_reader`
    // pulls through a `BufReader` and is measurably slower on a file this
    // size than one read plus `from_slice`, which is how `BlockFacts` next
    // door already reads its own. Same value either way.
    let behaviour_json: serde_json::Value =
        horizon_world::json_file::read(&root.join("block-behaviours.json"))
            .map_err(|e| e.to_string())?;
    let (facts, gameplay_projection) =
        BlockFacts::load_with_projection(root, assets.registry, &behaviour_json, parallel)
            .map_err(|e| e.to_string())?;
    drop(facts_step);
    progress.set(2);
    let (light, motion) = std::thread::scope(|scope| {
        let light = Step::spawn(scope, parallel, "light", || {
            startup::time("core.light", || {
                LightTables::load(root, assets.registry, &facts)
            })
            .map_err(|e| e.to_string())
        });
        let motion = startup::time("core.motion", || {
            let conditions = horizon_world::conditions::LoadConditions::vanilla();
            let motion =
                MotionResources::load_with_conditions(root, assets, &facts, &conditions)?;
            Ok::<_, Box<dyn std::error::Error>>(motion)
        })
        .map_err(|e| e.to_string());
        (light.join(), motion)
    });
    Ok(GameplayTables {
        light: light?,
        motion: motion?,
        behaviour_json,
        facts,
        gameplay_projection,
    })
}

/// The core bundle and the world's rest, loaded here; a failed pack starts
/// again with the game's own resources (Minecraft.rollbackResourcePacks).
/// `launch_world`: the world the launch opens, whose data is preloaded meanwhile.
pub(super) fn load_resources(
    mut resolved: horizon_assets::resource_source::ResourceRoot,
    additional_data: Vec<Arc<myriade_engine::datapack::DataPack>>,
    launch_world: Option<&str>,
    vanilla_root: &std::path::Path,
    _settings: &Settings,
    started: Instant,
    role: impl Into<mods::LoadRole>,
    progress: &Progress,
) -> Result<Loaded> {
    let load_role: mods::LoadRole = role.into();
    let _role = load_role.execution;
    let mut root = resolved.prepared().to_path_buf();
    let threads = horizon_threads::sizing::for_pool_with(false, horizon_threads::sizing::Pool::Jobs);
    // One pool of workers for every level's streaming, the client's and the
    // server thread's (horizon_threads::jobs), sized before any world opens.
    horizon_threads::jobs::configure(threads);
    // The core bundle loads on workers. A pack failure starts it again with the
    // game's own resources.
    let parallel = threads > 1;
    let mut can_rollback = true;
    let (core, resolved) = loop {
        let core = std::thread::scope(|scope| {
            let core = Step::spawn(scope, parallel, "core", || {
                // A failed pack starts core again from the game's own
                // resources, which the world's preload must read: with packs
                // it waits for the core that loaded.
                let early = launch_world.filter(|_| root == vanilla_root);
                startup::time("core", || {
                    load_core(
                        &resolved,
                        &additional_data,
                        load_role,
                        parallel,
                        progress,
                        early,
                    )
                })
            });
            core.join()
        });
        let failure = match core {
            Ok(Ok(core)) => break (core, resolved),
            Ok(Err(e)) if can_rollback => e,
            Ok(Err(e)) => {
                return Err(format!(
                    "{e}\nPrepare resources with the launcher, or: horizon-prepare --prepare"
                )
                .into());
            }
            Err(e) => return Err(e.into()),
        };
        log::warn!(
            "Horizon: resource packs failed to load ({failure}); using the default resources"
        );
        can_rollback = false;
        root = vanilla_root.to_path_buf();
        resolved = horizon_assets::resource_source::ResourceRoot::directory(root.clone());
    };
    // Every loaded game publishes scalar registry keys, including callers that
    // load a prepared root directly instead of building a mod content overlay.
    crate::game::mods::register_iterated_registries(resolved.prepared())?;
    log::info!("load core tables: {:.2}s", started.elapsed().as_secs_f32());
    let CoreTables {
        item_data,
        item_defaults: _,
        assets,
        gameplay:
            GameplayTables {
                behaviour_json,
                facts,
                gameplay_projection,
                light,
                motion,
            },
        biomes,
        environment,
        collision,
    } = core;
    let assets = Arc::new(assets);
    // The namespaces of the mod block states registered now: what every level.dat saved from
    // here on records, and what a world is checked against before it opens (`level_mods`). A
    // launch world's data preload, which may already have started, reads no mod records; its
    // open, which checks them, waits for these resources.
    {
        use myriade_engine::save::level_mods;
        let registry = assets.registry;
        let names = (0..registry.len() as u32)
            .filter_map(|id| registry.state(horizon_world::registry::BlockStateId(id)))
            .map(|state| state.name.as_str());
        level_mods::set_block_namespaces(Some(level_mods::mod_namespaces(names)));
    }
    if let Some(folder) = launch_world {
        world_loading::preload_launch_world(
            folder,
            &root,
            resolved.data_changes(),
            assets.registry,
            biomes.clone(),
            item_data.clone(),
            &additional_data,
        );
    }
    api_values::resolve_block_definitions(assets.registry)?;
    mods::extend_poi_types(assets.registry);
    mods::register_cauldron_fluid_contents(assets.registry);
    let pack_step = startup::step("pack.assemble");
    let mut pack = PackResources::assemble(
        &resolved,
        assets.clone(),
        biomes.clone(),
        Arc::new(light),
    );
    pack.additional_data = additional_data.into();
    drop(pack_step);
    log::info!("load pack tables: {:.2}s", started.elapsed().as_secs_f32());
    progress.set(4);
    myriade_engine::lang::set(myriade_engine::lang::Language::load(
        &[&pack.source],
        myriade_engine::lang::DEFAULT,
    ));
    progress.set(5);
    let suffocation =
        myriade_engine::mobs::suffocation::bind(collision.clone(), &motion.causes_suffocation)?;
    let resources = Resources {
        core: WorldCore {
            item_data,
            facts,
            biomes: biomes.clone(),
            environment,
            collision,
            motion,
            suffocation,
            spawn_registries: myriade_engine::mobs::finalize::SpawnRegistries::load(&root),
            threads,
        },
        rest: std::sync::OnceLock::new(),
        block_entity_reach: std::sync::OnceLock::new(),
    };
    let resources = Arc::new(resources);
    let rest = world_rest::load_world_rest(
        &resolved,
        &resources,
        &assets,
        &behaviour_json,
        &gameplay_projection,
        progress,
        started,
    )?;
    resources
        .rest
        .set(rest)
        .map_err(|_| "world resources already loaded")?;
    write_prepared_files(vanilla_root.to_path_buf());
    Ok(Loaded {
        root,
        resources,
        pack,
    })
}

/// Write the prepared folder's `horizon-data.bin` when it has none the game
/// can use: on a thread of its own, at idle priority, once the start is done.
/// This start has just read the folder's files (they are in the disk cache);
/// the next starts read them from one file (arismoko/horizon#1961).
pub(super) fn write_prepared_files(root: PathBuf) {
    use horizon_core::prepared_files;
    if prepared_files::open(&root).is_some() || !root.join("horizon-resources.json").is_file() {
        return;
    }
    let spawned = horizon_guest_stack::builder()
        .name("Prepared data file".into())
        .spawn(move || {
            horizon_threads::idle_current();
            let started = Instant::now();
            match prepared_files::write(&root) {
                Ok(()) => log::info!(
                    "Horizon: wrote {} in {} ms",
                    prepared_files::FILE_NAME,
                    started.elapsed().as_millis()
                ),
                Err(e) => log::info!("Horizon: {} not written: {e}", prepared_files::FILE_NAME),
            }
        });
    if let Err(e) = spawned {
        log::info!(
            "Horizon: {} not written: {e}",
            horizon_core::prepared_files::FILE_NAME
        );
    }
}
