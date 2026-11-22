//! Prepared-resource regressions through real projectile collision, mob AI and server death.
#![cfg(test)]
use crate::game::Game;
use crate::game::native_entity_refs::tests::{bind_local_player, with_game};
use horizon_client::{chunk::Chunk, items::ItemStack, motion::PlayerMotion};
use horizon_core::{geometry::Vec3, position::ChunkPos};
use mod_api::{
    ArmorHurtEvent, Flow, Hooks, HurtSource, LevelRandomHost, LivingDeathEvent,
    LivingIncomingDamageEvent, ModEntry, ServerHost,
};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, PartialEq)]
struct Hit {
    entity: u32,
    kind: String,
    damage_type: String,
    causing: i32,
    direct: i32,
    position: Option<[f64; 3]>,
    weapon: Option<mod_api::ItemStack>,
}
impl Hit {
    fn new(
        entity: u32,
        kind: &str,
        source: &HurtSource<'_>,
        weapon: Option<&mod_api::ItemStack>,
    ) -> Self {
        Self {
            entity,
            kind: kind.into(),
            damage_type: source.damage_type.into(),
            causing: source.causing,
            direct: source.direct,
            position: source.position,
            weapon: weapon.cloned(),
        }
    }
}

#[derive(Default)]
struct Seen {
    incoming: Vec<Hit>,
    deaths: Vec<Hit>,
    armor: Vec<Hit>,
    kinds: Vec<(String, String)>,
}
struct Probe {
    seen: Arc<Mutex<Seen>>,
    cancel: bool,
    heal: bool,
}
#[mod_api::__subscriptions]
impl Hooks for Probe {
    fn living_incoming_damage(
        &self,
        _: &mut dyn LevelRandomHost,
        event: &mut LivingIncomingDamageEvent<'_>,
    ) -> Flow {
        self.seen.lock().unwrap().incoming.push(Hit::new(
            event.entity as u32,
            event.kind,
            event.source,
            event.weapon,
        ));
        Flow::Continue
    }
    fn living_death(&self, host: &mut dyn ServerHost, event: &mut LivingDeathEvent<'_>) -> Flow {
        self.seen.lock().unwrap().deaths.push(Hit::new(
            event.entity,
            event.kind,
            event.source,
            event.weapon,
        ));
        let entity = host.entity(event.entity).expect("dying mob still resolves");
        let kind = entity.kind(host).expect("dying entity has a type").id(host);
        self.seen
            .lock()
            .unwrap()
            .kinds
            .push((event.kind.to_owned(), kind));
        if self.heal {
            let entity = host
                .entity(event.entity)
                .expect("dying entity still resolves");
            let living = host.living(entity).expect("dying body is living");
            assert!(living.set_health(host, 5.0));
            assert_eq!(living.health(host), Some(5.0));
        }
        if self.cancel && self.seen.lock().unwrap().deaths.len() == 1 {
            Flow::Cancel
        } else {
            Flow::Continue
        }
    }
    fn armor_hurt(&self, _: &mut dyn ServerHost, event: &mut ArmorHurtEvent<'_>) -> Flow {
        assert!(!event.entries.is_empty());
        assert!(event.entries.iter().any(|entry| entry.damage > 0.0));
        self.seen.lock().unwrap().armor.push(Hit::new(
            event.entity,
            event.kind,
            event.source,
            None,
        ));
        Flow::Continue
    }
}

fn observed(cancel: bool, heal: bool, check: impl FnOnce(&mut Game<'_>, &Arc<Mutex<Seen>>)) {
    crate::entry::install_client_hooks();
    let seen = Arc::new(Mutex::new(Seen::default()));
    mod_platform::testing::with_mods(
        vec![ModEntry::new(
            "death-source",
            Box::new(Probe {
                seen: seen.clone(),
                cancel,
                heal,
            }),
        )]
        .into(),
        || {
            with_game(|game| {
                game.build_registries();
                game.load_server_loot();
                game.set_allow_commands(true);
                game.world
                    .chunks
                    .insert((0, 0), Arc::new(Chunk::empty(0, 0)));
                game.block_ticks
                    .add_empty_chunk(ChunkPos::new(0, 0))
                    .unwrap();
                assert!(game.run_command("fill 0 79 0 15 79 15 stone"));
                check(game, &seen);
            })
        },
    );
}

fn player(game: &mut Game<'_>) {
    game.player = Some(PlayerMotion::new(game.motion, Vec3::new(4.5, 80.0, 4.5)).unwrap());
    bind_local_player(game, 11, 123);
    game.set_mode_id(0);
    game.next_entity = 100;
}

fn cancelled_player(heal: bool) {
    observed(true, heal, |game, seen| {
        crate::game::native_player_death::tests::add_guest(game);
        game.lan.as_mut().unwrap().hub.guests.update(0, |guest| {
            guest.health = 0.0;
            guest.loaded = true;
            guest.load_timeout = 0;
            guest.inventory[0] = Some(ItemStack::new("minecraft:diamond", 3));
            guest.vehicle = Some(99);
        });
        game.guest_die(0, 1);
        assert_eq!(seen.lock().unwrap().deaths.len(), 1);
        assert_eq!(
            game.lan.as_ref().unwrap().hub.guests[0].health,
            if heal { 5.0 } else { 0.0 }
        );
        let entity = game.lan.as_ref().unwrap().hub.guests[0].entity_id as u32;
        for tick in 1..=20 {
            // The ordinary server loop asks each tick; cancellation must not redispatch.
            game.guest_die(0, 1);
            if !heal {
                game.tick_guests();
            }
            if heal || tick < 20 {
                let host: &dyn mod_api::Host = game;
                assert!(
                    host.entity(entity).is_some(),
                    "body remains before tickDeath removal"
                );
            }
        }
        let guest = &game.lan.as_ref().unwrap().hub.guests[0];
        assert_eq!(guest.health, if heal { 5.0 } else { 0.0 });
        assert_eq!(
            guest.inventory[0],
            Some(ItemStack::new("minecraft:diamond", 3))
        );
        if !heal {
            assert_eq!(guest.death_time, 20);
            assert!(
                guest.vehicle.is_none(),
                "tickDeath removed the unhealed body from its vehicle"
            );
            let host: &dyn mod_api::Host = game;
            assert!(
                host.entity(guest.entity_id as u32).is_none(),
                "tickDeath removed the body from entity lookup"
            );
        } else {
            assert!(!guest.dead);
            assert_eq!(guest.death_time, 0);
        }
        assert_eq!(seen.lock().unwrap().deaths.len(), 1);
        assert!(game.dropped.is_empty());
    });
}

#[test]
#[ignore = "daily"]
fn cancelled_player_death_preserves_listener_health_five_and_inventory() {
    crate::game::in_own_process(
        "game::native_hooks::death_source_tests::cancelled_player_death_preserves_listener_health_five_and_inventory",
        || cancelled_player(true),
    );
}

#[test]
#[ignore = "daily"]
fn cancelled_unhealed_player_ticks_away_without_drops_or_redispatch() {
    crate::game::in_own_process(
        "game::native_hooks::death_source_tests::cancelled_unhealed_player_ticks_away_without_drops_or_redispatch",
        || cancelled_player(false),
    );
}

#[test]
#[ignore = "daily"]
fn later_cancelled_player_revival_is_transactional_and_a_second_kill_drops_inventory() {
    crate::game::in_own_process(
        "game::native_hooks::death_source_tests::later_cancelled_player_revival_is_transactional_and_a_second_kill_drops_inventory",
        || {
            observed(true, false, |game, seen| {
                crate::game::native_player_death::tests::add_guest(game);
                game.lan.as_mut().unwrap().hub.guests.update(0, |guest| {
                    guest.health = 0.0;
                    guest.loaded = true;
                    guest.load_timeout = 0;
                    guest.inventory[0] = Some(ItemStack::new("minecraft:diamond", 3));
                    guest.carried = Some(ItemStack::new("minecraft:diamond", 2));
                });
                game.guest_die(0, 1);
                game.tick_guests();
                let id = game.lan.as_ref().unwrap().hub.guests[0].entity_id as u32;
                let living = {
                    let host: &mut dyn ServerHost = game;
                    host.living(host.entity(id).unwrap()).unwrap()
                };
                let abort = game.transaction_open();
                assert!(living.set_health(game, 5.0));
                let guest = &game.lan.as_ref().unwrap().hub.guests[0];
                assert_eq!(
                    (guest.health, guest.death_time, guest.death_canceled),
                    (5.0, 0, false)
                );
                game.transaction_close(&abort, false);
                let guest = &game.lan.as_ref().unwrap().hub.guests[0];
                assert_eq!(
                    (guest.health, guest.death_time, guest.death_canceled),
                    (0.0, 1, true)
                );
                game.guest_die(0, 1);
                assert_eq!(
                    seen.lock().unwrap().deaths.len(),
                    1,
                    "abort restored the cancelled attempt"
                );
                assert!(game.dropped.is_empty());
                assert!(living.set_health(game, 5.0));
                let guest = &game.lan.as_ref().unwrap().hub.guests[0];
                assert_eq!(
                    (guest.health, guest.death_time, guest.death_canceled),
                    (5.0, 0, false)
                );
                assert!(!guest.motion.as_ref().unwrap().health.dead);
                assert!(game.run_command("kill @a"));
                game.guest_die(0, 1);
                assert_eq!(seen.lock().unwrap().deaths.len(), 2);
                assert!(game.lan.as_ref().unwrap().hub.guests[0].dead);
                // Inventory drops in die; the cursor belongs to tickDeath at the
                // ordinary twentieth tick, not to the earlier cancelled attempt.
                for tick in 1..20 {
                    game.world.game_time += 1;
                    game.tick_guests();
                    let guest = &game.lan.as_ref().unwrap().hub.guests[0];
                    assert_eq!(guest.death_time, tick);
                    assert!(guest.body_present());
                    assert_eq!(guest.carried, Some(ItemStack::new("minecraft:diamond", 2)));
                }
                game.world.game_time += 1;
                game.tick_guests();
                let guest = &game.lan.as_ref().unwrap().hub.guests[0];
                assert_eq!(guest.death_time, 20);
                assert!(!guest.body_present());
                assert!(guest.carried.is_none());
                assert_eq!(
                    game.dropped
                        .iter()
                        .filter(|drop| &*drop.stack.item == "minecraft:diamond")
                        .map(|drop| drop.stack.count)
                        .sum::<u32>(),
                    5
                );
            })
        },
    );
}
