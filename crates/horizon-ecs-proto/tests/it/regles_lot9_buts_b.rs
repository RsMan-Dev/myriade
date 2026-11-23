//! Lot 9b of the rules of the old goals (`mobs/goals.rs`, tests 15 to 28): a peaceful player is no target, a ghast is not hit back, the
//! walk and the within-home edges, a worn mob head halves the visibility, a chase ends out of home or for a spectator, the breath air
//! columns in order, universal anger, and a breed with a panicking partner. Expected values are the vanilla 26.3 ones (`javap`).
use crate::common::*;
use bevy_ecs::prelude::*;
use myriade_engine_lib::items::ItemStack;
use myriade_engine_lib::mobs::MobWorld;
use horizon_core::geometry::{Aabb, Vec3};
use horizon_ecs_proto::behaviours::breath_air::air_position;
use horizon_ecs_proto::behaviours::breed::Love;
use horizon_ecs_proto::behaviours::nearest_target::PendingTarget;
use horizon_ecs_proto::brain::behaviours::start_attacking::{resolve_target, TargetAnswer, TargetWatched};
use horizon_ecs_proto::behaviours::hurt_by;
use horizon_ecs_proto::behaviours::move_towards_restriction::Restriction;
use horizon_ecs_proto::behaviours::panic::Panicking;
use horizon_ecs_proto::behaviours::Entry;
use horizon_ecs_proto::cow::{spawn_species, GrowAt, Herd, Level, Mw, SpeciesTable};
use horizon_ecs_proto::hostile::{HurtBy, Meleeing, PlayerState};
use horizon_ecs_proto::items::item_id;
use horizon_ecs_proto::rules::move_to_block::{first_valid, search_zone};
use horizon_ecs_proto::rules::melee_attack::within_strike;
use horizon_ecs_proto::rules::move_towards_restriction::within_home;
use horizon_ecs_proto::species::Config;
use horizon_ecs_proto::targeting::{can_target, head_factor, set_target, sight_range, Target, Who};

static PLAIN: hurt_by::Params = hurt_by::Params::DEFAULT;

/// The species of the table by name.
fn spawn(h: &mut Herd, name: &str, x: f64, z: f64) -> Entity {
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == name).unwrap();
    spawn_species(&mut h.world, &sp, x, z, 5)
}

/// A victim of the run species whose `hurt_by` goal is only the one of the test, hit once by a `hitter` of the species named; the hit
/// is given a few ticks to be answered. Returns the herd, the victim and the hitter.
fn hit_by(hitter: &str) -> (Herd, Entity, Entity) {
    let mut h = Herd::with_slab_awake(20, false, false);
    let entries: &'static [Entry] = Box::leak(Box::new([Entry::new(&hurt_by::MODULE, 1, &PLAIN)]));
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let mut sp = *table.iter().find(|s| s.name == "minecraft:cow").unwrap();
    sp.goals = Config { entries };
    let victim = spawn_species(&mut h.world, &sp, 0.5, 0.5, 7);
    let from = spawn(&mut h, hitter, 4.5, 0.5);
    for _ in 0..5 {
        h.tick();
    }
    h.world.resource_mut::<HurtBy>().0.push((victim, Who::Entity(from)));
    for _ in 0..4 {
        h.tick();
    }
    (h, victim, from)
}

#[test]
fn peaceful_mobs_never_target_the_player() {
    if !have_data() || wrong_species(&["minecraft:iron_golem"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    h.set_online(&[(3.5, 0.5)]);
    let e = spawn(&mut h, "minecraft:iron_golem", 0.5, 0.5);
    assert!(can_target(&h.world, e, Who::Player(1)), "witness: a player in survival is a target");
    h.world.resource_mut::<PlayerState>().peaceful = true;
    assert!(!can_target(&h.world, e, Who::Player(1)), "peaceful: the mob targets the player");
}

#[test]
fn a_hurt_mob_does_not_retaliate_against_a_ghast() {
    if !have_data() || !ground_walker() {
        return;
    }
    // Control: the same hit by a creature of the run species is answered.
    let (h, victim, hitter) = hit_by("minecraft:cow");
    assert_eq!(h.world.get::<Target>(victim).map(|t| t.who), Some(Who::Entity(hitter)), "witness: the hit gives a target");
    // A ghast is refused by `Mob.canAttack`: no answer.
    let (h, victim, _) = hit_by("minecraft:ghast");
    assert!(h.world.get::<Target>(victim).is_none(), "a ghast's hit was answered");
}

#[test]
fn the_walk_stays_in_its_bounds() {
    for range in [-1, 0, 1, 2, 8, 24] {
        for vertical_range in [-2, 0, 1, 2, 3, 6] {
            for vertical_start in [-3, -1, 0, 1, 2] {
                for base in [[0, 64, 0], [-17, -60, 31], [100, 319, -100]] {
                    let zone = search_zone(base, range, vertical_start, vertical_range);
                    let mut walked = Vec::new();
                    let found = first_valid(base, range, vertical_start, vertical_range, |p| {
                        walked.push(p);
                        false
                    });
                    assert_eq!(found, None);
                    assert_eq!(!walked.is_empty(), zone.is_some(), "{range} {vertical_range} {vertical_start}");
                    if let Some((lo, hi)) = zone {
                        for p in walked {
                            assert!((0..3).all(|a| lo[a] <= p[a] && p[a] <= hi[a]), "{p:?}");
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn a_worn_head_hides_the_player_only_from_its_own_type() {
    if !have_data() {
        return;
    }
    let h = Herd::with_slab_awake(20, false, false);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let id = |n: &str| table.iter().find(|s| s.name == n).unwrap().id;
    let (zombie, skeleton) = (id("minecraft:zombie"), id("minecraft:skeleton"));
    let (zombie_head, skull) = (item_id("minecraft:zombie_head"), item_id("minecraft:skeleton_skull"));
    // A zombie at 16 blocks of range: the player in a zombie head is seen at 8 blocks, not at 10.5.
    assert_eq!(sight_range(16.0, head_factor(zombie_head, Some(zombie))), 8.0, "the zombie head halves the range");
    assert_eq!(head_factor(zombie_head, Some(skeleton)), 1.0, "a zombie head does not fool a skeleton");
    assert_eq!(head_factor(skull, Some(zombie)), 1.0, "a skeleton skull does not fool a zombie");
    assert_eq!(head_factor(0, Some(zombie)), 1.0, "no head");
}

#[test]
fn a_mods_visibility_hook_hides_the_player_from_the_mob() {
    if !have_data() || wrong_species(&["minecraft:iron_golem"]) {
        return;
    }
    // At a visibility of 0 the sight floor is 2 blocks (`TargetingConditions`): a player 3 blocks off is not found.
    struct Hide;
    impl horizon_ecs_proto::targeting::LivingVisibilityHost for Hide {
        fn visibility(&self, _: u32, _: u32, _: f64) -> f64 {
            0.0
        }
    }
    let found = |hook: Option<&dyn horizon_ecs_proto::targeting::LivingVisibilityHost>| {
        let mut h = Herd::with_slab_awake(20, false, false);
        h.set_online(&[(3.5, 0.5)]);
        let e = spawn(&mut h, "minecraft:iron_golem", 0.5, 0.5);
        h.world.insert_resource(horizon_ecs_proto::targeting::LivingVisibility(hook.map(horizon_ecs_proto::targeting::LentVisibility::lend)));
        for _ in 0..20 {
            h.tick();
        }
        h.world.get::<Target>(e).map(|t| t.who)
    };
    assert_eq!(found(None), Some(Who::Player(1)), "witness: the player at 3 blocks is found");
    assert_eq!(found(Some(&Hide)), None, "the mod's visibility hides the player");
}

/// `living_change_target` heard by a mod: the nearest-player goal asks first and sets its target only on `Set`; with no listener it sets
/// the target at once.
#[test]
fn a_nearest_target_asked_of_the_host_is_set_only_when_answered() {
    if !have_data() || wrong_species(&["minecraft:iron_golem"]) {
        return;
    }
    let found = |watched: bool| {
        let mut h = Herd::with_slab_awake(20, false, false);
        h.set_online(&[(3.5, 0.5)]);
        let e = spawn(&mut h, "minecraft:iron_golem", 0.5, 0.5);
        h.world.insert_resource(TargetWatched(watched));
        for _ in 0..20 {
            h.tick();
        }
        (e, h)
    };
    let (e, h) = found(false);
    assert_eq!(h.world.get::<Target>(e).map(|t| t.who), Some(Who::Player(1)), "no listener: the target is set at once");
    assert!(h.world.get::<PendingTarget>(e).is_none());
    let (e, mut h) = found(true);
    let w = &mut h.world;
    assert!(w.get::<Target>(e).is_none(), "asked first: no target yet");
    assert!(w.get::<PendingTarget>(e).is_some(), "the goal's target waits for the answer");
    resolve_target(w, e, TargetAnswer::Keep);
    assert!(w.get::<Target>(e).is_none() && w.get::<PendingTarget>(e).is_none(), "Keep leaves no target");
    w.entity_mut(e).insert(PendingTarget(Target::new(Who::Player(1), 40, 3)));
    set_target(w, e, Target::new(Who::Entity(9), 40, 3));
    resolve_target(w, e, TargetAnswer::Keep);
    assert_eq!(w.get::<Target>(e).map(|t| t.who), Some(Who::Entity(9)), "Keep keeps the old target");
    w.entity_mut(e).insert(PendingTarget(Target::new(Who::Player(1), 40, 3)));
    resolve_target(w, e, TargetAnswer::Set(None));
    assert!(w.get::<Target>(e).is_none(), "Set(None) clears the target, as setTarget(null)");
    w.entity_mut(e).insert(PendingTarget(Target::new(Who::Player(1), 40, 3)));
    resolve_target(w, e, TargetAnswer::Set(Some(Who::Player(1))));
    let t = w.get::<Target>(e).copied().expect("Set takes the goal's target");
    assert_eq!((t.who, t.memory, t.slot), (Who::Player(1), 40, 3));
}

#[test]
fn within_home_matches_vanilla_at_the_edges() {
    assert!(within_home([10, 64, -5], 6, [12.0, 64.0, -2.0]));
    assert!(!within_home([10, 64, -5], 6, [16.0, 64.0, -5.0]), "the radius itself is out");
    assert!(!within_home([i32::MIN, 0, 0], 6, [i32::MAX as f64, 0.0, 0.0]));
    assert!(!within_home([i32::MIN; 3], i32::MAX, [i32::MAX as f64; 3]));
    // 65 536² wraps to 0; 46 341² wraps negative.
    assert!(!within_home([0, 0, 0], 65_536, [0.0, 0.0, 0.0]));
    assert!(!within_home([0, 0, 0], 46_341, [0.0, 0.0, 0.0]));
    assert!(within_home([0, 0, 0], 46_340, [46_339.0, 0.0, 0.0]));
}

#[test]
fn searches_neighbor_columns_in_vanilla_order() {
    let mut h = Herd::with_slab_awake(20, false, false);
    let columns = [(0, 0), (0, -1), (1, 0), (0, 1), (-1, 0)];
    for (dx, dz) in columns {
        for y in 20..=29 {
            h.set_block(dx, y, dz, "minecraft:stone");
        }
    }
    h.tick();
    let origin = [0, 20, 0];
    let air = |h: &mut Herd, cells: &[[i32; 3]]| {
        for y in 20..=29 {
            for (dx, dz) in columns {
                h.set_block(dx, y, dz, "minecraft:stone");
            }
        }
        for c in cells {
            h.set_block(c[0], c[1], c[2], "minecraft:air");
        }
        h.tick();
        air_position(&h.world, origin, 9)
    };
    // The column first in order wins over a lower cell of a later column.
    assert_eq!(air(&mut h, &[[1, 21, 0], [0, 27, -1]]), Some([0, 27, -1]));
    assert_eq!(air(&mut h, &[[1, 21, 0]]), Some([1, 21, 0]));
    assert_eq!(air(&mut h, &[[-1, 25, 0]]), Some([-1, 25, 0]));
    assert_eq!(air(&mut h, &[[0, 29, 0]]), Some([0, 29, 0]));
    // One block above the 9-block columns: not searched.
    assert_eq!(air(&mut h, &[[0, 30, 0]]), None);
}

#[test]
fn universal_anger_leaves_the_players_hit_to_the_anger_goals() {
    if !have_data() || !ground_walker() {
        return;
    }
    for (universal, answered) in [(false, true), (true, false)] {
        let (mut h, victim) = hit_by_player(universal);
        assert_eq!(h.world.get::<Target>(victim).is_some(), answered, "universal anger {universal}");
    }
}

/// A hit by player 1 on a creature of the run species, with the universal anger rule `universal`.
fn hit_by_player(universal: bool) -> (Herd, Entity) {
    let mut h = Herd::with_slab_awake(20, false, false);
    let entries: &'static [Entry] = Box::leak(Box::new([Entry::new(&hurt_by::MODULE, 1, &PLAIN)]));
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let mut sp = *table.iter().find(|s| s.name == "minecraft:cow").unwrap();
    sp.goals = Config { entries };
    let victim = spawn_species(&mut h.world, &sp, 0.5, 0.5, 7);
    h.set_online(&[(3.5, 0.5)]);
    let mut level = *h.world.resource::<Level>();
    let rule: &'static MobWorld<'static> = Box::leak(Box::new(MobWorld { universal_anger: universal, ..level.mw.0.clone() }));
    level.mw = Mw(rule);
    *h.world.resource_mut::<Level>() = level;
    for _ in 0..5 {
        h.tick();
    }
    h.world.resource_mut::<HurtBy>().0.push((victim, Who::Player(1)));
    for _ in 0..4 {
        h.tick();
    }
    (h, victim)
}

#[test]
fn a_breed_goal_stops_when_the_partner_panics() {
    if !have_data() || wrong_species(&["minecraft:cow"]) {
        return;
    }
    // Control: a calm partner in love is the mate; a panicking one is not, so no calf comes.
    for (panics, calves) in [(false, true), (true, false)] {
        let mut h = Herd::with_slab_awake(20, false, false);
        let a = spawn(&mut h, "minecraft:cow", 0.5, 0.5);
        let b = spawn(&mut h, "minecraft:cow", 2.5, 0.5);
        h.world.entity_mut(a).insert(Love(u64::MAX));
        h.world.entity_mut(b).insert(Love(u64::MAX));
        if panics {
            h.world.entity_mut(b).insert(Panicking);
        }
        for _ in 0..400 {
            h.tick();
        }
        let babies = h.world.query::<&GrowAt>().iter(&h.world).count();
        assert_eq!(babies > 0, calves, "panicking partner: {panics}");
    }
}

#[test]
fn a_chase_stops_outside_the_mobs_home() {
    if !have_data() || wrong_species(&["minecraft:iron_golem"]) {
        return;
    }
    // Control: the target stays in the home, and the chase goes on.
    for (far, chasing) in [(false, true), (true, false)] {
        let mut h = Herd::with_slab_awake(20, false, false);
        h.set_online(&[(3.5, 0.5)]);
        let e = spawn(&mut h, "minecraft:iron_golem", 0.5, 0.5);
        h.world.entity_mut(e).insert(Restriction { center: [0, 0, 0], radius: 8, wanted: None });
        set_target(&mut h.world, e, Target::player(1, 20));
        for _ in 0..3 {
            h.tick();
        }
        assert!(h.world.get::<Meleeing>(e).is_some(), "the golem is in the chase");
        if far {
            h.set_online(&[(100.5, 0.5)]);
        }
        for _ in 0..2 {
            h.tick();
        }
        assert_eq!(h.world.get::<Meleeing>(e).is_some(), chasing, "far: {far}");
    }
}

#[test]
fn a_chase_stops_when_the_target_is_a_spectator() {
    if !have_data() || wrong_species(&["minecraft:iron_golem"]) {
        return;
    }
    for (spectator, chasing) in [(false, true), (true, false)] {
        let mut h = Herd::with_slab_awake(20, false, false);
        h.set_online(&[(3.5, 0.5)]);
        let e = spawn(&mut h, "minecraft:iron_golem", 0.5, 0.5);
        set_target(&mut h.world, e, Target::player(1, 20));
        for _ in 0..3 {
            h.tick();
        }
        assert!(h.world.get::<Meleeing>(e).is_some(), "the golem is in the chase");
        h.world.resource_mut::<PlayerState>().spectator = vec![false, spectator];
        for _ in 0..2 {
            h.tick();
        }
        assert_eq!(h.world.get::<Meleeing>(e).is_some(), chasing, "spectator: {spectator}");
    }
}

/// A held stack of `item` with the components of `text` (SNBT, a patch as the server sends it), or bare.
fn held(item: &str, text: Option<&str>) -> ItemStack {
    let mut s = ItemStack::new(item, 1);
    if let Some(text) = text {
        let tag = horizon_core::nbt::parse_snbt(text).unwrap().0;
        (s.damage, s.extra) = horizon_items::item_data::decode_components(Some(&tag));
    }
    s
}

/// Whether a creature 0.6 wide at the origin strikes a target whose near face is `gap` blocks east of its box, with `held` and a
/// `vehicle` box joined to it.
fn reaches(held: Option<&ItemStack>, gap: f64, vehicle: Option<Aabb>) -> bool {
    let own = Aabb::new(Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.6, 1.8, 0.6));
    let target = Aabb::new(Vec3::new(0.6 + gap, 0.0, 0.0), Vec3::new(1.6 + gap, 1.8, 0.6));
    within_strike(own, vehicle, target, held, None)
}

#[test]
fn the_active_items_attack_range_sets_the_reach() {
    if !have_data() {
        return;
    }
    // No component: the reach is `Mob.DEFAULT_ATTACK_REACH`, sqrt(2.04) - 0.6 (about 0.83).
    assert!(reaches(None, 0.8, None));
    assert!(!reaches(None, 0.9, None));
    // A spear: max 4.5 and min 2.0 times its mob factor 0.5.
    let spear = held("minecraft:wooden_spear", Some(r#"{"minecraft:attack_range":{min_reach:2.0f,max_reach:4.5f,mob_factor:0.5f}}"#));
    assert!(reaches(Some(&spear), 2.2, None));
    assert!(!reaches(Some(&spear), 2.3, None));
    assert!(!reaches(Some(&spear), 0.5, None), "inside the minimum range");
    assert!(reaches(Some(&spear), 1.1, None));
    // A longer reach, no minimum by default.
    let long = held("minecraft:iron_sword", Some(r#"{"minecraft:attack_range":{max_reach:6.0f}}"#));
    assert!(reaches(Some(&long), 5.9, None));
    assert!(reaches(Some(&long), 0.0, None), "no minimum by default");
    assert!(!reaches(Some(&long), 6.1, None));
    // A removed component: the default again.
    let blunt = held("minecraft:wooden_spear", Some(r#"{"!minecraft:attack_range":{}}"#));
    assert!(reaches(Some(&blunt), 0.5, None));
    assert!(!reaches(Some(&blunt), 2.0, None));
}

#[test]
fn a_riders_attack_box_takes_its_vehicles_width() {
    if !have_data() {
        return;
    }
    // The rider (0.6 wide, from y 1.3) on a horse 1.4 wide: its box is joined to the horse's on x, so a target 1.7 blocks off is struck.
    let rider = Aabb::new(Vec3::new(0.2, 1.3, 0.0), Vec3::new(0.8, 3.1, 0.6));
    let horse = Aabb::new(Vec3::new(-0.4, 0.0, -0.4), Vec3::new(1.0, 1.6, 1.0));
    let target = Aabb::new(Vec3::new(1.7, 1.3, 0.0), Vec3::new(2.7, 3.1, 0.6));
    assert!(!within_strike(rider, None, target, None, None), "witness: the rider alone does not reach");
    assert!(within_strike(rider, Some(horse), target, None, None), "the horse's width is in the box");
}

#[test]
fn a_bare_stack_takes_its_items_prototype_range() {
    if !have_data() {
        return;
    }
    // The stack has no patch: its prototype's `ATTACK_RANGE` (max 4.5, min 2.0, mob factor 0.5) sets the reach, as the item's default does.
    let proto = horizon_core::nbt::parse_snbt(r#"{min_reach:2.0f,max_reach:4.5f,mob_factor:0.5f}"#).unwrap().0;
    let own = Aabb::new(Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.6, 1.8, 0.6));
    let at = |gap: f64| Aabb::new(Vec3::new(0.6 + gap, 0.0, 0.0), Vec3::new(1.6 + gap, 1.8, 0.6));
    let bare = held("minecraft:wooden_spear", None);
    assert!(within_strike(own, None, at(2.2), Some(&bare), Some(&proto)));
    assert!(!within_strike(own, None, at(2.3), Some(&bare), Some(&proto)));
    assert!(!within_strike(own, None, at(2.2), Some(&bare), None), "no prototype: the default reach");
}
