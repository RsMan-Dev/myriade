// Included in persist::tests to keep existing test paths stable.
#[test]
#[ignore = "daily"]
fn malformed_position_from_a_distant_chunk_survives_save_and_unload() {
    super::super::native_entity_refs::tests::with_game(|game| {
        let source = (32, 48);
        game.world.chunks.clear();
        game.world
            .chunks
            .insert(source, Arc::new(Chunk::empty(source.0, source.1)));
        let raw = Tag::compound([
            ("id", Tag::String("minecraft:pig".into())),
            ("Pos", Tag::String("invalid".into())),
            ("UUID", Tag::IntArray(vec![2469, 0, 0, 1])),
            ("marker", Tag::Int(37)),
        ]);
        game.world.queue_entity_nbt(source, vec![raw.clone()]);
        game.spawn_loaded_entities();
        let keys = HashSet::from([source]);
        let saved = game.entity_chunks(&keys, false);
        assert_eq!(saved, vec![(source, vec![raw.clone()])]);
        assert!(
            saved.iter().all(|(key, _)| *key != (0, 0)),
            "never overwrite an unread origin chunk"
        );
        let unloaded = game.entity_chunks(&keys, true);
        assert_eq!(unloaded, vec![(source, vec![raw])]);
    });
}
