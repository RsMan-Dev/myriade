//! `Explosion.getIndirectSourceEntity` for a blast a mob caused: a mob-owned
//! primed TNT spares vehicles without `mobGriefing`, and a mob's own blast
//! hands its kind and UUID to the TNT it primes.
use super::*;
use crate::game::game_tests::tests::server_game;
use horizon_core::nbt::Tag;

/// A blast given an explicit cause of none (a crystal or cart source with no
/// entity) credits no one with what it breaks, even while a primed TNT's owner
/// is known: the explicit source wins, `DamageSource.getEntity` is null.
#[test]
#[ignore = "daily"]
fn an_explicit_cause_of_none_credits_no_one() {
    use crate::game::commands::Target;
    let mut resources = None;
    let mut game = server_game(&mut resources);
    game.combat.blast_owner = Some((7, Text::literal("Steve")));
    game.combat.blast_owner_player = true;
    assert_eq!(
        game.explosion_kill_credit(),
        Some(Target::Player),
        "the TNT owner"
    );
    game.combat.explicit_explosion_cause = Some(None);
    assert_eq!(
        game.explosion_kill_credit(),
        None,
        "an explicit cause of none"
    );
}
