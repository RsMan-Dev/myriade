//! `Warden.tick` → `WardenAi.setDigCooldown`: a warden that is persistent, a passenger or leashed restarts its dig cooldown to 1200 ticks each
//! tick; a free warden keeps the cooldown it had. The expected values are the vanilla ones (`WardenAi.setDigCooldown`, `Warden.tick`).
use horizon_ecs_proto::behaviours::act_phase::ActPhase;
use horizon_ecs_proto::cow::{now_of, spawn_species, Herd, SpeciesTable};
use horizon_ecs_proto::despawn;

#[test]
fn a_persistent_warden_keeps_its_dig_cooldown_at_1200_ticks() {
    if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let warden = *table.iter().find(|s| s.name == "minecraft:warden").unwrap();
    let free = spawn_species(&mut h.world, &warden, 0.5, 0.5, 1);
    let kept = spawn_species(&mut h.world, &warden, 4.5, 0.5, 2);
    despawn::keep(&mut h.world, kept);
    h.tick();
    let now = now_of(&h.world, kept);
    assert_eq!(h.world.get::<ActPhase>(kept).unwrap().dig_cooldown_until, now + 1200, "a persistent warden restarts its cooldown");
    assert_eq!(h.world.get::<ActPhase>(free).unwrap().dig_cooldown_until, 0, "a free warden keeps no cooldown from the tick");
}
