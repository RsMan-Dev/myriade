use super::{id, with_game};
use crate::game::commands::Target;
use horizon_core::geometry::Vec3;

#[test]
#[ignore = "daily"]
fn generic_root_moves_specialized_vehicle_before_its_mob() {
    with_game(|g| {
        assert!(g.run_command(r#"summon minecraft:item 4.5 200 4.5 {UUID:[I;161,26,3,1100],NoGravity:1b,Item:{id:"minecraft:stone",count:1},Passengers:[{id:"minecraft:minecart",UUID:[I;161,26,3,1101],NoGravity:1b,Passengers:[{id:"minecraft:skeleton",UUID:[I;161,26,3,1102],NoAI:1b,NoGravity:1b}]}]}"#));
        let root = id(g, 1100).unwrap();
        let cart = id(g, 1101).unwrap();
        let skeleton = id(g, 1102).unwrap();
        assert_eq!(g.passenger_vehicle(cart), Some(root));
        assert_eq!(g.passenger_vehicle(skeleton), Some(cart));
        for [x, y, z] in [[4.5, 200.0, 4.5], [8.5, 205.0, 7.5]] {
            g.place_saved_passenger(g.level_entity(root as i32).unwrap(), Vec3::new(x, y, z));
            g.sync_riders();
            assert_eq!(
                g.target_position(g.level_entity(cart as i32).unwrap()),
                Some([x, y + 0.25, z])
            );
            assert_eq!(
                g.target_position(Target::Mob(skeleton)),
                Some([x, y + 0.25 + 0.1875 - f64::from(0.7_f32), z])
            );
        }
        g.remove_vehicle_tree(root);
        for rider in [root, cart, skeleton] {
            assert!(!g.passenger_present(rider));
        }
    });
}
