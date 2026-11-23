//! Startup shared by the gameplay library and its executable.
use crate::{crash, game, logging};
pub(crate) mod dedicated_arguments;

/// Validated startup state. Nothing runs before prepare returns.
pub struct Startup {
    options: game::Options,
}

/// Reads and validates the command line. A bad one exits with the usage code.
pub fn prepare() -> Startup {
    let options = game::Options::validate(std::env::args().skip(1)).unwrap_or_else(|error| {
        eprintln!("Horizon: {error}");
        std::process::exit(game::exit_code::USAGE);
    });
    let Some(options) = options else {
        std::process::exit(0);
    };
    Startup { options }
}

/// Start the engine.
pub fn start(startup: Startup) {
    let Startup { options } = startup;
    if let Some(home) = options.home_dir() {
        logging::set_game_home(home);
    }
    let home = options
        .game_dir()
        .map_or_else(std::env::current_dir, Ok)
        .unwrap_or_else(|error| {
            eprintln!("Horizon: {error}");
            std::process::exit(game::exit_code::USAGE);
        });
    horizon_threads::configure_allocator();
    horizon_chat_filter::install();
    // Crates above the engine registered their hooks before this (horizon-official's `main`),
    // then the Horizon parts still in this crate; none registers after this point.
    horizon_hooks::engine::seal();
    // The text crates ship (`horizon_hooks::lang`), loaded with the game's own languages.
    myriade_engine::lang::install_crate_files(horizon_hooks::lang::engine::all());
    myriade_engine::spans::startup::init(std::time::Instant::now());
    // The dedicated server logs in its own folder, as vanilla's does; the game in its home.
    let (side, thread) = ("server", "Server thread");
    logging::init(home.as_path(), thread);
    // The log's first line names the build, so a bug report that carries the log says which one it was.
    log::info!("Horizon {}", crash::version());
    crash::install(home.clone(), side);
    if horizon_items::native_enchantment_hooks::install(
        crate::enchantment_levels::get_enchantment_level,
    )
    .is_err()
    {
        log::warn!("The enchantment level hook is already installed; keeping the first");
    }
    raise_open_file_limit();
    if let Err(error) = game::run_with(options) {
        // A bad command line is the caller's mistake, not a crash: no report.
        let missing_mods = game::dedicated::is_missing_mods_refusal(error.as_ref());
        let usage = error.is::<game::UsageError>() || missing_mods;
        if !missing_mods {
            log::error!("Horizon: {error}");
        }
        logging::flush();
        // The report's path is the last line on stderr (crash::SAVED_LINE).
        if !usage {
            crash::report_error(&home, side, &*error);
        }
        // Started from Explorer, the console window closes with the game:
        // keep the message on screen until it is read.
        #[cfg(windows)]
        if std::io::IsTerminal::is_terminal(&std::io::stdin()) {
            eprintln!("Press Enter to close.");
            let _ = std::io::stdin().read_line(&mut String::new());
        }
        std::process::exit(if usage {
            game::exit_code::USAGE
        } else {
            game::exit_code::ERROR
        });
    }
    horizon_hooks::engine::shutdown();
    logging::flush();
}

/// The JVM raises its open-file limit to the hard limit as it starts
/// (`-XX:+MaxFDLimit`, on by default), so a vanilla server may hold as many
/// sockets and region files as the host allows, not the soft default (often
/// 1024 in a container). The same here; macOS caps it at `OPEN_MAX`, as the
/// JVM does there.
fn raise_open_file_limit() {
    #[cfg(unix)]
    {
        use rustix::process::{Resource, Rlimit, getrlimit, setrlimit};
        let limit = getrlimit(Resource::Nofile);
        let hard = if cfg!(target_os = "macos") {
            Some(limit.maximum.map_or(10_240, |h| h.min(10_240)))
        } else {
            limit.maximum
        };
        let raised = match (limit.current, hard) {
            (Some(soft), Some(hard)) => hard > soft,
            (Some(_), None) => true,
            (None, _) => false,
        };
        if raised {
            let to = Rlimit {
                current: hard,
                maximum: limit.maximum,
            };
            match setrlimit(Resource::Nofile, to) {
                Ok(()) => log::info!(
                    "Open file limit raised from {:?} to {hard:?}",
                    limit.current
                ),
                Err(e) => log::warn!("Open file limit stays at {:?}: {e}", limit.current),
            }
        }
    }
}
