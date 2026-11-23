//! The component harnesses (`HORIZON_SCULK_FIXTURE`, `HORIZON_HOT_PATHS`): each runs on a
//! server Game of its own, on a thread with the server's stack.
use super::{Game, Options, PackResources, Resources, Settings};
use myriade_engine::Result;
use std::sync::Arc;

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
        let options = self.options.clone();
        let result = std::thread::Builder::new()
            .name(myriade_engine::crash::INTEGRATED_SERVER_THREAD.into())
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
    Game::new(res, pack, options, settings)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
