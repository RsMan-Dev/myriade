//! Villager metadata the server sends: the profession slot and the unhappy counter.
use horizon_client::mobs::{Mob, Species, villager::ResolvedProfessions};
use horizon_protocol::entity_data::Value;

/// [`server_meta_sync`] from the synced tables alone, so the tick's helper threads can build it
/// (`lan/mob_states.rs`).
pub(super) fn server_meta_sync(
    m: &Mob,
    sync: &horizon_client::lan::data::SyncData,
    effects: Vec<(u8, i32, Value)>,
    (player, time): (Option<i32>, i64),
    meta: &mut Vec<(u8, i32, Value)>,
    defaults: &mut Vec<(u8, i32, Value)>,
    table: &ResolvedProfessions,
) {
    super::lan::mob_meta_sync(m, sync, effects, player, time, meta, defaults);
    let index = match m.species() {
        Species::Villager => 19,
        Species::ZombieVillager => 20,
        _ => return,
    };
    for (field, _, value) in meta {
        if *field == index
            && let Value::Ids(data) = value
            && data.len() == 3
        {
            // Unknown saved names keep the existing none projection, without changing the save.
            data[1] = table
                .state_index(&m.extra.profession)
                .and_then(|slot| i32::try_from(slot).ok())
                .unwrap_or(0);
        }
    }
}

pub(super) fn unhappy_counter(
    m: &Mob,
    meta: &mut Vec<(u8, i32, Value)>,
    defaults: &mut Vec<(u8, i32, Value)>,
) {
    if m.extra.ints[0] > 0 {
        meta.push((
            18,
            horizon_protocol::entity_data::serializer::INT,
            Value::Int(m.extra.ints[0]),
        ));
    }
    defaults.push((
        18,
        horizon_protocol::entity_data::serializer::INT,
        Value::Int(0),
    ));
}
