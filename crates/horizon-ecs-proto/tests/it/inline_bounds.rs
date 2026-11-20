//! The per-entry state of a module (`Tempts`, `Avoiders`...) is a small inline array indexed by `Inst::nth`: no species may carry more
//! entries of the same module than the array holds. Names are those of `Module::name`.
use horizon_ecs_proto::species;

const BOUNDED: &[(&str, usize)] = &[("tempt", 2), ("avoid_entity", 3), ("nearest_target", 6), ("owner_target", 2), ("village_walk", 2), ("side_heads", 2), ("follow_player_vehicle", 2)];

#[test]
fn no_species_carries_more_entries_of_a_module_than_its_inline_array_holds() {
    let mut worst: Vec<(String, usize, String)> = Vec::new();
    for name in species::names() {
        let cfg = species::config(name);
        for &(m, cap) in BOUNDED {
            let n = cfg.entries.iter().filter(|e| e.module.name == m).count();
            if n > cap {
                worst.push((m.to_string(), n, name.to_string()));
            }
        }
    }
    assert!(worst.is_empty(), "more entries than the inline array: {worst:?}");
}
