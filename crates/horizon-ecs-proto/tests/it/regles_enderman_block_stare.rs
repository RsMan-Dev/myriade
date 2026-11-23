//! Règles de choix de case de l'enderman, reprises de `enderman_block_stare_tests.rs` (valeurs vanilla 26.3 inchangées) :
//! `EndermanLeaveBlockGoal.tick` tire dans la boîte 2 x 2 x 2 de ses pieds, `EndermanTakeBlockGoal.tick` dans la boîte 4 x 3 x 4.
//! Partiel : l'exclusion de l'enderman lui-même et la coupe par les formes de contour du monde ne sont pas testées ici.
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::rules::{leave_block, take_block};

#[test]
fn the_leave_cell_is_drawn_in_the_two_by_two_by_two_box_around_the_feet() {
    let p = Vec3::new(0.5, 0.0, 0.5);
    // x - 1 + 2d, y + 2d, z - 1 + 2d.
    assert_eq!(leave_block::cell_of(p, [0.0, 0.0, 0.0]), [-1, 0, -1]);
    assert_eq!(leave_block::cell_of(p, [0.5, 0.5, 0.5]), [0, 1, 0]);
}

#[test]
fn the_take_cell_is_drawn_in_the_four_by_three_by_four_box() {
    let p = Vec3::new(0.5, 0.0, 0.5);
    // x - 2 + 4d, y + 3d, z - 2 + 4d.
    assert_eq!(take_block::take_cell(p, [0.0, 0.0, 0.0]), [-2, 0, -2]);
    assert_eq!(take_block::take_cell(p, [0.5, 0.5, 0.5]), [0, 1, 0]);
}

#[test]
fn the_reach_line_runs_from_the_column_centre_to_the_cell_centre() {
    let p = Vec3::new(0.5, 0.0, 0.5);
    let (from, to) = take_block::reach_line(p, [2, 0, 0]);
    assert_eq!((from.x, from.y, from.z), (0.5, 0.5, 0.5));
    assert_eq!((to.x, to.y, to.z), (2.5, 0.5, 0.5));
}
