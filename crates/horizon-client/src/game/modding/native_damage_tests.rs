//! `living_damage_pre` and `living_damage_post` heard by a trait mod, through the game's own
//! hooks: the damage container each hit fills, the hit's level host, and the mob and dragon
//! hits that fire them.
use super::entity::damage_level_of;
use super::native_entity_refs::tests::with_game;
use horizon_client::command::args::GameMode;
use horizon_client::mobs::{Damage, DamageKind, Mob};
use horizon_core::{geometry::Vec3, random::RandomSource};
use mod_api::{
    self, Hooks, HurtSource, LevelRandomHost, LivingDamagePostEvent, LivingDamagePreEvent,
    LivingIncomingDamageEvent, LivingShieldBlockEvent, Reduction,
};
use std::sync::{Arc, Mutex};

/// What the probe heard: Pre's entity, original damage, new damage and armor's reduction, and
/// Post's health damage, then its entity, health damage, absorption reduction and blocked damage.
#[derive(Debug, Default, PartialEq)]
struct Heard {
    pre: Vec<(i32, f32, f32, f32)>,
    post: Vec<(i32, f32)>,
    post_values: Vec<(i32, f32, f32, f32)>,
    sources: Vec<HitSource>,
    side_effects: Vec<(i32, bool)>,
}

#[derive(Debug, PartialEq)]
struct HitSource {
    phase: &'static str,
    entity: i32,
    causing: i32,
    direct: i32,
}

impl Heard {
    fn source(&mut self, phase: &'static str, entity: i32, source: &HurtSource<'_>) {
        self.sources.push(HitSource {
            phase,
            entity,
            causing: source.causing,
            direct: source.direct,
        });
    }
}

/// The test mod: 225001's incoming hit becomes 4, Pre sets 225003's and 225008's damage to 2,
/// and draws once from the level's random for 225010.
struct Probe(Arc<Mutex<Heard>>);

#[mod_api::__subscriptions]
impl Hooks for Probe {
    fn living_incoming_damage(
        &self,
        _: &mut dyn LevelRandomHost,
        event: &mut LivingIncomingDamageEvent<'_>,
    ) -> mod_api::Flow {
        self.0
            .lock()
            .unwrap()
            .source("incoming", event.entity, event.source);
        if event.entity == 225001 {
            event.container.set_new_damage(4.0);
        }
        mod_api::Flow::Continue
    }
    fn living_shield_block(
        &self,
        _: &mut dyn LevelRandomHost,
        event: &mut LivingShieldBlockEvent<'_>,
    ) {
        if event.entity == 227002 {
            event.block.blocked = true;
            event.block.blocked_damage = event.damage;
        }
    }
    fn living_damage_pre(
        &self,
        level: &mut dyn LevelRandomHost,
        event: &mut LivingDamagePreEvent<'_>,
    ) {
        let container = &mut *event.container;
        if matches!(event.entity, 225003 | 225008) {
            container.set_new_damage(2.0);
        }
        if event.entity == 225010 {
            level.with_random(|random| random.next_int()).unwrap();
        }
        let heard = (
            event.entity,
            container.original(),
            container.new_damage(),
            container.reduction(Reduction::Armor),
        );
        let mut log = self.0.lock().unwrap();
        log.pre.push(heard);
        log.source("pre", event.entity, event.source);
    }
    fn living_damage_post(
        &self,
        _: &mut dyn LevelRandomHost,
        event: &mut LivingDamagePostEvent<'_>,
    ) {
        let heard = (event.entity, event.container.health_damage());
        let mut log = self.0.lock().unwrap();
        log.post.push(heard);
        log.post_values.push((
            event.entity,
            event.container.health_damage(),
            event.container.reduction(Reduction::Absorption),
            event.container.blocked(),
        ));
        log.source("post", event.entity, event.source);
        log.side_effects
            .push((event.entity, event.container.should_cause_side_effects()));
    }
}

/// Runs `test` in a game with the probe installed, and returns what it heard.
fn heard(test: impl FnOnce(&mut super::Game<'_>)) -> Heard {
    let log = Arc::new(Mutex::new(Heard::default()));
    let mods: mod_api::Mods = vec![mod_api::ModEntry::new(
        "probe",
        Box::new(Probe(log.clone())),
    )]
    .into();
    mod_platform::testing::with_mods(mods, || with_game(test));
    std::mem::take(&mut *log.lock().unwrap())
}

fn mob(game: &super::Game<'_>, id: u32, kind: &str) -> Mob {
    let kind = game.res.mob_types().get(kind).unwrap().clone();
    Mob::new(id, kind, Vec3::ZERO, 0.0)
}

#[test]
#[ignore = "daily"]
fn the_original_damage_is_the_hit_before_the_incoming_hook() {
    let heard = heard(|game| {
        let mut pig = mob(game, 225001, "minecraft:pig");
        assert!(pig.damage(
            damage_level_of!(game),
            10.0,
            Damage::of(DamageKind::Generic)
        ));
        assert_eq!(pig.health, 6.0);
    });
    assert_eq!(heard.pre, [(225001, 10.0, 4.0, 0.0)]);
    assert_eq!(heard.post, [(225001, 4.0)]);
}

#[test]
#[ignore = "daily"]
fn pre_sets_the_health_lost_and_the_last_hurt_the_cooldown_compares() {
    let heard = heard(|game| {
        let mut pig = mob(game, 225003, "minecraft:pig");
        assert!(pig.damage(damage_level_of!(game), 6.0, Damage::of(DamageKind::Generic)));
        assert_eq!(pig.health, 8.0);
        // The inflicted damage reaches `lastHurt`.
        assert_eq!(pig.last_hurt, 2.0);
    });
    assert_eq!(heard.pre, [(225003, 6.0, 2.0, 0.0)]);
    assert_eq!(heard.post, [(225003, 2.0)]);
}

#[test]
#[ignore = "daily"]
fn pre_fires_for_a_hit_its_reductions_took_to_zero() {
    let heard = heard(|game| {
        let mut witch = mob(game, 225008, "minecraft:witch");
        // Witch.getDamageAfterMagicAbsorb: none from its own doing.
        let own = Damage::mob(225008, [2.0, 0.0, 2.0]);
        assert!(witch.damage(damage_level_of!(game), 6.0, own));
        assert_eq!(witch.health, 24.0);
    });
    assert_eq!(heard.pre, [(225008, 6.0, 2.0, 0.0)]);
}

#[test]
#[ignore = "daily"]
fn pre_draws_from_the_level_random_of_the_hit() {
    heard(|game| {
        let mut expected = game.world.random.clone();
        expected.next_int();
        let mut pig = mob(game, 225010, "minecraft:pig");
        assert!(pig.damage(damage_level_of!(game), 1.0, Damage::of(DamageKind::Generic)));
        assert_eq!(game.world.random.next_int(), expected.next_int());
    });
}

#[test]
#[ignore = "daily"]
fn a_hit_on_the_dragon_fires_pre_and_post() {
    let heard = heard(|game| {
        game.ecs_with(|ecs, mw| ecs.summon_dragon(mw, [0.0, 80.0, 0.0], 0.0, 0, horizon_ecs_proto::species::ender_dragon::START_PHASE, &horizon_ecs_proto::spawn_sheet::SpawnSheet::default(), &mut || 225009));
        let _ = game.command_damage_dragon(225009, 6.0, "minecraft:generic", DamageKind::Generic, (None, None, None));
        assert_eq!(game.ecs_living_health(225009), Some(194.0));
    });
    assert_eq!(heard.pre, [(225009, 6.0, 6.0, 0.0)]);
    assert_eq!(heard.post, [(225009, 6.0)]);
}

#[test]
#[ignore = "daily"]
fn mob_and_dragon_post_records_fresh_hit_decision() {
    let heard = heard(|game| {
        let mut pig = mob(game, 225014, "minecraft:pig");
        let source = Damage::of(DamageKind::Generic);
        assert!(pig.damage(damage_level_of!(game), 2.0, source));
        assert!(pig.damage(damage_level_of!(game), 4.0, source));
        assert!(!pig.damage(damage_level_of!(game), 3.0, source));
        assert_eq!(pig.health, 6.0);
        game.ecs_with(|ecs, mw| ecs.summon_dragon(mw, [0.0, 80.0, 0.0], 0.0, 0, horizon_ecs_proto::species::ender_dragon::START_PHASE, &horizon_ecs_proto::spawn_sheet::SpawnSheet::default(), &mut || 225015));
        for amount in [2.0, 4.0, 3.0] {
            let _ = game.command_damage_dragon(225015, amount, "minecraft:generic", DamageKind::Generic, (None, None, None));
        }
        assert_eq!(game.ecs_living_health(225015), Some(196.0));
    });
    assert_eq!(
        heard.post,
        [(225014, 2.0), (225014, 2.0), (225015, 2.0), (225015, 2.0)]
    );
    assert_eq!(
        heard.side_effects,
        [
            (225014, true),
            (225014, false),
            (225015, true),
            (225015, false)
        ]
    );
}

/// Run `f` in guest `id`'s turn, the guest in survival mode.
fn as_survival_guest(game: &mut super::Game<'_>, id: u32, f: impl FnOnce(&mut super::Game<'_>)) {
    super::native_guest_turn_tests::open_hub(game);
    super::native_guest_turn_tests::push_guest(game, id, u128::from(id));
    game.as_guest(id, |game| {
        game.set_game_mode(GameMode::Survival);
        f(game);
    })
    .unwrap();
}

#[test]
#[ignore = "daily"]
fn post_fires_for_a_player_hit_that_absorption_took_whole() {
    let heard = heard(|game| {
        as_survival_guest(game, 227001, |game| {
            // Absorption is cut to `max_absorption`, 0 without a modifier.
            let player = game.player.as_mut().unwrap();
            player
                .set_modifier(
                    "minecraft:max_absorption",
                    "test:absorption",
                    10.0,
                    "add_value",
                    true,
                )
                .unwrap();
            player.set_health(2.0, true).unwrap();
            assert!(game.hurt_player_from(2.0, None, "minecraft:generic", Some(None), false));
            let health = &game.player.as_ref().unwrap().health;
            assert_eq!((health.health(), health.absorption()), (20.0, 0.0));
        });
    });
    assert_eq!(heard.post_values, [(227001, 0.0, 2.0, 0.0)]);
}

#[test]
#[ignore = "daily"]
fn post_fires_for_a_player_hit_a_shield_blocked_whole() {
    let heard = heard(|game| {
        as_survival_guest(game, 227002, |game| {
            game.hurt_player_from(2.0, None, "minecraft:generic", Some(None), false);
            assert_eq!(game.player.as_ref().unwrap().health.health(), 20.0);
        });
    });
    assert_eq!(heard.post_values, [(227002, 0.0, 0.0, 2.0)]);
}

#[test]
#[ignore = "daily"]
fn a_hit_inside_the_cooldown_no_stronger_than_the_last_fires_no_post() {
    let heard = heard(|game| {
        // Pre sets 225003's damage to 2, which `lastHurt` keeps.
        let mut pig = mob(game, 225003, "minecraft:pig");
        let source = Damage::of(DamageKind::Generic);
        assert!(pig.damage(damage_level_of!(game), 4.0, source));
        // Inside the cooldown and no stronger than the last hit: refused, no Post.
        assert!(!pig.damage(damage_level_of!(game), 2.0, source));
    });
    assert_eq!(heard.post, [(225003, 2.0)]);
}

#[test]
#[ignore = "daily"]
fn post_fires_once_for_a_killing_blow_on_a_mob() {
    let heard = heard(|game| {
        let mut pig = mob(game, 225016, "minecraft:pig");
        assert!(pig.damage(
            damage_level_of!(game),
            100.0,
            Damage::of(DamageKind::Generic)
        ));
        assert_eq!(pig.health, 0.0);
    });
    // `living_death` is settled by the game's next tick, after the hurt and its Post.
    assert_eq!(heard.post, [(225016, 100.0)]);
}

/// What a killing blow tells a mod, in order: `("post", entity)` and `("death", entity)`.
#[derive(Default)]
struct DeathOrder {
    heard: Mutex<Vec<(&'static str, i64)>>,
    cancel: std::sync::atomic::AtomicBool,
}

struct DeathProbe(Arc<DeathOrder>);

#[mod_api::__subscriptions]
impl Hooks for DeathProbe {
    fn living_damage_post(
        &self,
        _: &mut dyn LevelRandomHost,
        event: &mut LivingDamagePostEvent<'_>,
    ) {
        let entity = i64::from(event.entity);
        self.0.heard.lock().unwrap().push(("post", entity));
    }
    fn living_death(
        &self,
        _: &mut dyn mod_api::ServerHost,
        event: &mut mod_api::LivingDeathEvent<'_>,
    ) -> mod_api::Flow {
        let entity = i64::from(event.entity);
        self.0.heard.lock().unwrap().push(("death", entity));
        if self.0.cancel.load(std::sync::atomic::Ordering::Relaxed) {
            mod_api::Flow::Cancel
        } else {
            mod_api::Flow::Continue
        }
    }
}

#[test]
#[ignore = "daily"]
fn the_container_names_what_armor_took() {
    let heard = heard(|game| {
        let mut zombie = mob(game, 225011, "minecraft:zombie");
        // A mob's attack: armor takes its share, where a generic hit bypasses it.
        let attack = Damage::mob(225012, [2.0, 0.0, 2.0]);
        assert!(zombie.damage(damage_level_of!(game), 10.0, attack));
    });
    // A zombie's two armor points against 10 damage: `CombatRules.getDamageAfterAbsorb`
    // clamps 2 - 10 / 2 up to a fifth of the armor, 0.4, and keeps 1 - 0.4 / 25 of the hit.
    let after = 10.0 * (1.0 - 0.4_f32 / 25.0);
    assert_eq!(heard.pre, [(225011, 10.0, after, 10.0 - after)]);
}

/// A level built the normal way lends the game's hooks to its hits: a path that built it
/// without them would run no mod's damage hook.
#[test]
#[ignore = "daily"]
fn a_game_built_level_has_every_damage_hook() {
    with_game(|game| {
        let hooks = &game.damage_hooks;
        assert!(hooks.incoming.is_some() && hooks.pre.is_some() && hooks.post.is_some());
        assert!(hooks.knockback.is_some() && hooks.totem.is_some() && hooks.shield.is_some());
    });
}

/// The real projectile hit routes (not hand-built HurtSources) preserve the direct
/// entity through incoming, pre and post, independently of whether there is a shooter.
#[test]
#[ignore = "daily"]
fn snapshot_crosses_the_real_damage_hook_with_reductions() {
    let log = heard(|game| {
        let mut container = horizon_world::living_damage::DamageContainer::new(10.0);
        container.record_reduction(horizon_world::living_damage::Reduction::Armor, 3.5);
        container.enter_pre(6.5);
        damage_level_of!(game).damage_pre(
            horizon_client::health::Hurting::Player(
                225003,
                HurtSource {
                    damage_type: "minecraft:generic",
                    causing: 0,
                    direct: 0,
                    position: None,
                },
            ),
            &mut container,
        );
        assert_eq!(container.capture_inflicted(), 2.0);
        assert_eq!(
            container.reduction(horizon_world::living_damage::Reduction::Armor),
            3.5
        );
    });
    assert_eq!(log.pre, [(225003, 10.0, 2.0, 3.5)]);
}

#[test]
#[ignore = "daily"]
fn unsubscribed_damage_does_not_convert() {
    mod_platform::testing::with_mods(Vec::<mod_api::ModEntry>::new().into(), || {
        with_game(|game| {
            super::api_values::damage::CONVERSIONS.with(|count| count.set(0));
            let mut container = horizon_world::living_damage::DamageContainer::new(10.0);
            damage_level_of!(game).damage_pre(
                horizon_client::health::Hurting::Player(
                    225003,
                    HurtSource {
                        damage_type: "minecraft:generic",
                        causing: 0,
                        direct: 0,
                        position: None,
                    },
                ),
                &mut container,
            );
            super::api_values::damage::CONVERSIONS.with(|count| assert_eq!(count.get(), 0));
        })
    });
}
