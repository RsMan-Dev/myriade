//! Revue du marchand et du villageois, ECS seul sur la même scène et le même nombre de tics (points 1, 2, 3, 4, 6, 7). Non écrits :
//! 5 (perte de poste) et 8 (échange de nourriture).
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use myriade_engine_lib::mobs::villager;
use horizon_ecs_proto::{
    behaviours::{
        food_gift::Heroes,
        poi_claim::ClaimIndex,
        profession::Profession,
        schedule::Times,
        timed_idle,
        trade_offers::{self, Merchant},
        trade_session, Entry,
    },
    brain::*,
    cow::*,
    goals::*,
    item::Item,
    species::{villager::HOME, Config},
    targeting::Who,
};
use horizon_world::poi::PoiType;

const DAY: i64 = 4000;
const SITE: [i32; 3] = [3, 1, 3];

fn skip() -> bool {
    !have_data() || wrong_species(&["minecraft:villager"])
}

fn villager_at(h: &mut Herd, x: f64, z: f64, seed: u64) -> Entity {
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == "minecraft:villager").unwrap();
    let e = spawn_species(&mut h.world, &sp, x, z, seed);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.resource_mut::<Entered>().0.push(e);
    e
}

fn ecs_herd() -> Herd {
    let mut h = Herd::with_slab_awake(40, true, false);
    h.world.insert_resource(ClaimIndex::default());
    h.tick();
    h
}

/// The ticks where `step` is on, as (start, end) pairs.
fn pauses(mut step: impl FnMut() -> bool, ticks: u32) -> Vec<(u32, u32)> {
    let (mut out, mut from) = (Vec::new(), None);
    for t in 0..ticks {
        match (from, step()) {
            (None, true) => from = Some(t),
            (Some(s), false) => {
                out.push((s, t));
                from = None;
            }
            _ => {}
        }
    }
    out
}

/// Point 1: a wandering trader that trades with a player 2 blocks away holds still and faces it.
fn ecs_trader_beside(ticks: usize) -> (f64, bool) {
    let mut h = Herd::with_slab_awake(40, true, false);
    h.tick();
    h.set_day(true);
    let e = spawn_species(&mut h.world, species(), 0.5, 0.5, 5);
    h.world.resource_mut::<Entered>().0.push(e);
    let (items, enchantments, trades) = crate::common::registries();
    h.world.insert_resource(trade_offers::TradeData { trades, items, enchantments, seed: 0x5EED, sequences: Default::default() });
    let y = h.world.get::<Pos>(e).unwrap().0.y;
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 2.5, y, z: 0.5, ..Default::default() });
    h.tick();
    trade_session::open(&mut h.world, e, 1);
    let mut faces = false;
    for _ in 0..ticks {
        h.tick();
        // The trader is a goal-based creature (no brain memory): the look control's target is the player's position.
        faces |= h.world.get::<horizon_ecs_proto::goals::Look>(e).is_some_and(|l| (l.target[0] - 2.5).abs() < 0.5 && (l.target[2] - 0.5).abs() < 0.5);
    }
    (h.world.get::<Pos>(e).unwrap().0.x, faces)
}

#[test]
fn wandering_trader_holds_still_and_faces_its_player_vs_mob() {
    if !have_data() || wrong_species(&["minecraft:wandering_trader"]) {
        return;
    }
    let (ex, ecs_faces) = ecs_trader_beside(40);
    eprintln!("trader: x ecs {ex:.2}; faces ecs {ecs_faces}");
    assert!((ex - 0.5).abs() <= 0.5, "the trader walks toward the player: ecs x {ex}");
    assert!(ecs_faces, "the trader faces the player: ecs {ecs_faces}");
}

/// Point 2: the warden's `DoNothing(Timed::new(30, 60))`, the same goal in the ECS (`timed_idle`).
fn ecs_pauses_of(seed: u64) -> Vec<(u32, u32)> {
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = spawn_species(&mut h.world, species(), 0.5, 0.5, seed);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.resource_mut::<Entered>().0.push(e);
    pauses(
        || {
            h.tick();
            h.world.get::<GoalSet>(e).unwrap().is_running(&timed_idle::MODULE)
        },
        1000,
    )
}

/// Eight seeds, for the ECS draws few pauses in 1000 ticks.
fn ecs_pauses() -> Vec<(u32, u32)> {
    (70..78).flat_map(ecs_pauses_of).collect()
}

/// The warden's own pause (`RunOne[Stroll 2, DoNothing 1]`, `DoNothing(Timed::new(30, 60))`): the length of a pause, over the first 1000
/// ticks (the warden then digs).
#[test]
fn timed_idle_pause_lasts_thirty_to_sixty_ticks_vs_mob() {
    if !have_data() || wrong_species(&["minecraft:warden"]) {
        return;
    }
    let e = ecs_pauses();
    let de: Vec<f64> = e.iter().map(|(s, t)| f64::from(t - s)).collect();
    eprintln!("pauses: ecs n {} mean {:.1}", de.len(), mean(&de));
    assert!(de.len() >= 2, "pauses: ecs {}", de.len());
    assert!(de.iter().all(|d| (30.0..=61.0).contains(d)), "a pause outside 30..=60: ecs {de:?}");
}

/// Point 3: a jobless villager, 9 blocks from a post, takes the profession of the post block by day, within 600 ticks.
fn ecs_job(block: &'static str) -> String {
    let mut h = ecs_herd();
    crate::common::insert_trade_data(&mut h);
    h.set_block(SITE[0], SITE[1], SITE[2], block);
    let e = villager_at(&mut h, 12.5, 3.5, 11);
    Times::set(&mut h.world, DAY);
    for _ in 0..600 {
        h.tick();
    }
    h.world.get::<Profession>(e).map_or_else(String::new, |p| format!("{:?}", p.0))
}

#[test]
fn villager_takes_the_profession_of_its_post_block_vs_mob() {
    if skip() {
        return;
    }
    for (block, poi) in [("minecraft:composter", PoiType::Farmer), ("minecraft:lectern", PoiType::Librarian)] {
        let e = ecs_job(block);
        assert_eq!(e, format!("{poi:?}"), "ecs profession on {block}");
    }
}

/// Point 4: a bed whose head is occupied is not claimed; the free bed beside it is.
#[test]
fn villager_claims_the_free_bed_not_the_occupied_one_vs_mob() {
    if skip() {
        return;
    }
    const TAKEN: [i32; 3] = [6, 1, 3];
    const FREE: [i32; 3] = [6, 1, 6];
    let mut h = ecs_herd();
    h.set_block_state(TAKEN[0], TAKEN[1], TAKEN[2], "minecraft:white_bed", &[("part", "head"), ("occupied", "true")]);
    h.set_block_state(FREE[0], FREE[1], FREE[2], "minecraft:white_bed", &[("part", "head"), ("occupied", "false")]);
    let e = villager_at(&mut h, 4.5, 4.5, 11);
    Times::set(&mut h.world, DAY);
    for _ in 0..200 {
        h.tick();
    }
    let ecs_home = match get(&h.world, e, &HOME) {
        Some(MemValue::Block(b)) => Some(*b),
        _ => None,
    };
    assert_eq!(ecs_home, Some(FREE), "ecs home");
}

/// Point 6: the offers spent at the post are reset when the day's restock is due (12000 ticks after the last one).
#[test]
fn restock_resets_spent_offers_at_the_post_vs_mob() {
    if skip() {
        return;
    }
    let mut h = ecs_herd();
    let (items, enchantments, trades) = crate::common::registries();
    h.world.insert_resource(trade_offers::TradeData { trades, items, enchantments, seed: 0x5EED, sequences: Default::default() });
    h.set_block(SITE[0], SITE[1], SITE[2], "minecraft:composter");
    let e = villager_at(&mut h, 3.5, 3.5, 11);
    let farmer = villager::profession_index("farmer").unwrap();
    h.world.entity_mut(e).insert((Profession(PoiType::Farmer), Merchant::new(false, farmer, 0)));
    h.world.get_mut::<Merchant>(e).unwrap().trade.xp = 1;
    trade_offers::ensure_offers(&mut h.world, e);
    {
        let mut m = h.world.get_mut::<Merchant>(e).unwrap();
        for o in m.trade.offers.iter_mut() {
            o.uses = o.max_uses;
        }
    }
    for _ in 0..12100 {
        h.tick();
    }
    let offers = &h.world.get::<Merchant>(e).unwrap().trade.offers;
    let ecs_reset = !offers.is_empty() && offers.iter().all(|o| o.uses == 0);
    assert!(ecs_reset, "offers reset after the restock: ecs {ecs_reset}");
}

/// Point 7: a hero in view, the villager throws its gift toward the player after the 600-tick wait.
fn ecs_gift(ticks: u32) -> (Option<u32>, f64) {
    let mut h = Herd::with_slab_awake(60, true, false);
    h.world.insert_resource(Heroes([1].into_iter().collect()));
    crate::common::insert_trade_data(&mut h);
    villager_at(&mut h, 0.5, 0.5, 1);
    Times::set(&mut h.world, DAY);
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 3.5, y: 1.0, z: 0.5, ..Default::default() });
    let (mut first, mut x) = (None, 0.0);
    for t in 0..ticks {
        h.tick();
        if first.is_none() {
            if let Some(p) = h.world.query::<(&Item, &Pos)>().iter(&h.world).map(|(_, p)| p.0.x).next() {
                first = Some(t);
                x = p;
            }
        }
    }
    (first, x)
}

#[test]
fn hero_gift_is_thrown_toward_the_player_after_the_wait_vs_mob() {
    if skip() {
        return;
    }
    let (e, ex) = ecs_gift(900);
    eprintln!("gift: ecs tick {e:?} x {ex}");
    let e = e.expect("the ECS villager throws no gift");
    assert!(e >= 600, "the gift comes after the 600-tick wait: ecs {e}");
    // The ECS villager throws from where it stands once within 5 blocks of the player (`GiveGiftToHero`): it may have strolled away from
    // the player first, the item then leaves it toward the player.
    assert!((ex - 3.5).abs() < 5.0, "the ECS item is not thrown toward the player: x {ex}");
}
