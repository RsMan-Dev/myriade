// Test module extracted without changing its logical path.
use super::*;
use horizon_client::motion::PlayerMotion;

fn positioned(kind: &str, uuid: Option<[i32; 4]>) -> Tag {
    let pos = |v: f64| Tag::Double(v.to_bits());
    let mut tag = Tag::compound([
        ("id", Tag::String(kind.into())),
        ("Pos", Tag::List(vec![pos(4.5), pos(64.0), pos(4.5)])),
    ]);
    if let (Some(uuid), Tag::Compound(fields)) = (uuid, &mut tag) {
        fields.insert("UUID".into(), Tag::IntArray(uuid.to_vec()));
    }
    tag
}

#[test]
#[ignore = "daily"]
fn cloud_fangs_and_kept_entities_keep_their_constructor_uuid() {
    // Opt in only in a fresh child; never mutate the parallel parent's environment.
    if crate::game::native_entity_refs::tests::run_fresh_cases(
        "game::persist::summon_uuid_conflict_tests::cloud_fangs_and_kept_entities_keep_their_constructor_uuid",
        "HORIZON_TEST_SEEDS",
        &["1"],
    ) {
        return;
    }
    use horizon_core::random::RandomSource;

    // Independent Java LCG calculation, as in the mob test above: uniquifier 12345
    // gives this Entity.random's first two nextLongs, the UUID. The constructor takes
    // exactly one uniquifier step, so the next Entity.random is the one the uniquifier
    // squared seeds, whose first nextInt is below.
    const CONSTRUCTOR_UUID: [i32; 4] = [1462068314, 225001827, -1901508738, 42864579];
    const NEXT_ENTITY_FIRST_INT: i32 = -705649180;
    const SAVED: [i32; 4] = [160, 0, 0, 987];
    const KINDS: [&str; 3] = [
        "minecraft:area_effect_cloud",
        "minecraft:evoker_fangs",
        "minecraft:marker",
    ];
    with_game(|game, _| {
        for kind in KINDS {
            for saved in [None, Some(SAVED)] {
                horizon_client::random_source::set_thread_uniquifier(12345).unwrap();
                let tag = positioned(kind, saved);
                let id = if kind == "minecraft:marker" {
                    game.load_other(tag, kind).unwrap()
                } else {
                    game.load_unlisted((0, 0), kind, saved, tag).unwrap();
                    if kind == "minecraft:evoker_fangs" {
                        game.fang_ids().last().copied().unwrap()
                    } else {
                        game.breath_clouds.last().unwrap().id
                    }
                };
                let expected = saved.unwrap_or(CONSTRUCTOR_UUID);
                assert_eq!(game.persist.uuid_of(id), Some(expected), "{kind} {saved:?}");
                assert_eq!(
                    game.uuid_for(id),
                    expected,
                    "{kind} {saved:?}: saving draws no second UUID"
                );
                assert_eq!(
                    horizon_client::random_source::entity().next_int(),
                    NEXT_ENTITY_FIRST_INT,
                    "{kind} {saved:?}: one constructor draw"
                );
            }
        }
    });
}

const PIG: &str = "NoAI:1b,NoGravity:1b,PersistenceRequired:1b,Silent:1b";

fn pig(tag: &str, n: i32, riders: &str) -> String {
    format!("{{id:\"minecraft:pig\",UUID:[I;160,0,0,{n}],Tags:[\"{tag}\"],{PIG}{riders}}}")
}
fn summon(game: &mut Game<'_>, tag: &str, n: i32, children: &str) -> bool {
    let body = pig(tag, n, children);
    let body = body.trim_start_matches("{id:\"minecraft:pig\",");
    game.run_command(&format!("summon minecraft:pig 4.5 200 4.5 {{{body}"))
}
/// The entities in the level that carry `uuid`.
fn live(game: &Game<'_>, n: i32) -> Vec<u32> {
    let mut ids: Vec<u32> = game
        .persist
        .uuids
        .iter()
        .filter(|(_, u)| **u == [160, 0, 0, n])
        .filter(|(id, _)| game.level_entity(**id as i32).is_some())
        .map(|(id, _)| *id)
        .collect();
    ids.sort_unstable();
    ids
}

/// A game with a loaded, force-loaded chunk 0 0, a live pig holding UUID
/// 100 and a live armor stand holding UUID 150.
fn with_game(test: impl FnOnce(&mut Game<'_>, u32)) {
    crate::game::native_entity_refs::tests::with_game(|game| {
        game.options.game_dir = Some(
            std::path::PathBuf::from(std::env::var_os("HORIZON_HOME").unwrap())
                .join(format!("h160-{:?}", std::thread::current().id())),
        );
        game.build_registries();
        game.set_allow_commands(true);
        game.world
            .chunks
            .insert((0, 0), Arc::new(Chunk::empty(0, 0)));
        game.block_ticks
            .add_empty_chunk(ChunkPos::new(0, 0))
            .unwrap();
        assert!(game.run_command("forceload add 0 0"));
        game.player = Some(PlayerMotion::new(game.motion, Vec3::new(2.5, 200.0, 2.5)).unwrap());
        game.creative = true;
        // Setup controls: the live actors the later trees name.
        assert!(summon(game, "h160_existing", 100, ""));
        assert!(game.run_command(
            "summon minecraft:armor_stand 1.5 200 3.5 {UUID:[I;160,0,0,150],Tags:[\"h160_stand\"],NoGravity:1b}"
        ));
        let existing = live(game, 100);
        assert_eq!(existing.len(), 1);
        assert_eq!(live(game, 150).len(), 1);
        test(game, existing[0]);
    });
}
