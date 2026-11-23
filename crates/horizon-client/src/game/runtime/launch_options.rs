//! The command line the game starts with (`Options`): what it asks for, read once at start,
//! then the game folder it names.
use super::*;

#[derive(Clone)]
pub(crate) struct Options {
    pub(super) resources: PathBuf,
    /// `--world NAME`: the world folder under saves/ (a path also, for harnesses).
    pub(super) world: Option<String>,
    /// `--pregen`: enable the command even when the saved Extras switch is off.
    pub(super) pregen: bool,
    /// `HORIZON_SPIKES=MS`: the slow ring's threshold, with spans on every thread; nothing is logged.
    pub(super) spikes: Option<f32>,
    /// `--dedicated`: vanilla's server arguments (`--nogui`, `--port`, `--world`,
    /// `--universe`, `--initSettings`) follow it, and they are kept here.
    pub(super) dedicated: Vec<String>,
    /// `--transfers SETTINGS`: a dedicated server takes a proxy's hand-over requests
    /// (`server.toml`'s `[transfer]`, which horizon-server passes; lan/arrivals.rs).
    pub(super) transfers: Option<horizon_client::lan::arrivals::Settings>,
    /// Content selection and the compiled native runtime shared with resource loaders.
    pub(super) mods: mods::ModsState,
    /// What this Game executes; independent of its current server connection.
    pub(super) execution_role: execution_role::ExecutionRole,
    /// `--game-dir DIR`: the folder `read` makes the working directory (saves, packs...).
    pub(super) game_dir: Option<PathBuf>,
    /// Control socket path (`--control-socket`).
    pub(super) control: Option<PathBuf>,
}

impl Options {
    /// The name the server gives a player it has no name for (stats file, structure block
    /// author): the offline name kept in the game folder (created on first use).
    pub(super) fn player_name(&self) -> String {
        std::env::current_dir().map_or_else(
            |_| horizon_client::offline_name::generate(0),
            |dir| horizon_client::offline_name::load_or_create(&dir),
        )
    }
    /// The command line, then what it asks of the process: the game folder.
    /// A bad command line is a [`launch_api::UsageError`] (exit code 2, no crash report).
    pub(super) fn read(mut o: Self) -> Result<Self> {
        if o.pregen {
            horizon_client::command::vanilla::set_horizon_commands(true);
        }
        if let Some(dir) = o.game_dir.take() {
            std::fs::create_dir_all(&dir)
                .and_then(|()| std::env::set_current_dir(&dir))
                .map_err(|e| format!("game folder {}: {e}", dir.display()))?;
        }
        Ok(o)
    }
    /// The command line alone: no file is read and nothing changes. `None` for `--help` and
    /// `--launch-api`, which print and end the run.
    #[cfg(test)]
    pub(super) fn parse(args: impl Iterator<Item = String>) -> Result<Option<Self>> {
        Self::from_launch(LaunchArguments::parse(
            args,
            &std::env::current_dir()?,
            &horizon_client::logging::game_home(),
            std::env::var_os("HORIZON_HOME").as_deref(),
        )?)
    }
    pub(super) fn from_launch(launch: LaunchArguments) -> Result<Option<Self>> {
        let installation_side = launch.installation_side();
        let LaunchArguments { context, rest } = launch;
        let sibling = std::env::current_exe()?
            .parent()
            .ok_or("missing executable directory")?
            .join("resources");
        let mut o = Self {
            resources: if sibling.join("horizon-resources.json").exists() {
                sibling
            } else {
                std::path::Path::new(".local/horizon-client").join(horizon_core::VANILLA_VERSION)
            },
            world: None,
            pregen: false,
            spikes: std::env::var("HORIZON_SPIKES")
                .ok()
                .and_then(|v| v.parse().ok()),
            dedicated: Vec::new(),
            transfers: None,
            mods: mods::ModsState {
                installation_side,
                overrides_dir: context.game_dir.join(horizon_mod_package::mod_list::DIR),
                dev: context.dev_mods,
                ..Default::default()
            },
            execution_role: execution_role::ExecutionRole::DedicatedServer,
            game_dir: Some(context.game_dir),
            control: None,
        };
        for argument in rest {
            match argument {
                Argument::Switch(arg) => match arg.as_str() {
                    "--help" | "-h" => {
                        println!(
                            "Horizon — Minecraft Java Edition {} in Rust\n\n  --resources PATH        Prepared resources (horizon-prepare)\n  --world NAME            World folder under saves/ (default \"New World\")\n  --game-tests            Enable test-only mod content (or HORIZON_GAME_TESTS=1/true)\n  --control-socket PATH   Control socket path (the supervisor's endpoint file, *.json for TCP)\n  --game-dir DIR          Game folder: saves, server.properties, packs, mods (default: the working directory)\n  --home DIR              Logs and crash reports (default $HORIZON_HOME, else the platform data folder)\n  --pregen                Add Horizon's /pregen command (docs/pregeneration.md)\n  --launch-api            Print the Launch API version and options as JSON (docs/api/launch.md)",
                            horizon_core::VANILLA_VERSION
                        );
                        return Ok(None);
                    }
                    "--launch-api" => {
                        println!("{}", launch_api::reply(&horizon_client::crash::version()));
                        return Ok(None);
                    }
                    "--pregen" => o.pregen = true,
                    "--game-tests" => {}
                    _ => return Err(format!("{arg}: unknown option").into()),
                },
                Argument::Dedicated(args) => {
                    o.dedicated = args;
                    break;
                }
                Argument::Value(arg, value) => {
                    // The error names the option, so the message says which one was refused.
                    let read = || -> Result<()> {
                        match arg.as_str() {
                            "--resources" => o.resources = value.into(),
                            // Vanilla opens only a level id that is one folder of saves/. --world, for
                            // harnesses, also takes a path (tools/bundle_tooltip_e2e.py).
                            "--world" => o.world = Some(value),
                            "--control-socket" => o.control = Some(value.into()),
                            "--transfers" => {
                                o.transfers =
                                    Some(horizon_client::lan::arrivals::Settings::parse(&value)?)
                            }
                            _ => return Err("unknown option".into()),
                        }
                        Ok(())
                    };
                    read().map_err(|e| format!("{arg}: {e}"))?;
                }
            }
        }
        Ok(Some(o))
    }
}
