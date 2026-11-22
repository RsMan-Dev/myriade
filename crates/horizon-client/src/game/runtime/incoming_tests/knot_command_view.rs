use super::*;

/// `/kill`, `/tp`, `/rotate`, `/spreadplayers` and `/tag add|remove` leave a natural knot out
/// rather than failing for every target, and find no entity when only knots matched.
#[test]
fn moving_commands_leave_natural_knots_out() {
    use super::commands::{Target, without_knots};
    assert_eq!(
        without_knots(vec![Target::Knot(3), Target::Player]).unwrap(),
        vec![Target::Player]
    );
    assert!(without_knots(vec![Target::Knot(3)]).is_err());
    assert!(without_knots(Vec::new()).unwrap().is_empty());
}
