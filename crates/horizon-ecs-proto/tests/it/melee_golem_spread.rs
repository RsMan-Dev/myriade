//! Point 5: `IronGolem.doHurtTarget` deals half the attack damage plus a draw below its integer part.
use horizon_ecs_proto::rules::hostile::spread_damage;

#[test]
fn half_the_damage_plus_the_draw() {
    assert_eq!(spread_damage(15.0, |bound| { assert_eq!(bound, 15); 4 }), 11.5);
}

#[test]
fn below_one_the_damage_is_kept_and_nothing_is_drawn() {
    assert_eq!(spread_damage(0.5, |_| panic!("no draw below 1")), 0.5);
}
