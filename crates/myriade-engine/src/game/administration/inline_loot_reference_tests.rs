//! `ResourceOrIdArgument.lootTable` decodes an inline `/loot` table while the
//! command is parsed: a `minecraft:loot_table` entry's `value` is a
//! `HolderSet<LootTable>`, so an id the registry does not hold fails the
//! parse. The command never runs, an `execute store` around it stores
//! nothing and no item is given
//! (tools/test_inline_loot_reference_parity.py measures the same on vanilla 26.3).
use super::*;
use myriade_engine::chunk::Chunk;
use horizon_core::position::ChunkPos;
use std::sync::Arc;

const MISSING_MESSAGE: &str = "Failed to parse structure: Failed to parse either. First: Not a tag id; \
     Second: Failed to parse either. First: Not a list: \"parity:il_none\"; \
     Second: Failed to get element parity:il_none from registry minecraft:loot_table";


fn reference(value: &str) -> String {
    format!("{{pools:[{{rolls:1,entries:[{{type:\"minecraft:loot_table\",value:{value}}}]}}]}}")
}

fn score(game: &Game<'_>, name: &str) -> i32 {
    game.score
        .board
        .score(name, "il")
        .map_or(i32::MIN, |s| s.value)
}


#[test]
#[ignore = "daily"]
fn an_inline_reference_to_a_missing_table_fails_the_parse() {
    let mut resources = None;
    let mut game = game(&mut resources);
    // The valid references run: one diamond each time, and nothing for the empty table.
    let (ok, said, r, s, d) = give(&mut game, &reference("\"parity:il_one\""));
    assert_eq!(
        (ok, said.as_deref(), r, s, d),
        (true, Some("Dropped 1 [Diamond]"), 1, 1, 2)
    );
    let (ok, said, r, s, d) = give(&mut game, &reference("\"parity:il_empty\""));
    assert_eq!(
        (ok, said.as_deref(), r, s, d),
        (true, Some("Dropped 0 items"), 0, 1, 0)
    );
    // A missing table, at the top, in a composite's child and in a nested
    // inline table: the parse fails, nothing runs, nothing is stored.
    for table in [
        reference("\"parity:il_none\""),
        "{pools:[{rolls:1,entries:[{type:\"minecraft:alternatives\",children:[{type:\"minecraft:loot_table\",value:\"parity:il_none\"}]}]}]}".to_owned(),
        reference(&reference("\"parity:il_none\"")),
    ] {
        let (ok, said, r, s, d) = give(&mut game, &table);
        assert_eq!(
            (ok, said.as_deref(), r, s, d),
            (false, Some(MISSING_MESSAGE), 31, 31, 0),
            "{table}"
        );
    }
    // The named table is unchanged.
    let (ok, said, r, s, d) = give(&mut game, "parity:il_one");
    assert_eq!(
        (ok, said.as_deref(), r, s, d),
        (true, Some("Dropped 1 [Diamond]"), 1, 1, 2)
    );
}

