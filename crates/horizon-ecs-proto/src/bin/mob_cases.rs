//! The single-cow hazard cases of `cow::tests::falls_fire_lava_snow_and_cactus_hurt_a_cow`, run on Horizon's `Mob`
//! (`HORIZON_TEST_RESOURCES` set): the health (or "dead") of each cow at the ticks 40, 140 and 340.
use horizon_client_lib::{
    health::TestLevel,
    mobs::{self, Mob, MobEvent, MobWorld, PendingPushes, Seen},
};
use horizon_core::geometry::Vec3;

fn main() {
    let half = 56;
    let (level, _species, kind) = horizon_ecs_proto::cow::build_level(half, true);
    let w: &MobWorld = level.mw.0;
    let items = w.items.expect("items");
    let cases: [(&str, Vec3); 6] = [
        ("fall 11", Vec3::new(5.5, 12.0, 5.5)),
        ("fall 39", Vec3::new(8.5, 40.0, 5.5)),
        ("lava", Vec3::new(-47.0, 1.0, -47.0)),
        ("fire", Vec3::new(-47.0, 1.0, -39.0)),
        ("powder snow", Vec3::new(-31.0, 1.0, -47.0)),
        ("cactus", Vec3::new(-39.0, 1.0, -39.0)),
    ];
    let mut mobs: Vec<Mob> = cases
        .iter()
        .enumerate()
        .map(|(i, (_, at))| {
            let mut m = Mob::new(i as u32 + 1, kind.clone(), *at, 0.0);
            m.persistent = true;
            m.ai.no_action_time = 100;
            m
        })
        .collect();
    let mut test_level = TestLevel::new(7);
    let mut pushes = PendingPushes::default();
    for t in 1..=340 {
        let mut snapshot: Vec<Seen> = mobs.iter().map(|m| m.seen(None, items)).collect();
        let mut grid = mobs::seen_grid::SeenGrid::new(snapshot.iter().map(|s| s.bounds));
        let mut events: Vec<MobEvent> = Vec::new();
        let mut lvl = test_level.level();
        for slot in 0..mobs.len() {
            mobs::apply_mob_pushes(&mut mobs[slot], &mut pushes);
            let from = events.len();
            let view = MobWorld { others: &snapshot, others_grid: Some(&grid), others_solid: Some(&[]), nearest_player: None, ..w.clone() };
            let kept = mobs::ai::tick(&mut mobs[slot], &view, &mut lvl, &mut events);
            mobs::take_mob_pushes(&mut events, from, &mut pushes);
            let mut s = mobs[slot].seen(None, items);
            s.alive &= kept;
            s.removed |= !kept;
            grid.update(slot, s.bounds);
            snapshot[slot] = s;
        }
        if [40, 140, 340].contains(&t) {
            let line: Vec<String> = cases
                .iter()
                .zip(&mobs)
                .map(|((name, _), m)| {
                    if m.health <= 0.0 || m.removed { format!("{name}: dead") } else { format!("{name}: {}", m.health) }
                })
                .collect();
            println!("tick {t}: {}", line.join(", "));
        }
    }
}
