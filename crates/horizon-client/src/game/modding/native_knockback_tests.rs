//! `living_knockback` heard by a trait mod through the game's own bridge: who the hook names
//! as the attacker, what the attacker holds, what a mod may cancel, and thorns' wearer.
use super::entity::damage_level_of;
use super::native_entity_refs::tests::with_game;
use horizon_client::health::{Attack, Hurting, Knock, NO_ATTACK};
use horizon_client::items::ItemStack;
use horizon_client::mobs::{Damage, DamageKind, Mob, Who};
use horizon_core::{geometry::Vec3, random::RandomSource};
use mod_api::{
    self, Hooks, HurtSource, LevelRandomHost, LivingIncomingDamageEvent, LivingKnockbackEvent,
};
use std::sync::{Arc, Mutex};

/// What a knockback heard: the attacker and the stacks it holds (by item id).
type Held = (i32, Option<String>, Option<String>);

#[derive(Default)]
struct Heard {
    knocks: Vec<Held>,
    /// The entry of `LivingEntity.knockback` each push came through, as the hook named it.
    entries: Vec<mod_api::KnockbackEntry>,
    incoming_causing: Vec<i32>,
    cancel: bool,
    repeat_short: bool,
}

struct Probe(Arc<Mutex<Heard>>);

#[mod_api::__subscriptions]
impl Hooks for Probe {
    fn living_knockback(
        &self,
        _: &mut dyn LevelRandomHost,
        event: &mut LivingKnockbackEvent<'_>,
    ) -> mod_api::Flow {
        let mut heard = self.0.lock().unwrap();
        let id = |s: Option<&mod_api::ItemStack>| {
            s.and_then(|s| mod_api::__private::item_key_id(s.item()).map(str::to_owned))
        };
        heard.knocks.push((
            event.attacker,
            id(event.attacker_main),
            id(event.attacker_off),
        ));
        heard.entries.push(event.entry);
        event.strength *= 2.0;
        if heard.repeat_short && event.entry == mod_api::KnockbackEntry::Short {
            event.strength *= 2.0;
        }
        event.x = -event.x;
        if heard.cancel {
            return mod_api::Flow::Cancel;
        }
        mod_api::Flow::Continue
    }
    fn living_incoming_damage(
        &self,
        _: &mut dyn LevelRandomHost,
        event: &mut LivingIncomingDamageEvent<'_>,
    ) -> mod_api::Flow {
        self.0
            .lock()
            .unwrap()
            .incoming_causing
            .push(event.source.causing);
        mod_api::Flow::Continue
    }
}

fn heard(test: impl FnOnce(&mut super::Game<'_>, &Mutex<Heard>)) -> Heard {
    let log = Arc::new(Mutex::new(Heard::default()));
    let mods: mod_api::Mods = vec![mod_api::ModEntry::new(
        "probe",
        Box::new(Probe(log.clone())),
    )]
    .into();
    mod_platform::testing::with_mods(mods, || with_game(|game| test(game, &log)));
    std::mem::take(&mut *log.lock().unwrap())
}

fn source(causing: i32) -> HurtSource<'static> {
    HurtSource {
        damage_type: "minecraft:player_attack",
        causing,
        direct: 0,
        position: None,
    }
}

const PUSH: Knock = Knock {
    strength: 0.4,
    x: 1.0,
    z: 0.0,
};

#[test]
#[ignore = "daily"]
fn the_bridge_names_the_attacker_and_what_it_holds() {
    let heard = heard(|game, _| {
        let mob = {
            let kind = game
                .res
                .mob_types()
                .get("minecraft:zombie")
                .unwrap()
                .clone();
            let mut mob = Mob::new(12, kind, Vec3::ZERO, 0.0);
            mob.ai.last_hurt_by = Some(Who::Mob(99));
            mob
        };
        let by_player = Damage {
            attacker: Some(Who::Player),
            ..Damage::of(DamageKind::Generic)
        };
        let by_mob = Damage {
            attacker: Some(Who::Mob(3)),
            ..by_player
        };
        let by_nobody = Damage {
            attacker: None,
            ..by_player
        };
        let mut push = |pushed: Hurting<'_>, attack: &Attack| {
            let mut knock = PUSH;
            let canceled = super::native_hooks::knockback(
                damage_level_of!(game, attack).reborrow(),
                pushed,
                &mut knock,
            );
            (canceled, knock)
        };
        let by_seven = Attack {
            attacker: 7,
            projectile: None,
            direct: None,
            player_uuid: None,
            held: Default::default(),
            ..Default::default()
        };
        for pushed in [
            Hurting::Mob(&mob, &by_player),
            Hurting::Player(12, source(0)),
        ] {
            // The probe doubles the strength and mirrors the direction.
            let (canceled, knock) = push(pushed, &by_seven);
            assert!(!canceled);
            assert_eq!(
                knock,
                Knock {
                    strength: 0.8,
                    x: -1.0,
                    z: 0.0
                }
            );
        }
        push(Hurting::Mob(&mob, &by_mob), &by_seven);
        push(Hurting::Mob(&mob, &by_nobody), &by_seven);
        push(Hurting::Dragon(12, source(5)), &by_seven);
        push(Hurting::ArmorStand(12, source(0)), &by_seven);
        let sword = Attack {
            attacker: 7,
            projectile: None,
            direct: None,
            player_uuid: None,
            held: [Some(ItemStack::new("minecraft:iron_sword", 1)), None],
            ..Default::default()
        };
        push(Hurting::Player(12, source(0)), &sword);
        push(Hurting::Player(12, source(0)), &NO_ATTACK);
    });
    let sword = Some("minecraft:iron_sword".to_string());
    let attackers = [7, 7, 3, 99, 5, 7, 7, 0].map(|attacker| (attacker, None, None));
    let mut expected = attackers.to_vec();
    expected[6].1 = sword;
    assert_eq!(heard.knocks, expected);
}

#[test]
#[ignore = "daily"]
fn a_canceled_push_is_reported() {
    let heard = heard(|game, log| {
        let level = damage_level_of!(game);
        log.lock().unwrap().cancel = true;
        let mut knock = PUSH;
        assert!(super::native_hooks::knockback(
            level.reborrow(),
            Hurting::Player(12, source(7)),
            &mut knock
        ));
    });
    assert_eq!(heard.knocks.len(), 1);
}

fn player_with_sword(game: &mut super::Game<'_>) {
    use horizon_client::motion::PlayerMotion;
    let mut player = PlayerMotion::new(game.motion, Vec3::ZERO).unwrap();
    player.entity_id = 7;
    game.player = Some(player);
    game.inventory[game.selected] = Some(ItemStack::new("minecraft:iron_sword", 1));
}

/// The attack the player-victim path lands in is lent by `with_attack`, restored when it
/// returns, and not seen across a real guest swap (the guest's combat tracker has its own).
#[test]
#[ignore = "daily"]
fn the_attack_scope_nests_restores_and_a_guest_swap_hides_it() {
    // TODO(phase 2): joueur agissant = invité (with_acting_guest ne suffit pas : le test emboîte un tour de l'invité 12 dans l'attaque du joueur hôte, ce qui fait un tour imbriqué sur le même invité)
    let log = Arc::new(Mutex::new(Heard::default()));
    let mods: mod_api::Mods = vec![mod_api::ModEntry::new(
        "probe",
        Box::new(Probe(log.clone())),
    )]
    .into();
    mod_platform::testing::with_mods(mods, || {
        super::native_guest_turn_tests::with_guests(|game| {
            player_with_sword(game);
            let outer = game.attack_of(7);
            assert_eq!(
                outer.held[0],
                Some(ItemStack::new("minecraft:iron_sword", 1))
            );
            game.with_attack(outer.clone(), |game| {
                assert_eq!(game.current_attack(), outer);
                let seen_by_guest = game
                    .as_guest(12, |game| {
                        assert_eq!(game.current_attack(), NO_ATTACK, "the outer attack leaked");
                        let inner = Attack {
                            attacker: 9,
                            projectile: None,
                            direct: None,
                            player_uuid: None,
                            held: Default::default(),
                            ..Default::default()
                        };
                        game.with_attack(inner.clone(), |game| {
                            assert_eq!(game.current_attack(), inner);
                        });
                        game.current_attack()
                    })
                    .unwrap();
                assert_eq!(seen_by_guest, NO_ATTACK);
                assert_eq!(game.current_attack(), outer, "the swap did not restore it");
            });
            assert_eq!(game.current_attack(), NO_ATTACK, "the scope leaked");
        });
    });
}

/// A guest's blow on another guest lands through `pvp_land`: the hook names the attacking guest
/// and the sword in its hand, though the victim is the player swapped in when the push runs.
#[test]
#[ignore = "daily"]
fn a_guest_blow_on_a_guest_names_the_attacker_and_the_sword() {
    let log = Arc::new(Mutex::new(Heard::default()));
    let mods: mod_api::Mods = vec![mod_api::ModEntry::new(
        "probe",
        Box::new(Probe(log.clone())),
    )]
    .into();
    mod_platform::testing::with_mods(mods, || {
        super::native_guest_turn_tests::with_guests(|game| {
            game.lan
                .as_mut()
                .unwrap()
                .hub
                .update_guest(12, |guest| {
                    guest.inventory[guest.selected] =
                        Some(ItemStack::new("minecraft:iron_sword", 1));
                })
                .unwrap();
            let mut victim =
                horizon_client::motion::PlayerMotion::new(game.motion, Vec3::new(3.0, 64.0, 5.0))
                    .unwrap();
            victim.entity_id = 13;
            game.lan
                .as_mut()
                .unwrap()
                .hub
                .update_guest(13, |victim_guest| {
                    victim_guest.motion = Some(Box::new(victim));
                    // The guest has finished loading in.
                    victim_guest.load_timeout = 0;
                })
                .unwrap();
            // A fresh body is a creative one: change the mode away and back so the survival
            // abilities apply to it.
            game.as_guest(13, |g| {
                use horizon_client::command::args::GameMode;
                g.set_game_mode(GameMode::Creative);
                g.set_game_mode(GameMode::Survival);
            })
            .unwrap();
            let strike = super::lan_pvp::Strike::test_blow(12);
            let hurt = game.pvp_land(super::lan_pvp::Victim::Guest(13), &strike);
            let seen = log.lock().unwrap().incoming_causing.clone();
            assert!(hurt, "not hurt; damage heard from {seen:?}");
            assert_eq!(game.current_attack(), NO_ATTACK);
        });
    });
    let heard = std::mem::take(&mut *log.lock().unwrap());
    let sword = Some("minecraft:iron_sword".to_string());
    assert!(
        !heard.knocks.is_empty(),
        "no push reached the hook; damage heard from {:?}",
        heard.incoming_causing
    );
    assert!(
        heard.knocks.iter().all(|k| *k == (12, sword.clone(), None)),
        "heard {:?}",
        heard.knocks
    );
}
