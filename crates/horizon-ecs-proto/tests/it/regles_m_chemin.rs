use crate::common::*;
use myriade_engine_lib::blocks::Fluid;
use myriade_engine_lib::mobs::data::PathComputationType;
use myriade_engine_lib::mobs::path::PathType;
use myriade_engine_lib::mobs::world::leaked_test_world_mut;
use horizon_ecs_proto::pathsearch::{part_pathfindable, resolve_part_answers};
use horizon_world::registry::BlockStateId;

/// `WalkNodeEvaluator.getPathTypeFromState`: a state's type follows the `isPathfindable(LAND)` answer a mod part gave; without the answers,
/// every state is back to the type its block exports.
#[test]
fn a_states_path_type_follows_the_land_answer_its_parts_gave() {
    have_data();
    let (mw, _, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let (data, facts) = (mw.data, mw.facts);
    let n = data.states.len();
    let exported: Vec<PathType> = (0..n).map(|i| data.path_type(BlockStateId(i as u32))).collect();
    // A state that is blocked and holds no water: a part that says it is land makes it open.
    let k = (0..n)
        .find(|&i| exported[i] == PathType::Blocked && facts.get(BlockStateId(i as u32)).fluid != Fluid::Water)
        .expect("a blocked state");
    let land = part_pathfindable(data, n, |id, kind| (kind == PathComputationType::Land && id.0 as usize == k).then_some(true));
    resolve_part_answers(data, facts, Some(&land));
    assert_eq!(data.path_type(BlockStateId(k as u32)), PathType::Open, "the land answer opens the state");
    resolve_part_answers(data, facts, None);
    assert!((0..n).all(|i| data.path_type(BlockStateId(i as u32)) == exported[i]), "without answers, the exported types");
}
