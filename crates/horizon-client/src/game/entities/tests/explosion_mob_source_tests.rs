//! `Explosion.getIndirectSourceEntity` for a blast a mob caused: a mob-owned
//! primed TNT spares vehicles without `mobGriefing`, and a mob's own blast
//! hands its kind and UUID to the TNT it primes.
use super::*;
use crate::game::boats::Boat;
use crate::game::game_tests::tests::server_game;
use horizon_client::gamerules::Value;
use horizon_client::mobs::{Mob, MobEvent, Species};
use horizon_core::nbt::Tag;

/// Blasts a boat 7.7 blocks away with `owner` as the indirect source, and
/// returns what the boat took: its damage and its push.
fn blast_boat(
    game: &mut Game<'_>,
    owner: Option<(u32, Text)>,
    player: bool,
    mob_griefing: bool,
) -> (f32, Vec3) {
    game.boats.clear();
    game.boats
        .push(Boat::new(1, "oak".into(), Vec3::new(16.2, 64.0, 8.5), 0.0));
    assert!(game.ui.rules.set("mob_griefing", Value::Bool(mob_griefing)));
    game.combat.blast_owner = owner;
    game.combat.blast_owner_player = player;
    game.server_explode([8.5, 64.0, 8.5], 4.0, BlockInteraction::Destroy, false)
        .unwrap();
    assert_eq!(game.boats.len(), 1, "the boat survives the blast");
    let boat = &game.boats[0];
    (boat.damage, boat.velocity)
}

#[test]
#[ignore = "daily"]
fn a_mob_owned_blast_spares_vehicles_only_without_mob_griefing() {
    let mut resources = None;
    let mut game = server_game(&mut resources);
    game.build_registries();
    let kind = game.res.mob_types().get("minecraft:pig").unwrap().clone();
    game.mobs
        .push(Mob::new(7, kind, Vec3::new(40.0, 64.0, 40.0), 0.0));
    let pig = || (7, Text::translate("entity.minecraft.pig", &[]));
    let steve = (1, Text::literal("Steve"));
    // VehicleEntity.ignoreExplosion: the indirect source is a mob and
    // mobGriefing is off, so the boat is neither hurt nor pushed.
    let (damage, velocity) = blast_boat(&mut game, Some(pig()), false, false);
    assert_eq!(damage, 0.0, "a mob's blast spares the boat");
    assert_eq!(velocity, Vec3::ZERO, "and does not push it");
    // The same blast with mobGriefing on still hits it.
    let (damage, velocity) = blast_boat(&mut game, Some(pig()), false, true);
    assert!(damage > 0.0, "mobGriefing on lets the boat be hurt");
    assert_ne!(velocity, Vec3::ZERO, "and pushed");
    // A player-owned TNT is not a mob: mobGriefing off does not spare it.
    let (damage, velocity) = blast_boat(&mut game, Some(steve), true, false);
    assert!(damage > 0.0, "a player's blast still hurts the boat");
    assert_ne!(velocity, Vec3::ZERO, "and pushes it");
    // PrimedTnt.getOwner no longer resolves a removed mob.
    game.mobs[0].removed = true;
    let (damage, _) = blast_boat(&mut game, Some(pig()), false, false);
    assert!(damage > 0.0, "a removed owner no longer spares the boat");
}

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
