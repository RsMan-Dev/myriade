//! Règle du feu après une téléportation, reprise de `hurt_teleport_contact_tests.rs::a_burning_teleport_does_not_take_the_old_spots_fire_along`
//! (valeur inchangée : 19 au point d'arrivée, pas de réallumage par la case quittée, qui porte du feu). Le jar (Entity.baseTick,
//! snapTo) donne ce résultat ; l'écart à soumettre (journal regles-ecrites-4.txt, rang 7) est le chemin des mouvements de l'ancien
//! point, que l'ECS ne garde pas : ce test ne le couvre pas à lui seul.
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::inside_effects::fire_left;

#[test]
fn a_burning_creature_teleported_off_a_fire_keeps_no_fire_from_the_spot_it_left() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    h.set_block(0, 1, 0, "minecraft:fire");
    h.tick();
    let e = ecs_cow(&mut h, 0.5, 1.0, 0.5, 4);
    set_on_fire(&mut h.world, e, 20);
    teleport(&mut h.world, e, Vec3::new(4.5, 1.0, 0.5));
    h.tick();
    assert_eq!(fire_left(&h.world, e), 19, "no fire where it landed: the spot it left burns, the new one does not");
}
