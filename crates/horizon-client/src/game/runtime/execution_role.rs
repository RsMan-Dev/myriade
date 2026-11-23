pub(super) use horizon_gameplay::execution_role::ExecutionRole;

use super::Game;
use mod_api::HostSide;

/// The hook host of `game`'s side.
pub(super) fn host_side<'a>(game: &'a mut Game<'_>) -> HostSide<'a> {
    HostSide::Server(game)
}
