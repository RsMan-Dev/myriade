//! The host's build metadata agrees with the chunk writer, before any world is open.
use crate::game::native_entity_refs::tests::with_game;
use horizon_client::chunk::Chunk;
use horizon_client::save::{LevelShape, SaveDir};
use horizon_core::nbt::{Limits, Tag};
use horizon_core::position::ChunkPos;
use horizon_world::region::RegionFile;
use mod_api::ServerHost;
use std::sync::Arc;

#[test]
#[ignore = "daily"]
fn host_data_version_matches_a_freshly_saved_chunk() {
    with_game(|game| {
        let scratch = tempfile::tempdir().unwrap();
        let save = SaveDir::new(scratch.path().to_path_buf(), LevelShape::OVERWORLD);
        assert_eq!(
            save.save(
                &[Arc::new(Chunk::empty(0, 0))],
                game.pack.assets.registry,
                game.res.biomes(),
            )
            .unwrap(),
            1
        );
        // Read the actual region record without the save loader's datafixes.
        let saved = RegionFile::open(scratch.path().join("region/r.0.0.mca"))
            .unwrap()
            .read(ChunkPos::new(0, 0))
            .unwrap()
            .expect("saved chunk")
            .nbt(Limits::saved())
            .unwrap();
        let Some(Tag::Int(version)) = saved.tag.get("DataVersion") else {
            panic!("saved chunk must have an integer DataVersion");
        };
        assert_eq!(*version, horizon_core::version::DATA_VERSION);

        let _phase = mod_api::__private::PhaseOwner::scope(mod_api::__private::ModPhase::Setup);
        let server: &dyn ServerHost = game;
        assert_eq!(server.server_data_version(), *version);
    });
}
