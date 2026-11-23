//! Règles du regard de l'enderman, reprises de `enderman_block_stare_tests.rs` (valeurs vanilla 26.3 inchangées) : le son de regard
//! ne joue qu'une fois en 400 ticks, la première fois toujours.
use horizon_ecs_proto::behaviours::look_for_player::sound_due;

#[test]
fn the_stare_sound_comes_once_in_400_ticks() {
    // Premier regard à 100 : le suivant attend 500 ticks ; un silencieux compte aussi les siens.
    assert!(sound_due(None, 100), "the first stare sounds");
    assert!(!sound_due(Some(100), 150), "within 400 ticks");
    assert!(!sound_due(Some(100), 200), "within 400 ticks");
    assert!(!sound_due(Some(100), 499), "within 400 ticks");
    assert!(sound_due(Some(100), 520), "the end of the anger");
    assert!(!sound_due(Some(520), 560), "within 400 ticks");
}
