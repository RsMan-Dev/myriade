//! Règles du vol en élytre d'un mob, reprises de `fall_flying_tests.rs` (valeurs vanilla 26.3 inchangées) : le vol ne tient qu'en l'air,
//! non monté, sans lévitation et avec un planeur utilisable ; l'usure tombe aux 20e et 40e ticks de vol ; l'événement tous les 10 ticks.
use horizon_ecs_proto::gliding::{durable_enough, glide_event_on, keeps_gliding, wears_on};
use horizon_ecs_proto::rules::climbable::climbs;

#[test]
fn a_mob_stops_gliding_on_the_ground_riding_levitating_or_without_a_glider() {
    // (au sol, monté, lévité, planeur) -> le vol tient.
    assert!(keeps_gliding(false, false, false, true), "a whole elytra in the air");
    assert!(!keeps_gliding(false, false, false, false), "no glider");
    assert!(!keeps_gliding(true, false, false, true), "grounded");
    assert!(!keeps_gliding(false, true, false, true), "riding");
    assert!(!keeps_gliding(false, false, true, true), "levitating");
}

#[test]
fn a_glider_one_point_from_breaking_does_not_glide() {
    // Élytre de durabilité 432 : de 0 à 430 volent, 431 non ; un objet qui ne casse jamais vole.
    assert!(durable_enough(0, 432));
    assert!(durable_enough(430, 432));
    assert!(!durable_enough(431, 432), "its next point would break it");
    assert!(durable_enough(431, 0), "an item that never breaks");
}

#[test]
fn a_glider_wears_on_the_twentieth_and_fortieth_tick_and_glides_on_every_tenth() {
    let worn: Vec<i32> = (1..=40).filter(|&t| wears_on(t)).collect();
    assert_eq!(worn, [20, 40]);
    let glided: Vec<i32> = (1..=40).filter(|&t| glide_event_on(t)).collect();
    assert_eq!(glided, [10, 20, 30, 40]);
}

#[test]
fn a_glider_in_a_glide_through_block_does_not_climb_a_ladder_that_a_walker_climbs() {
    // Ladder (climbable), no glide-through block: a walker and a glider both climb. Glide-through block: the glider does not, the walker still does.
    assert!(climbs(true, false, false, false));
    assert!(climbs(true, false, true, false));
    assert!(!climbs(true, false, true, true));
    assert!(climbs(true, false, false, true));
    assert!(!climbs(true, true, false, false));
}
