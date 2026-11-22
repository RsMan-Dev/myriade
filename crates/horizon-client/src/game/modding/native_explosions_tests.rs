//! `LevelRef::explode`'s contract through both mod targets: a native call of the handle method
//! and a wasm guest's call through the generated forwarder, each on a fresh game, on the stack
//! a wasm guest runs on. The source and interactions are arismoko/horizon#3929's; the damage type, the
//! calculator, the effects and the unbounded radius arismoko/horizon#4009's.
#![cfg(test)]
use crate::game::Game;
use crate::game::native_entity_refs::tests::{bind_local_player, run_on_guest_stack, with_game};
use horizon_client::{chunk::Chunk, mobs::Who, motion::PlayerMotion};
use horizon_core::{
    geometry::Vec3,
    position::{BlockPos, ChunkPos},
};
use mod_api::{
    EntityRef, ExplodeError, ExplosionCalculator, ExplosionDetonateEvent, ExplosionInteraction,
    ExplosionSettings, ExplosionStartEvent, Flow, Hooks, LevelRandomHost, LivingDamagePostEvent,
    LivingDeathEvent, ModEntry, ServerHost,
    wire::{Reader, Writer},
};
use std::sync::{Arc, Mutex};

/// How the call reaches the game.
#[derive(Clone, Copy, Debug)]
enum Via {
    Native,
    Wasm,
}
const BOTH: [Via; 2] = [Via::Native, Via::Wasm];

/// One `explode` call.
struct Blast {
    source: Option<EntityRef>,
    center: [f64; 3],
    radius: f32,
    fire: bool,
    interaction: ExplosionInteraction,
    settings: ExplosionSettings,
}
impl Blast {
    /// A sourceless `Block` blast without fire and with vanilla's settings.
    fn at(center: [f64; 3], radius: f32) -> Self {
        Self {
            source: None,
            center,
            radius,
            fire: false,
            interaction: ExplosionInteraction::Block,
            settings: ExplosionSettings::default(),
        }
    }
}

/// What `explode` answers.
type Answer = Result<(), ExplodeError>;

/// Runs `blast` in the overworld through `via` and checks its answer.
fn explode(game: &mut Game<'_>, via: Via, blast: Blast, expected: Answer) {
    let level = game.level("minecraft:overworld").unwrap();
    match via {
        Via::Native => assert_eq!(
            level.explode(
                game,
                blast.source,
                blast.center,
                blast.radius,
                blast.fire,
                blast.interaction,
                blast.settings
            ),
            expected
        ),
        Via::Wasm => wasm_explode(game, level, blast, expected),
    }
}

/// A guest whose server tick calls `level_explode` through its import and
/// answers the tick only when the host's answer is `expected` (it traps otherwise).
fn wasm_explode(game: &mut Game<'_>, level: mod_api::LevelRef, blast: Blast, expected: Answer) {
    let mut input = Writer::new();
    input.put(&level);
    input.put(&blast.source);
    input.put(&blast.center);
    input.put(&blast.radius);
    input.put(&blast.fire);
    input.put(&blast.interaction);
    input.put(&blast.settings);
    let mut answer = Writer::new();
    answer.put(&expected);
    let mut reply = Writer::new();
    reply.put(&mod_api::ServerTickPhase::ServerTickEnd {
        world_running: true,
    });
    let escape = |bytes: &[u8]| {
        bytes
            .iter()
            .map(|b| format!("\\{b:02x}"))
            .collect::<String>()
    };
    let check = answer
        .bytes()
        .iter()
        .enumerate()
        .map(|(i, byte)| {
            format!(
                "i32.const {} i32.load8_u i32.const {byte} i32.ne if unreachable end ",
                32768 + i
            )
        })
        .collect::<String>();
    let id = horizon_mod_wasm::fixture::import_id("BackendMut", "level_explode").unwrap();
    let bytes = wat::parse_str(horizon_mod_wasm::fixture::by_name(&format!(
        r#"(module
          (@custom "dylink.0" "\01\04\00\00\00\00")
          (import "env" "memory" (memory 1))
          (import "env" "__indirect_function_table" (table 1 funcref))
          (import "horizon" "host" (func $host (param i32 i32 i32 i32 i32) (result i32)))
          (import "horizon" "host_reply" (func (param i32)))
          (func (export "horizon_layout") (result i64) i64.const {layout})
          (data (i32.const 4096) "{input}")
          (data (i32.const 16384) "{reply}")
          (func (export "horizon_in") (param i32) (result i32) i32.const 512)
          (func (export "horizon_hook") (param i32) (result i64)
            i32.const {id} i32.const 4096 i32.const {input_len} i32.const 32768 i32.const 4096 call $host drop
            {check}
            i64.const {reply_at}))"#,
        layout = *mod_api::wire::LAYOUT,
        input = escape(input.bytes()),
        reply = escape(reply.bytes()),
        input_len = input.bytes().len(),
        reply_at = (16384u64 << 32) | reply.bytes().len() as u64,
    ), &[("BackendMut", "level_explode")]))
    .unwrap();
    let module = horizon_mod_wasm::fixture::module(&bytes).unwrap();
    let mut context = Writer::new();
    context.put(&mod_api::Authority::DedicatedServer);
    context.put(&level.epoch());
    context.put(&mod_api::TickId::new(1));
    let context: mod_api::LifecycleContext = Reader::new(context.bytes()).take().unwrap();
    let mut tick = mod_api::ServerTickEvent {
        context: &context,
        phase: mod_api::ServerTickPhase::ServerTickStart {
            world_running: true,
        },
    };
    Hooks::server_tick(&module, game, &mut tick);
    assert!(
        matches!(
            tick.phase,
            mod_api::ServerTickPhase::ServerTickEnd {
                world_running: true
            }
        ),
        "the guest saw the answer {expected:?}"
    );
}

/// A server's overworld chunk (0, 0) with a stone floor at y 79.
fn floor(game: &mut Game<'_>) {
    game.build_registries();
    game.load_server_loot();
    game.set_allow_commands(true);
    game.options.dedicated = Some(Vec::new());
    game.world
        .chunks
        .insert((0, 0), Arc::new(Chunk::empty(0, 0)));
    game.block_ticks
        .add_empty_chunk(ChunkPos::new(0, 0))
        .unwrap();
    assert!(game.run_command("fill 0 79 0 15 79 15 stone"));
}

fn name(game: &Game<'_>, pos: [i32; 3]) -> String {
    let state = game.world.block(pos[0], pos[1], pos[2]);
    game.res.facts().get(state).name.to_string()
}

/// How many floor blocks are still stone.
fn stone(game: &Game<'_>) -> usize {
    (0..16)
        .flat_map(|x| (0..16).map(move |z| [x, 79, z]))
        .filter(|&p| name(game, p) == "minecraft:stone")
        .count()
}

/// What `living_death` heard: the dying entity, the damage type and the causing entity.
type Deaths = Arc<Mutex<Vec<(u32, String, i32)>>>;
struct Deathwatch(Deaths);
#[mod_api::__subscriptions]
impl Hooks for Deathwatch {
    fn living_death(&self, _: &mut dyn ServerHost, event: &mut LivingDeathEvent<'_>) -> Flow {
        let source = event.source;
        self.0
            .lock()
            .unwrap()
            .push((event.entity, source.damage_type.into(), source.causing));
        Flow::Continue
    }
}

/// Primed TNT lit by `owner` beside the zombie.
fn tnt(game: &mut Game<'_>, owner: Option<Who>) -> u32 {
    game.add_primed_tnt(BlockPos::new(4, 80, 4), 0.0, 80, owner, None);
    *game.ecs.species_ids("minecraft:tnt").last().unwrap()
}

/// A summoned entity's id.
fn summoned(game: &mut Game<'_>, kind: &str, nbt: &str) -> u32 {
    let target = game
        .summon_with_nbt(kind, [4.5, 80.5, 4.5], nbt)
        .unwrap()
        .unwrap();
    use crate::game::commands::Target;
    match target {
        Target::Stand(id) | Target::Trident(id) | Target::Rocket(id) => id,
        other => panic!("{kind} summoned as {other:?}"),
    }
}

#[test]
#[ignore = "daily"]
fn a_trigger_blast_presses_a_button_and_breaks_nothing() {
    run_on_guest_stack(|| {
        for via in BOTH {
            with_game(|game| {
                floor(game);
                assert!(game.run_command("setblock 6 80 4 minecraft:stone_button[face=floor]"));
                let blast = Blast {
                    source: None,
                    center: [4.5, 80.5, 4.5],
                    radius: 3.0,
                    fire: false,
                    interaction: ExplosionInteraction::Trigger,
                    settings: ExplosionSettings::default(),
                };
                explode(game, via, blast, Ok(()));
                let state = game.world.block(6, 80, 4);
                let powered = game
                    .pack
                    .assets
                    .registry
                    .state(state)
                    .and_then(|s| s.properties.get("powered").cloned());
                assert_eq!(name(game, [6, 80, 4]), "minecraft:stone_button", "{via:?}");
                assert_eq!(powered.as_deref(), Some("true"), "{via:?}");
                assert_eq!(stone(game), 256, "{via:?}");
            });
        }
    });
}

#[test]
#[ignore = "daily"]
fn a_blast_that_keeps_blocks_still_lights_fire() {
    run_on_guest_stack(|| {
        for via in BOTH {
            with_game(|game| {
                floor(game);
                let blast = Blast {
                    source: None,
                    center: [8.5, 80.5, 8.5],
                    radius: 4.0,
                    fire: true,
                    interaction: ExplosionInteraction::None,
                    settings: ExplosionSettings::default(),
                };
                explode(game, via, blast, Ok(()));
                let fires = (0..16)
                    .flat_map(|x| (0..16).map(move |z| [x, 80, z]))
                    .filter(|&p| name(game, p) == "minecraft:fire")
                    .count();
                assert!(fires > 0, "{via:?}");
                assert_eq!(stone(game), 256, "{via:?}");
            });
        }
    });
}

/// What a blast's hooks heard: the damage type `explosion_start` names, how
/// often `explosion_detonate` ran, and each `living_damage` (post) with its
/// entity and damage type.
#[derive(Default)]
struct Heard {
    starts: Vec<String>,
    detonates: usize,
    damage: Vec<(i64, String)>,
}
struct Listener(Arc<Mutex<Heard>>);
#[mod_api::__subscriptions]
impl Hooks for Listener {
    fn explosion_start(&self, _: &mut dyn ServerHost, event: &mut ExplosionStartEvent<'_>) -> Flow {
        let damage_type = event.explosion.damage_type.to_owned();
        self.0.lock().unwrap().starts.push(damage_type);
        Flow::Continue
    }
    fn explosion_detonate(&self, _: &mut dyn ServerHost, _: &mut ExplosionDetonateEvent<'_>) {
        self.0.lock().unwrap().detonates += 1;
    }
    fn living_damage_post(
        &self,
        _: &mut dyn LevelRandomHost,
        event: &mut LivingDamagePostEvent<'_>,
    ) {
        let heard = (i64::from(event.entity), event.source.damage_type.to_owned());
        self.0.lock().unwrap().damage.push(heard);
    }
}

/// Runs `test` on a fresh game with a stone floor through both targets, the
/// [`Listener`] hearing it, and hands it what was heard.
fn heard(test: impl Fn(&mut Game<'_>, Via, &Arc<Mutex<Heard>>) + Send + Sync + 'static) {
    crate::entry::install_client_hooks();
    run_on_guest_stack(move || {
        for via in BOTH {
            let heard = Arc::new(Mutex::new(Heard::default()));
            let mods = vec![ModEntry::new(
                "explode-heard",
                Box::new(Listener(heard.clone())),
            )];
            mod_platform::testing::with_mods(mods.into(), || {
                with_game(|game| {
                    floor(game);
                    test(game, via, &heard);
                })
            });
        }
    });
}

/// The game's damage types plus `test:fuel_tank`, whose death message is
/// `death.attack.fuelTank` and which is in `#minecraft:is_explosion`.
fn register_fuel_tank(game: &mut Game<'_>) {
    use serde_json::{Value, json};
    let old = &game.health_resources.damage;
    let mut types: Vec<Value> = old
        .entries()
        .map(|(_, d)| {
            json!({
                "id": d.name, "message_id": d.message_id,
                "exhaustion": format!("{:08x}", d.exhaustion.to_bits()),
                "scaling": d.scaling.name(), "effects": d.effects,
                "death_message_type": d.death_message_type,
            })
        })
        .collect();
    types.push(json!({
        "id": "test:fuel_tank", "message_id": "fuelTank", "exhaustion": "3dcccccd",
        "scaling": "never", "effects": "hurt", "death_message_type": "default",
    }));
    let mut tags = serde_json::Map::new();
    for tag in old.tags() {
        let mut values: Vec<&str> = old
            .entries()
            .filter(|&(id, _)| old.in_tag(id, tag))
            .map(|(_, d)| d.name.as_str())
            .collect();
        if tag == "minecraft:is_explosion" {
            values.push("test:fuel_tank");
        }
        tags.insert(tag.into(), json!({ "values": values }).to_string().into());
    }
    let types = json!({ "data_version": horizon_world::DATA_VERSION, "types": types });
    game.health_resources.damage =
        horizon_world::damage_source::Catalog::from_json(&types, &Value::Object(tags)).unwrap();
}

#[test]
#[ignore = "daily"]
fn immune_blocks_survive_and_every_other_cell_resists_nothing() {
    for immune in [Some("minecraft:logs"), None] {
        heard(move |game, via, _| {
            assert!(game.run_command("setblock 8 80 10 minecraft:oak_log"));
            assert!(game.run_command("setblock 8 80 6 minecraft:obsidian"));
            let blast = Blast {
                settings: ExplosionSettings {
                    calculator: Some(ExplosionCalculator {
                        immune_blocks: immune.map(str::to_owned),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                ..Blast::at([8.5, 80.5, 8.5], 6.0)
            };
            explode(game, via, blast, Ok(()));
            let (log, obsidian) = (name(game, [8, 80, 10]), name(game, [8, 80, 6]));
            assert_eq!(
                name(game, [8, 79, 8]),
                "minecraft:air",
                "{via:?} {immune:?}"
            );
            if immune.is_some() {
                // The tag replaces the rule: obsidian's 1200 counts for nothing.
                assert_eq!(
                    (log.as_str(), obsidian.as_str()),
                    ("minecraft:oak_log", "minecraft:air"),
                    "{via:?}"
                );
            } else {
                assert_eq!(
                    (log.as_str(), obsidian.as_str()),
                    ("minecraft:air", "minecraft:obsidian"),
                    "{via:?}"
                );
            }
        });
    }
}

#[test]
#[ignore = "daily"]
fn a_radius_of_48_breaks_a_block_40_blocks_from_the_centre() {
    heard(move |game, via, _| {
        for x in 1..4 {
            game.world
                .chunks
                .insert((x, 0), Arc::new(Chunk::empty(x, 0)));
            game.block_ticks
                .add_empty_chunk(ChunkPos::new(x, 0))
                .unwrap();
        }
        // The ray along (1, 1/15, 1/15) from the centre crosses this block
        // 39.7 blocks out.
        assert!(game.run_command("setblock 48 103 11 minecraft:dirt"));
        explode(game, via, Blast::at([8.5, 100.5, 8.5], 48.0), Ok(()));
        assert_eq!(name(game, [48, 103, 11]), "minecraft:air", "{via:?}");
    });
}

/// A nether portal block at (8, 80, 10) between obsidian on its axis (x) and
/// above and below, which nothing around it changes.
fn portal(game: &mut Game<'_>) {
    for pos in ["7 80 10", "9 80 10", "8 81 10", "8 79 10"] {
        assert!(game.run_command(&format!("setblock {pos} minecraft:obsidian")));
    }
    assert!(game.run_command("setblock 8 80 10 minecraft:nether_portal[axis=x]"));
}

#[test]
#[ignore = "daily"]
fn primed_tnt_that_went_through_a_portal_spares_a_portal_unless_a_calculator_is_passed() {
    // (source, calculator, the portal survives)
    let cases = [
        (true, false, true),
        (false, false, false),
        (true, true, false),
    ];
    for (source, calculator, survives) in cases {
        heard(move |game, via, _| {
            portal(game);
            let source = source.then(|| {
                let id = tnt(game, None);
                game.ecs_set_tnt_used_portal(id, true);
                game.entity(id).unwrap()
            });
            let blast = Blast {
                source,
                settings: ExplosionSettings {
                    calculator: calculator.then(ExplosionCalculator::default),
                    ..Default::default()
                },
                ..Blast::at([8.5, 80.5, 8.5], 4.0)
            };
            explode(game, via, blast, Ok(()));
            let case = (source.is_some(), calculator);
            assert_eq!(name(game, [8, 79, 8]), "minecraft:air", "{via:?} {case:?}");
            let expected = if survives {
                "minecraft:nether_portal"
            } else {
                "minecraft:air"
            };
            assert_eq!(name(game, [8, 80, 10]), expected, "{via:?} {case:?}");
            assert_eq!(
                name(game, [7, 80, 10]),
                "minecraft:obsidian",
                "{via:?} {case:?}"
            );
        });
    }
}

#[test]
#[ignore = "daily"]
fn a_wind_charge_source_brings_its_tag_calculator_unless_a_calculator_is_passed() {
    use horizon_client::projectiles::{Projectile, ProjectileKind};
    // (source, calculator, obsidian survives)
    let cases = [
        (true, false, false),
        (false, false, true),
        (true, true, true),
    ];
    for (source, calculator, survives) in cases {
        heard(move |game, via, _| {
            assert!(game.run_command("setblock 8 80 10 minecraft:obsidian"));
            assert!(game.run_command("setblock 8 80 6 minecraft:bedrock"));
            let source = source.then(|| {
                let charge =
                    Projectile::new(ProjectileKind::WindCharge, Vec3::new(4.5, 80.5, 4.5), None);
                let id = game.add_projectile(charge);
                game.entity(id).unwrap()
            });
            let blast = Blast {
                source,
                settings: ExplosionSettings {
                    calculator: calculator.then(ExplosionCalculator::default),
                    ..Default::default()
                },
                ..Blast::at([8.5, 80.5, 8.5], 4.0)
            };
            explode(game, via, blast, Ok(()));
            let case = (source.is_some(), calculator);
            assert_eq!(name(game, [8, 79, 8]), "minecraft:air", "{via:?} {case:?}");
            // `#blocks_wind_charge_explosions` (barrier, bedrock) stands either way.
            assert_eq!(
                name(game, [8, 80, 6]),
                "minecraft:bedrock",
                "{via:?} {case:?}"
            );
            let expected = if survives {
                "minecraft:obsidian"
            } else {
                "minecraft:air"
            };
            assert_eq!(name(game, [8, 80, 10]), expected, "{via:?} {case:?}");
        });
    }
}

#[test]
#[ignore = "daily"]
fn a_non_finite_radius_is_refused_and_one_of_zero_finds_no_blocks() {
    heard(move |game, via, heard| {
        for radius in [f32::NAN, f32::INFINITY] {
            let blast = Blast::at([8.5, 80.5, 8.5], radius);
            explode(game, via, blast, Err(ExplodeError::InvalidBlast));
        }
        assert!(heard.lock().unwrap().starts.is_empty(), "{via:?}");
        // As in vanilla: the start hook, no blocks, no detonate.
        explode(game, via, Blast::at([8.5, 79.5, 8.5], 0.0), Ok(()));
        assert_eq!(stone(game), 256, "{via:?}");
        let heard = heard.lock().unwrap();
        assert_eq!((heard.starts.len(), heard.detonates), (1, 0), "{via:?}");
    });
}
