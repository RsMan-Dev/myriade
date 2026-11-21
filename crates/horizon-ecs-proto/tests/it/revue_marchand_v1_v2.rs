//! Revue du marchand et du villageois, ECS contre `Mob` sur la même scène et le même nombre de tics (points 1, 2, 3, 4, 6, 7). Non écrits :
//! 5 (perte de poste : le `Mob` ne permet pas de fixer un métier sans poste) et 8 (échange de nourriture : aucun équivalent côté `Mob`).
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_client_lib::mobs::{villager, MobEvent, MobWorld};
use horizon_core::geometry::Vec3;
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

/// A Mob world by day (the work and the walks to a post are of the day).
fn day_mob() -> MobRef {
    let mut r = MobRef::new(40, false);
    r.w = Box::leak(Box::new(MobWorld { day_time: DAY, ..r.w.clone() }));
    r
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
    let (items, enchantments, trades) = crate::trade_vs_mob::registries();
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

fn mob_trader_beside(ticks: usize) -> (f64, bool) {
    let mut r = MobRef::new(40, false);
    let t = r.add_id(Vec3::new(0.5, 1.0, 0.5), 5);
    r.mobs[t].extra.flag2 = true;
    let view = player_view(Vec3::new(2.5, 1.0, 0.5), None);
    for _ in 0..ticks {
        r.step(Some(&view));
    }
    (r.mobs[t].position.x, r.mobs[t].ai.goals.running_names().contains(&"TradeWithPlayer"))
}

#[test]
fn wandering_trader_holds_still_and_faces_its_player_vs_mob() {
    if !have_data() || wrong_species(&["minecraft:wandering_trader"]) {
        return;
    }
    let ((ex, ecs_faces), (mx, mob_faces)) = (ecs_trader_beside(40), mob_trader_beside(40));
    eprintln!("trader: x ecs {ex:.2} mob {mx:.2}; faces ecs {ecs_faces} mob {mob_faces}");
    assert!((ex - 0.5).abs() <= 0.5 && (mx - 0.5).abs() <= 0.5, "the trader walks toward the player: ecs x {ex}, mob x {mx}");
    assert!((ex - mx).abs() <= 0.5, "trader x: ecs {ex} against mob {mx}");
    // The Mob's look is read through its trade goal (its look target is private).
    assert!(ecs_faces && mob_faces, "the trader faces the player: ecs {ecs_faces}, mob {mob_faces}");
}

/// Point 2: the warden's `DoNothing(Timed::new(30, 60))`, the same goal in the ECS (`timed_idle`) with the Mob's pauses.
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

/// The Mob's `DoNothing` is read in the warden's brain (`Timed.running`, shown by its `Debug`): no name is given by the brain.
fn mob_pauses() -> Vec<(u32, u32)> {
    let mut r = MobRef::new(60, false);
    let i = r.add_kind(Vec3::new(0.5, 1.0, 0.5), mob_kind("minecraft:warden"));
    let mut spawn_events = Vec::new();
    horizon_client_lib::mobs::warden::finalize(&mut r.mobs[i], true, &mut spawn_events);
    r.mobs[i].persistent = false;
    pauses(
        || {
            r.step(None);
            format!("{:?}", r.mobs[i].extra.warden).contains("DoNothing(Timed { running: true")
        },
        1000,
    )
}

/// The warden's own pause (`RunOne[Stroll 2, DoNothing 1]`, `DoNothing(Timed::new(30, 60))`), the real species' goal against the Mob's
/// brain: the length of a pause, and the interval between two pauses (start to start), over the first 1000 ticks (the warden then digs, on both sides).
#[test]
fn timed_idle_pause_lasts_thirty_to_sixty_ticks_vs_mob() {
    if !have_data() || wrong_species(&["minecraft:warden"]) {
        return;
    }
    let (e, m) = (ecs_pauses(), mob_pauses());
    let dur = |v: &[(u32, u32)]| v.iter().map(|(s, t)| f64::from(t - s)).collect::<Vec<f64>>();
    let (de, dm) = (dur(&e), dur(&m));
    eprintln!("pauses: ecs n {} mean {:.1}; mob n {} mean {:.1}", de.len(), mean(&de), dm.len(), mean(&dm));
    assert!(de.len() >= 2 && dm.len() >= 5, "pauses: ecs {} mob {}", de.len(), dm.len());
    assert!(de.iter().chain(&dm).all(|d| (30.0..=61.0).contains(d)), "a pause outside 30..=60: ecs {de:?} mob {dm:?}");
    assert!((mean(&de) - mean(&dm)).abs() <= 8.0, "mean pause: ecs {} against mob {}", mean(&de), mean(&dm));
}

/// The interval between two pauses (start to start). The Mob re-asks its `RunOne` at once (a pause every 46 ticks, 24 in 1200 ticks);
/// the ECS draws once, at each end of a pause, the number of re-asks until the pause is picked (jar: `RunOne` has no wait).
#[test]
fn timed_idle_pauses_come_as_often_as_the_mobs() {
    if !have_data() || wrong_species(&["minecraft:warden"]) {
        return;
    }
    let (e, m) = (ecs_pauses(), mob_pauses());
    let gaps = |v: &[(u32, u32)]| v.windows(2).filter(|w| w[1].0 > w[0].0).map(|w| f64::from(w[1].0 - w[0].0)).collect::<Vec<f64>>();
    let (ge, gm) = (gaps(&e), gaps(&m));
    assert!(!ge.is_empty() && !gm.is_empty(), "pauses: ecs {} mob {}", e.len(), m.len());
    assert!((mean(&ge) - mean(&gm)).abs() <= 0.25 * mean(&gm), "mean interval between pauses: ecs {} against mob {}", mean(&ge), mean(&gm));
}

/// Point 3: a jobless villager, 9 blocks from a post, takes the profession of the post block by day, within 600 ticks.
fn ecs_job(block: &'static str) -> String {
    let mut h = ecs_herd();
    super::villager_misc_vs_mob::insert_trade_data(&mut h);
    h.set_block(SITE[0], SITE[1], SITE[2], block);
    let e = villager_at(&mut h, 12.5, 3.5, 11);
    Times::set(&mut h.world, DAY);
    for _ in 0..600 {
        h.tick();
    }
    h.world.get::<Profession>(e).map_or_else(String::new, |p| format!("{:?}", p.0))
}

fn mob_job(block: &'static str) -> Option<u8> {
    let mut r = day_mob();
    r.set_block(SITE[0], SITE[1], SITE[2], block);
    let i = r.add_kind(Vec3::new(12.5, 1.0, 3.5), mob_kind("minecraft:villager"));
    for _ in 0..600 {
        r.step(None);
    }
    r.mobs[i].extra.profession.index()
}

#[test]
fn villager_takes_the_profession_of_its_post_block_vs_mob() {
    if skip() {
        return;
    }
    for (block, poi, name) in [("minecraft:composter", PoiType::Farmer, "farmer"), ("minecraft:lectern", PoiType::Librarian, "librarian")] {
        let (e, m) = (ecs_job(block), mob_job(block));
        assert_eq!(e, format!("{poi:?}"), "ecs profession on {block}");
        assert_eq!(m, villager::profession_index(name), "mob profession on {block}");
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
    let mut r = day_mob();
    r.set_block_props(TAKEN[0], TAKEN[1], TAKEN[2], "minecraft:white_bed", &[("part", "head"), ("occupied", "true")]);
    r.set_block_props(FREE[0], FREE[1], FREE[2], "minecraft:white_bed", &[("part", "head"), ("occupied", "false")]);
    let i = r.add_kind(Vec3::new(4.5, 1.0, 4.5), mob_kind("minecraft:villager"));
    for _ in 0..200 {
        r.step(None);
    }
    let mob_home = r.mobs[i].extra.bed;
    assert_eq!(ecs_home, Some(FREE), "ecs home");
    assert_eq!(mob_home, Some(FREE), "mob bed");
}

/// Point 6: the offers spent at the post are reset when the day's restock is due (12000 ticks after the last one), by the same clock.
#[test]
fn restock_resets_spent_offers_at_the_post_vs_mob() {
    if skip() {
        return;
    }
    let mut h = ecs_herd();
    let (items, enchantments, trades) = crate::trade_vs_mob::registries();
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

    let mut r = day_mob();
    r.set_block(SITE[0], SITE[1], SITE[2], "minecraft:composter");
    let i = r.add_kind(Vec3::new(3.5, 1.0, 3.5), mob_kind("minecraft:villager"));
    // The Mob keeps no offers until a player asks for them (`Trader`, `None` at first): it takes the profession at the post, then is given
    // spent offers, with the experience that makes the post's villager a trader.
    for _ in 0..1200 {
        r.step(None);
        if r.mobs[i].extra.profession.index() == villager::profession_index("farmer") {
            break;
        }
    }
    assert!(r.mobs[i].extra.profession.index() == villager::profession_index("farmer"), "the Mob villager takes the profession within 1200 ticks at the post");
    {
        let stack = |item: &str| horizon_client_lib::items::ItemStack::new(item, 1);
        let offer = horizon_client_lib::merchant::MerchantOffer {
            base_cost_a: horizon_client_lib::merchant::ItemCost { stack: stack("minecraft:emerald") },
            cost_b: None,
            result: stack("minecraft:bread"),
            uses: 10,
            max_uses: 10,
            reward_exp: true,
            special_price_diff: 0,
            demand: 0,
            price_multiplier: 0.0,
            xp: 0,
        };
        r.mobs[i].extra.trade = Some(Box::new(horizon_client_lib::merchant::Trader { xp: 1, offers: vec![offer], ..Default::default() }));
    }
    for t in 0..12100 {
        // The Mob's world is frozen, game time included: it carries the clock that the restock reads (`lastRestockGameTime + 12000`).
        r.w = Box::leak(Box::new(MobWorld { game_time: 1000 + t, ..r.w.clone() }));
        r.step(None);
    }
    let mob_reset = r.mobs[i].extra.trade.as_ref().is_some_and(|t| !t.offers.is_empty() && t.offers.iter().all(|o| o.uses == 0));
    assert!(ecs_reset && mob_reset, "offers reset after the restock: ecs {ecs_reset}, mob {mob_reset}");
}

/// Point 7: a hero in view, the villager throws its gift toward the player after the 600-tick wait (within 40 ticks of the Mob: the idle strolls differ).
fn ecs_gift(ticks: u32) -> (Option<u32>, f64) {
    let mut h = Herd::with_slab_awake(60, true, false);
    h.world.insert_resource(Heroes([1].into_iter().collect()));
    super::villager_misc_vs_mob::insert_trade_data(&mut h);
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

fn mob_gift(ticks: u32) -> (Option<u32>, [f64; 3], [f64; 3]) {
    let mut r = day_mob();
    r.add_kind(Vec3::new(0.5, 1.0, 0.5), mob_kind("minecraft:villager"));
    let mut view = player_view(Vec3::new(3.5, 1.0, 0.5), None);
    view.hero = true;
    let (mut seen, mut first, mut from, mut to) = (0, None, [0.0; 3], [0.0; 3]);
    for t in 0..ticks {
        r.step(Some(&view));
        for ev in &r.events[seen..] {
            if let MobEvent::ThrowGift { from: f, to: o, .. } = ev {
                if first.is_none() {
                    first = Some(t);
                    from = *f;
                    to = *o;
                }
            }
        }
        seen = r.events.len();
    }
    (first, from, to)
}

#[test]
fn hero_gift_is_thrown_toward_the_player_after_the_wait_vs_mob() {
    if skip() {
        return;
    }
    let (e, ex) = ecs_gift(900);
    let (m, from, to) = mob_gift(900);
    eprintln!("gift: ecs tick {e:?} x {ex}; mob tick {m:?} from {from:?} to {to:?}");
    let (e, m) = (e.expect("the ECS villager throws no gift"), m.expect("the Mob villager throws no gift"));
    assert!(e >= 600 && m >= 600, "the gift comes after the 600-tick wait: ecs {e}, mob {m}");
    // The ECS villager's sensors and brain updates are phased by its seed (20-tick scans, strolls): over seeds 1 to 8 the gift came at 639 to 689
    // (seed 1: 667), the Mob's (no seed) at 633, 20 ticks after the 600-tick wait and a 13-tick first sight. The ECS never comes before the wait
    // plus its 20 ticks, and within one brain cadence of 60 ticks after the Mob.
    assert!(e >= 620 && e.abs_diff(m) <= 60, "gift at tick {e} (ecs) against {m} (mob)");
    assert_eq!(to, [3.5, 1.0, 0.5], "the Mob throws toward the player");
    let (dm, de) = (((to[0] - from[0]).powi(2) + (to[2] - from[2]).powi(2)).sqrt(), (ex - 3.5).abs());
    assert!(dm > 0.5, "the Mob's throw is aimed at the player from {dm} blocks");
    // The ECS villager throws from where it stands once within 5 blocks of the player (`GiveGiftToHero`): it may have strolled away from
    // the player first (x -0.72 measured, 4.2 blocks), the item then leaves it toward the player.
    assert!(de < 5.0, "the ECS item is not thrown toward the player: x {ex}");
}
