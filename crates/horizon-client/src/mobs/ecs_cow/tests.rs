//! The oracle: the same cows as `Mob`s (the existing tick) and as ECS cows, in the same world, from the
//! same state, ticked in the same order; after every tick each cow's state, the snapshot the others read
//! and the level's random must be equal. See `suivi/ecs-prototype.md` for what this compares and leaves out.
use super::{cowworld::CowWorld, event::CowEvent, plain::PlainWorld};
use crate::{
    health::TestLevel,
    mobs::{self, Mob, MobEvent, MobWorld, PendingPushes, Seen, Who},
};
use horizon_core::{geometry::Vec3, random::RandomSource};
use std::sync::Arc;

/// A walled stone floor with a pond, a lava pit, powder snow, fire and cacti.
fn build(world: &mut crate::world::World, registry: &horizon_world::registry::BlockRegistry) {
    for cx in -2..2 {
        for cz in -2..2 {
            world.chunks.insert((cx, cz), Arc::new(crate::chunk::Chunk::empty(cx, cz)));
        }
    }
    let s = |name: &str| registry.default_state(name).unwrap();
    let (stone, water, lava, snow, fire, cactus) = (
        s("minecraft:stone"),
        s("minecraft:water"),
        s("minecraft:lava"),
        s("minecraft:powder_snow"),
        s("minecraft:fire"),
        s("minecraft:cactus"),
    );
    for x in -32..32 {
        for z in -32..32 {
            world.set(x, -1, z, stone).unwrap();
            world.set(x, 0, z, stone).unwrap();
            if x == -32 || x == 31 || z == -32 || z == 31 {
                world.set(x, 1, z, stone).unwrap();
                world.set(x, 2, z, stone).unwrap();
            }
        }
    }
    for x in 8..14 {
        for z in 8..14 {
            world.set(x, 0, z, water).unwrap();
        }
    }
    for x in -12..-8 {
        for z in 8..12 {
            world.set(x, 0, z, lava).unwrap();
        }
    }
    for x in -10..-6 {
        for z in -10..-6 {
            world.set(x, 0, z, snow).unwrap();
        }
    }
    for x in 0..3 {
        world.set(x, 1, -12, fire).unwrap();
    }
    world.set(5, 1, 5, cactus).unwrap();
    world.set(6, 1, 5, cactus).unwrap();
}

#[derive(Default, Debug)]
pub(super) struct Coverage {
    pub water: bool,
    pub lava: bool,
    pub burning: bool,
    pub hurt: bool,
    pub panicking: bool,
    pub grew: bool,
    pub children: u32,
    pub pushes: u32,
    pub fell: bool,
    pub frozen: bool,
    pub strolled: bool,
    /// (asks, hits) of the ECS cows and of the plain cows.
    pub rest: ((u64, u64), (u64, u64)),
    /// (free, coupled) cow-passes of the systems.
    pub lanes: (u64, u64),
}
thread_local!(static PUSHES: std::cell::Cell<u32> = const { std::cell::Cell::new(0) });

/// One pass of the `Mob`s of a world of cows (`tick_borrowed_creatures`: the snapshot, the grid, the pushes).
pub(super) fn mob_pass(
    mobs: &mut Vec<Mob>,
    w: &MobWorld,
    level: &mut crate::health::DamageLevel<'_>,
    pushes: &mut PendingPushes,
    items: &crate::items::Items,
) -> Vec<MobEvent> {
    let mut snapshot: Vec<Seen> = mobs.iter().map(|m| m.seen(None, items)).collect();
    let mut grid = mobs::seen_grid::SeenGrid::new(snapshot.iter().map(|s| s.bounds));
    let mut events = Vec::new();
    let mut keep = vec![true; mobs.len()];
    for slot in 0..mobs.len() {
        mobs::apply_mob_pushes(&mut mobs[slot], pushes);
        let from = events.len();
        let view = MobWorld { others: &snapshot, others_grid: Some(&grid), others_solid: Some(&[]), ..w.clone() };
        let kept = mobs::ai::tick(&mut mobs[slot], &view, level, &mut events);
        PUSHES.with(|c| c.set(c.get() + events[from..].iter().filter(|e| matches!(e, MobEvent::Push { target: Who::Mob(_), .. })).count() as u32));
        mobs::take_mob_pushes(&mut events, from, pushes);
        let mut s = mobs[slot].seen(None, items);
        s.alive &= kept;
        s.removed |= !kept;
        grid.update(slot, s.bounds);
        snapshot[slot] = s;
        keep[slot] = kept;
    }
    let mut i = 0;
    mobs.retain(|_| {
        i += 1;
        keep[i - 1]
    });
    events
}

/// The child `Game::breed` makes of a cow pair (`create_mob_reason` raw, the inheritance, no orb).
fn child(
    level: &mut crate::health::DamageLevel<'_>,
    id: u32,
    kind: &Arc<mobs::MobType>,
    at: Vec3,
    variant: u8,
    coin: bool,
    partner_variant: Option<u8>,
) -> Mob {
    let yaw = level.random().next_float() * 360.0;
    let (own, _uuid) = mobs::living_random();
    let mut c = Mob::with_random(id, kind.clone(), at, yaw, own);
    c.persistent = true;
    c.extra.age = -24000;
    mobs::spawning::bred_offspring(&mut c);
    c.extra.sound_variant = None;
    c.extra.variant = variant;
    if !coin {
        c.extra.variant = partner_variant.unwrap_or(variant);
    }
    c
}

pub(super) fn run(n: usize, ticks: u32, goals: bool, what: &str) -> Result<(u32, Coverage), String> {
    let mut result: Result<u32, String> = Err("no resources".into());
    let mut cover = Coverage::default();
    let ran = mobs::world::with_built_test_world(&[], build, |w, types| {
        let items = w.items.expect("items");
        let kind = Arc::new(types.types["minecraft:cow"].clone());
        let mut cows: Vec<Mob> = (0..n)
            .map(|i| {
                let (x, z) = if what.contains("spread") {
                    (-30.0 + (i % 10) as f64 * 6.0 + 0.5, -30.0 + (i / 10) as f64 * 6.0 + 0.5)
                } else {
                    (-20.0 + (i % 10) as f64 * 3.0 + 0.5, -20.0 + (i / 10) as f64 * 3.0 + 0.5)
                };
                // A few start high (they fall), a few are babies (they grow up, follow a parent).
                let y = if i % 7 == 3 { 6.0 } else { 1.0 };
                // Some start inside the hazards: the pond, the lava, the powder snow, the fire, the cactus.
                let special = [
                    (10.5, 0.2, 10.5),
                    (-10.5, 0.2, 10.5),
                    (-8.5, 0.2, -8.5),
                    (1.5, 1.0, -12.5),
                    (5.5, 1.0, 5.5),
                    (9.5, 0.2, 9.5),
                ];
                let (x, y, z) = if i < special.len() && n >= 30 { special[i] } else { (x, y, z) };
                let mut m = Mob::new(i as u32 + 1, kind.clone(), Vec3::new(x, y, z), (i * 37 % 360) as f32);
                m.persistent = true;
                if i % 9 == 1 {
                    m.extra.age = -150;
                }
                if !goals {
                    m.ai.goals.clear();
                }
                m
            })
            .collect();
        let mut ecs = CowWorld::new();
        ecs.mutate_order = what == "reverse";
        ecs.systems = what.starts_with("sys");
        ecs.all_free = what.contains("allfree");
        ecs.rest = what.starts_with("rest");
        for m in &cows {
            let e = ecs.spawn_from(m.clone());
            if !goals {
                ecs.clear_goals(e);
            }
        }
        let mut plain = PlainWorld::new();
        plain.rest = what.starts_with("rest");
        for (slot, m) in cows.iter().enumerate() {
            plain.spawn_from(m.clone());
            if !goals {
                plain.clear_goals(slot);
            }
        }
        let mut pushes = PendingPushes::default();
        let (mut la, mut lb, mut lc) = (TestLevel::new(7), TestLevel::new(7), TestLevel::new(7));
        let mut next_id = n as u32 + 1;
        for t in 0..ticks {
            // The damage and the love of the script, to both.
            if t == 100 {
                for slot in 20..22.min(cows.len()) {
                    let src = mobs::Damage::player([0.0, 1.0, 0.0]);
                    cows[slot].damage(&mut la.level(), 3.0, src);
                    let e = ecs.order[slot];
                    ecs.damage(e, &mut lb.level(), 3.0, src);
                    plain.damage(slot, &mut lc.level(), 3.0, src);
                }
            }
            if t == 300 {
                // Two cows die: they leave the list (and the ECS storage moves a row) a few ticks later.
                for id in [11u32, 26] {
                    if let Some(slot) = cows.iter().position(|m| m.id == id) {
                        let src = mobs::Damage::of(mobs::DamageKind::Generic);
                        cows[slot].damage(&mut la.level(), 100.0, src);
                        let e = ecs.order[slot];
                        ecs.damage(e, &mut lb.level(), 100.0, src);
                        plain.damage(slot, &mut lc.level(), 100.0, src);
                    }
                }
            }
            if (t == 180 || t == 200) && cows.len() >= 30 {
                // Cows are put beside a fire, a lava pit and powder snow (in the same chunk, to rest there), then into
                // them (their rest must end: the key must hold what the cow reads, not only the chunks).
                let spots = if t == 180 {
                    [(25, [1.5, 1.0, -10.2]), (27, [-10.5, 1.0, 12.9]), (29, [-8.5, 1.0, -5.2]), (31, [6.5, 1.0, 6.5])]
                } else {
                    [(25, [1.5, 1.0, -11.5]), (27, [-10.5, 0.2, 11.5]), (29, [-8.5, 0.2, -6.5]), (31, [6.5, -0.8, 6.5])]
                };
                for (slot, [x, y, z]) in spots {
                    let at = Vec3::new(x, y, z);
                    cows[slot].position = at;
                    cows[slot].previous = at;
                    cows[slot].velocity = Vec3::ZERO;
                    let e = ecs.order[slot];
                    ecs.teleport(e, at);
                    plain.teleport(slot, at);
                }
            }
            if t == 150 {
                for slot in [30, 32, 14, 15] {
                    if slot < cows.len() {
                        cows[slot].extra.in_love = 600;
                        let e = ecs.order[slot];
                        ecs.set_in_love(e, 600);
                        plain.set_in_love(slot, 600);
                    }
                }
            }
            let (mut ev_a, ev_b);
            ev_a = mob_pass(&mut cows, w, &mut la.level(), &mut pushes, items);
            ev_b = ecs.pass(w, &mut lb.level());
            let ev_c = plain.pass(w, &mut lc.level());
            // The breedings: the child is a new cow at the end of the list.
            let mut kids: Vec<(u32, Vec3, u8, bool, Option<u8>, Arc<mobs::MobType>)> = Vec::new();
            ev_a.retain(|e| {
                if let MobEvent::Breed { parent, partner, parent_variant, .. } = e {
                    let p = cows.iter().find(|m| m.id == *parent);
                    let q = cows.iter().find(|m| m.id == *partner);
                    if let Some(p) = p {
                        kids.push((p.id, p.position, p.extra.variant, parent_variant.unwrap_or(false), q.map(|q| q.extra.variant), p.kind.clone()));
                    }
                    let partner = *partner;
                    if let Some(q) = cows.iter_mut().find(|m| m.id == partner) {
                        q.extra.age = 6000;
                        q.extra.in_love = 0;
                    }
                    false
                } else {
                    true
                }
            });
            let mut seeds = Vec::new();
            for (_, at, variant, coin, partner_variant, k) in kids {
                let (c, replay) = horizon_client_render::random_source::record_seeds(|| {
                    child(&mut la.level(), next_id, &k, at, variant, coin, partner_variant)
                });
                seeds.push(replay);
                next_id += 1;
                cows.push(c);
            }
            let mut made = 0;
            let mut made_plain = 0usize;
            ecs.handle(ev_b, |p| {
                let replay = seeds[made as usize].clone();
                let c = horizon_client_render::random_source::replay_seeds(replay, || {
                    child(&mut lb.level(), n as u32 + 1 + made, &p.kind, p.position, p.variant, p.parent_variant, p.partner_variant)
                });
                made += 1;
                Some(c)
            });
            for e in ev_c {
                let CowEvent::Breed { parent, partner, parent_variant, .. } = e else { continue };
                let pv = plain.facts(parent);
                let qv = plain.facts(partner).map(|f| f.2);
                plain.set_partner(partner);
                if let Some((k, at, variant)) = pv {
                    let replay = seeds[made_plain].fresh();
                    let c = horizon_client_render::random_source::replay_seeds(replay, || {
                        child(&mut lc.level(), n as u32 + 1 + made_plain as u32, &k, at, variant, parent_variant, qv)
                    });
                    made_plain += 1;
                    plain.spawn_from(c);
                }
            }
            let _ = (what, &mut next_id);
            // After the tick.
            if cows.len() != plain.len() {
                result = Err(format!("tick {t}: {} Mobs and {} plain cows", cows.len(), plain.len()));
                return;
            }
            for (slot, m) in cows.iter().enumerate() {
                let mut diffs = plain.differences(slot, m);
                if format!("{:?}", m.seen(None, items)) != format!("{:?}", plain.seen_of(slot)) {
                    diffs.push("the snapshot row".into());
                }
                if !diffs.is_empty() {
                    result = Err(format!("PLAIN tick {t}, cow {slot} (id {}): {}", m.id, diffs.join("; ")));
                    return;
                }
            }
            if cows.len() != ecs.len() {
                result = Err(format!("tick {t}: {} Mobs and {} ECS cows", cows.len(), ecs.len()));
                return;
            }
            for (slot, m) in cows.iter().enumerate() {
                let mut diffs = ecs.differences(slot, m);
                if format!("{:?}", m.seen(None, items)) != format!("{:?}", ecs.seen_of(slot)) {
                    diffs.push("the snapshot row".into());
                }
                if !diffs.is_empty() {
                    result = Err(format!("tick {t}, cow {slot} (id {}): {}", m.id, diffs.join("; ")));
                    return;
                }
            }
            for m in &cows {
                cover.water |= m.fluids.in_water;
                cover.lava |= m.fluids.in_lava;
                cover.burning |= m.fire_ticks > 0;
                cover.hurt |= m.hurt_time > 0;
                cover.panicking |= m.ai.panicking;
                cover.grew |= m.extra.age == 0;
                cover.frozen |= m.ticks_frozen > 0;
                cover.fell |= m.fall_distance > 1.0;
                cover.strolled |= m.ai.goals.is_running("Stroll");
            }
            let (ra, rb, rc) = (la.random().next_int(), lb.random().next_int(), lc.random().next_int());
            if ra != rb || ra != rc {
                result = Err(format!("tick {t}: the level's random streams differ"));
                return;
            }
        }
        result = Ok(ticks);
        cover.children = next_id - n as u32 - 1;
        cover.pushes = PUSHES.with(std::cell::Cell::get);
        cover.rest = (ecs.rest_counts(), plain.rest_counts());
        cover.lanes = ecs.lane_counts();
    });
    if !ran {
        return Err("no resources".into());
    }
    result.map(|t| (t, cover))
}

#[test]
#[ignore = "daily"]
fn one_cow_without_goals_ticks_like_the_mob() {
    assert_eq!(run(1, 200, false, "one").unwrap().0, 200);
}
const _: Option<CowEvent> = None;
const _: Option<Who> = None;

#[test]
#[ignore = "daily"]
fn forty_cows_without_goals_tick_like_the_mobs() {
    let (t, c) = run(40, 500, false, "forty").unwrap();
    eprintln!("{c:?}");
    assert_eq!(t, 500);
}

/// The mutation of the order: the ECS pass visits the cows in reverse, and the test fails.
#[test]
#[ignore = "daily"]
fn visiting_the_cows_in_another_order_is_caught() {
    assert!(run(40, 800, true, "reverse").is_err());
}

#[test]
#[ignore = "daily"]
fn forty_cows_with_their_goals_tick_like_the_mobs() {
    let (t, c) = run(40, 800, true, "goals").unwrap();
    eprintln!("coverage {c:?}");
    assert_eq!(t, 800);
}

/// The resting cows put back what `travel` made last tick: the same states as the Mob cows, and the skip is used.
#[test]
#[ignore = "daily"]
fn forty_cows_that_rest_tick_like_the_mobs() {
    let (t, c) = run(40, 800, true, "rest").unwrap();
    eprintln!("rest coverage {c:?}");
    assert_eq!(t, 800);
    let ((asks, hits), (pasks, phits)) = c.rest;
    assert!(asks > 10_000 && hits * 2 > asks, "{asks} {hits}");
    assert_eq!((asks, hits), (pasks, phits));
}

/// The mutation of the rest key (only the blocks around the cow left in it): the test fails.
#[test]
#[ignore = "daily"]
fn a_rest_key_that_holds_only_the_blocks_is_caught() {
    super::rest::MUTATE_KEY.with(|c| c.set(true));
    let result = run(40, 800, true, "rest_mutated");
    super::rest::MUTATE_KEY.with(|c| c.set(false));
    assert!(result.is_err());
}

/// The hand-written memo on `Mob` (`cow_rest.rs`): the same Mob cows with and without it, tick by tick.
#[test]
#[ignore = "daily"]
fn mob_cows_with_the_rest_memo_tick_like_without_it() {
    let ran = mobs::world::with_built_test_world(&[], build, |w, types| {
        let items = w.items.expect("items");
        let kind = Arc::new(types.types["minecraft:cow"].clone());
        let make = |memo: bool| -> Vec<Mob> {
            (0..40usize)
                .map(|i| {
                    let (x, z) = (-20.0 + (i % 10) as f64 * 3.0 + 0.5, -20.0 + (i / 10) as f64 * 3.0 + 0.5);
                    let y = if i % 7 == 3 { 6.0 } else { 1.0 };
                    let mut m = Mob::new(i as u32 + 1, kind.clone(), Vec3::new(x, y, z), (i * 37 % 360) as f32);
                    m.persistent = true;
                    m.memo.0.enabled = memo;
                    if i % 9 == 1 {
                        m.extra.age = -150;
                    }
                    m
                })
                .collect()
        };
        let (mut a, mut b) = (make(false), make(true));
        let (mut pa, mut pb) = (PendingPushes::default(), PendingPushes::default());
        let (mut la, mut lb) = (TestLevel::new(7), TestLevel::new(7));
        let mut asks_hits = (0u64, 0u64);
        for t in 0..800u32 {
            if t == 100 {
                for slot in 20..22 {
                    let src = mobs::Damage::player([0.0, 1.0, 0.0]);
                    a[slot].damage(&mut la.level(), 3.0, src);
                    b[slot].damage(&mut lb.level(), 3.0, src);
                }
            }
            let _ = mob_pass(&mut a, w, &mut la.level(), &mut pa, items);
            let _ = mob_pass(&mut b, w, &mut lb.level(), &mut pb, items);
            for (slot, (x, y)) in a.iter().zip(&b).enumerate() {
                let diffs = super::compare_mob::differences(x, y);
                assert!(diffs.is_empty(), "tick {t}, cow {slot}: {}", diffs.join("; "));
            }
            assert_eq!(la.random().next_int(), lb.random().next_int());
        }
        for m in &b {
            asks_hits.0 += m.memo.0.asks;
            asks_hits.1 += m.memo.0.hits;
        }
        eprintln!("mob memo asks {} hits {}", asks_hits.0, asks_hits.1);
        assert!(asks_hits.1 * 2 > asks_hits.0);
    });
    assert!(ran);
}

/// The pass as systems, cows far apart (most are free), then close together (most are coupled).
#[test]
#[ignore = "daily"]
fn forty_cows_in_systems_tick_like_the_mobs() {
    for what in ["sys_spread", "sys"] {
        let (t, c) = run(40, 800, true, what).unwrap();
        eprintln!("{what} coverage {c:?}");
        assert_eq!(t, 800);
        assert!(c.lanes.0 > 0 && c.lanes.1 > 0, "{:?}", c.lanes);
    }
}

/// The mutation of the classification (every cow declared free): the test fails.
#[test]
#[ignore = "daily"]
fn declaring_every_cow_free_is_caught() {
    assert!(run(40, 800, true, "sys_allfree").is_err());
}

/// The mutation of the base memo's key (only the blocks around the cow): the test fails.
#[test]
#[ignore = "daily"]
fn a_base_key_that_holds_only_the_blocks_is_caught() {
    super::rest::MUTATE_BASE.with(|c| c.set(true));
    let result = run(40, 800, true, "rest_base_mutated");
    super::rest::MUTATE_BASE.with(|c| c.set(false));
    assert!(result.is_err());
}
