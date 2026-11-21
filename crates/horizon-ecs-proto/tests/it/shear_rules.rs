//! `behaviours::shear` through the player's shears: each shearable species changes its state once and the host gets the sound, the tool's
//! wear and the game event; the mooshroom becomes a cow; a baby is refused.
use crate::common;
use common::*;
use horizon_ecs_proto::behaviours::Interaction;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::goals::*;
use horizon_ecs_proto::host_out::HostEvent;
use horizon_ecs_proto::items;

/// Spawns `name`, puts the shears in the hand of player 1, runs `prep`, and returns the world, the entity and what the host heard so far.
fn setup(name: &str, seed: u64, prep: impl Fn(&mut Herd, Entity)) -> Option<(Herd, Entity)> {
    if !have_data() {
        return None;
    }
    let sp = species_table().0.into_iter().find(|s| s.name == name)?;
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, seed);
    h.world.resource_mut::<Entered>().0.push(e);
    let shears = items::item_id("minecraft:shears");
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5, y: 1.0, z: 2.5, main: shears, off: 0, ..Default::default() });
    prep(&mut h, e);
    h.world.resource_mut::<Events>().host.drain().for_each(drop);
    Some((h, e))
}

fn click(h: &mut Herd, e: Entity) -> bool {
    interact(&mut h.world, e, &Interaction { player: 1, item: items::item_id("minecraft:shears") })
}

fn heard(h: &mut Herd) -> Vec<HostEvent> {
    h.world.resource_mut::<Events>().host.drain().collect()
}

fn has_sound(out: &[HostEvent], sound: &str) -> bool {
    out.iter().any(|ev| matches!(ev, HostEvent::Sound { name, .. } if *name == sound))
}

#[test]
fn shears_take_the_wool_once_with_sound_wear_and_game_event() {
    let Some((mut h, e)) = setup("minecraft:sheep", 11, |_, _| {}) else { return };
    assert!(click(&mut h, e));
    let out = heard(&mut h);
    assert!(has_sound(&out, "minecraft:entity.sheep.shear"));
    assert!(out.iter().any(|ev| matches!(ev, HostEvent::HeldWorn { player: 1, .. })));
    assert!(out.iter().any(|ev| matches!(ev, HostEvent::PlayerGameEvent { player: 1, name: "minecraft:shear", .. })));
    assert!(!click(&mut h, e), "a shorn sheep is refused");
}

#[test]
fn shears_cut_the_snow_golems_pumpkin() {
    let Some((mut h, e)) = setup("minecraft:snow_golem", 12, |_, _| {}) else { return };
    assert!(click(&mut h, e));
    assert!(has_sound(&heard(&mut h), "minecraft:entity.snow_golem.shear"));
    assert!(!click(&mut h, e));
}

#[test]
fn shears_take_the_mushrooms_off_a_bogged() {
    let Some((mut h, e)) = setup("minecraft:bogged", 13, |_, _| {}) else { return };
    assert!(click(&mut h, e));
    assert!(has_sound(&heard(&mut h), "minecraft:entity.bogged.shear"));
    assert!(!click(&mut h, e));
}

#[test]
fn shears_refuse_a_sulfur_cube_with_nothing_in_its_body_slot() {
    let Some((mut h, e)) = setup("minecraft:sulfur_cube", 14, |_, _| {}) else { return };
    assert!(!click(&mut h, e));
}

#[test]
fn shears_turn_a_mooshroom_into_a_cow_and_a_baby_is_refused() {
    let Some((mut h, e)) = setup("minecraft:mooshroom", 15, |_, _| {}) else { return };
    assert!(click(&mut h, e));
    assert!(has_sound(&heard(&mut h), "minecraft:entity.mooshroom.shear"));
    h.tick();
    assert!(h.world.get_entity(e).is_err(), "the mooshroom is replaced by a cow");
    let Some((mut b, eb)) = setup("minecraft:mooshroom", 16, |h, e| horizon_ecs_proto::behaviours::follow_parent::make_baby(&mut h.world, e, 24000)) else { return };
    assert!(!click(&mut b, eb));
}
