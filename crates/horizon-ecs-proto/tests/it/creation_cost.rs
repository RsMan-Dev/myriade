//! Instructions retired per creation of one species (`spawn_species`), read with `CREATION_COST=1`; no assertion (a measure).
use horizon_ecs_proto::{
    cow::{spawn_species, species_table, Herd},
    instr::Counter,
};

#[test]
#[ignore = "a measure: run with CREATION_COST=1 --ignored --nocapture"]
fn creation_cost_instructions_per_species_creation() {
    if std::env::var("CREATION_COST").is_err() || !super::common::have_data() {
        return;
    }
    let Some(counter) = Counter::new() else { return };
    let mut h = Herd::with_slab(40, true, false);
    h.tick();
    let Some(sp) = species_table().0.into_iter().find(|s| s.name == "minecraft:cow") else { return };
    let n = 2000_u32;
    for i in 0..200_u32 {
        spawn_species(&mut h.world, &sp, 0.5 + f64::from(i % 50), 0.5 + f64::from(i / 50), 1);
    }
    let mut best = u64::MAX;
    for round in 0..5_u64 {
        let from = counter.read();
        for i in 0..n {
            spawn_species(&mut h.world, &sp, 0.5 + f64::from(i % 50), 0.5 + f64::from(i / 50), round + 2);
        }
        best = best.min((counter.read() - from) / u64::from(n));
    }
    eprintln!("creation cost: {best} instr per cow creation");
}
