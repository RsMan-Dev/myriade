//! `AbstractVillager.setUnhappy` starts the shake with `setUnhappyCounter(40)`: a refusal restarted before the 40 ticks are over takes
//! the counter back to 40 (not to what was left), and the first deadline does not end the restarted shake. Needs the vanilla data.
use horizon_core::nbt::{JavaString, Tag};
use horizon_ecs_proto::{
    behaviours::unhappy::{self, Unhappy},
    cow::{now_of, Herd},
    nbt,
};

#[test]
fn a_restart_resets_the_counter_to_forty_and_outlives_the_first_deadline() {
    if !super::common::have_data() {
        return;
    }
    let mut h = Herd::with_slab(40, false, false);
    h.tick();
    let mut c = horizon_core::nbt::Compound::default();
    c.insert("id".into(), Tag::String(JavaString::from("minecraft:cow")));
    c.insert("Pos".into(), Tag::List(vec![Tag::Double(0.5f64.to_bits()), Tag::Double(1.0f64.to_bits()), Tag::Double(0.5f64.to_bits())]));
    let e = nbt::restore(&mut h.world, &c).expect("a creature");
    unhappy::start(&mut h.world, e);
    let first = h.world.get::<Unhappy>(e).unwrap().until;
    for _ in 0..10 {
        h.tick();
    }
    unhappy::start(&mut h.world, e);
    let now = now_of(&h.world, e);
    assert_eq!(h.world.get::<Unhappy>(e).unwrap().until, now + 40, "the counter is 40 again, not the 30 left");
    assert!(first < now + 40);
    while now_of(&h.world, e) <= first + 1 {
        h.tick();
    }
    assert!(h.world.get::<Unhappy>(e).is_some(), "the first deadline must not end the restarted shake");
    while now_of(&h.world, e) < now + 41 {
        h.tick();
    }
    assert!(h.world.get::<Unhappy>(e).is_none(), "the restarted shake ends 40 ticks after the restart");
}
