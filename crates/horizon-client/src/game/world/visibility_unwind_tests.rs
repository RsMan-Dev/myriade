//! Disk regression for saves requested by native listeners during entity passes.
use super::*;
use horizon_client::{
    chunk::Chunk,
    motion::PlayerMotion,
    save::{LevelShape, SaveDir},
    server::user_lists::{BanList, IpBanList, OpList, WhiteList},
};
use mod_api::{Hooks, ItemPickedUpEvent, ServerHost};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

pub(super) fn with_save_game(root: &std::path::Path, test: impl FnOnce(&mut Game<'_>)) {
    native_guest_turn_tests::with_guests(|game| {
        game.world_root = root.to_path_buf();
        game.save = Arc::new(SaveDir::new(root.to_path_buf(), LevelShape::OVERWORLD));
        game.options.dedicated = Some(Vec::new());
        game.options.execution_role = execution_role::ExecutionRole::DedicatedServer;
        game.server_lists = Some(Arc::new(Mutex::new(dedicated::Lists {
            ops: OpList::new(root.join("ops.json")),
            white: WhiteList::new(root.join("whitelist.json")),
            bans: BanList::new(root.join("bans.json")),
            ip_bans: IpBanList::new(root.join("ip-bans.json")),
            use_whitelist: false,
            enforce_whitelist: false,
            op_level: 4,
            player_idle_timeout: 0,
            stop: false,
            autosave: true,
            whitelist_bypass: Vec::new(),
        })));
        game.build_registries();
        game.session.dispatcher = crate::game::command_text::SessionDispatcher(Arc::new(
            horizon_client::command::vanilla::build_for(false, true),
        ));
        game.world
            .chunks
            .insert((0, 0), Arc::new(Chunk::empty(0, 0)));
        game.world.modified.insert((0, 0));
        game.player = Some(PlayerMotion::new(game.motion, Vec3::new(2.0, 64.0, 2.0)).unwrap());
        game.level.player_uuid = Some([0, 0, 0, 99]);
        test(game);
    });
}
