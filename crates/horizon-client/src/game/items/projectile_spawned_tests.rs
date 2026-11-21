//! `Projectile.applyOnProjectileSpawned` through `Game::spawn_projectile` and
//! the mod host, with enchantments from a world datapack. Expected values come
//! from vanilla's formulas run on copies of the randoms involved.
#![cfg(test)]
use crate::game::native_entity_refs::tests::with_game;
use horizon_client::{
    items::ItemStack,
    mobs::{Mob, Who},
    projectiles::{Projectile, ProjectileKind},
};
use horizon_core::{
    geometry::Vec3,
    random::{EntityRandom, LegacyRandom},
};
use mod_api::Identifier;
use serde_json::{Value, json};

/// A world whose datapack defines `spawn_test:<name>` for each
/// `(name, projectile_spawned effects)`.
fn with_enchantments(defs: &[(&str, Value)], test: impl FnOnce(&mut super::Game<'_>)) {
    with_game(|game| {
        let root = std::env::temp_dir().join(format!(
            "horizon-projectile-spawned-{}-{}",
            std::process::id(),
            defs.iter()
                .map(|(name, _)| *name)
                .collect::<Vec<_>>()
                .join("-")
        ));
        let pack = root.join("datapacks/spawned");
        let definitions = pack.join("data/spawn_test/enchantment");
        std::fs::create_dir_all(&definitions).unwrap();
        std::fs::write(
            pack.join("pack.mcmeta"),
            r#"{"pack":{"description":"projectile_spawned fixture"}}"#,
        )
        .unwrap();
        std::fs::write(
            root.join("datapacks.json"),
            r#"{"enabled":["file/spawned"],"disabled":[]}"#,
        )
        .unwrap();
        for (name, effects) in defs {
            let definition = json!({
                "description": "fixture", "supported_items": "#minecraft:enchantable/bow",
                "weight": 1, "max_level": 3,
                "min_cost": {"base": 1, "per_level_above_first": 0},
                "max_cost": {"base": 1, "per_level_above_first": 0},
                "anvil_cost": 1, "slots": ["mainhand"],
                "effects": {"minecraft:projectile_spawned": effects},
            });
            std::fs::write(
                definitions.join(format!("{name}.json")),
                serde_json::to_vec(&definition).unwrap(),
            )
            .unwrap();
        }
        game.world_root = root.clone();
        let stack = game.reload_datapacks(false, &[]).unwrap();
        let resources = game.options.resources.clone();
        game.world_tables = game
            .stage_world_tables(&stack, false, &resources)
            .unwrap()
            .0;
        for (name, _) in defs {
            assert!(
                game.stations()
                    .enchantments
                    .get(&format!("spawn_test:{name}"))
                    .is_some(),
                "{name} did not load"
            );
        }
        test(game);
        let _ = std::fs::remove_dir_all(root);
    });
}

fn enchanted(item: &str, count: u32, levels: &[(&str, u32)]) -> ItemStack {
    let mut stack = ItemStack::new(item, count);
    stack.data_mut().enchantments = levels
        .iter()
        .map(|(id, level)| ((*id).into(), *level))
        .collect();
    stack
}

fn sound(name: &str) -> Value {
    json!({"type": "minecraft:play_sound", "sound": name,
        "volume": {"type": "minecraft:uniform", "min_inclusive": 0.5, "max_exclusive": 1.0},
        "pitch": {"type": "minecraft:uniform", "min_inclusive": 1.0, "max_exclusive": 2.0}})
}

fn ignite(seconds: f32) -> Value {
    json!({"type": "minecraft:ignite", "duration": seconds})
}

/// The sounds played since capture began, as (event, volume, pitch).
fn played(game: &mut super::Game<'_>) -> Vec<(String, f32, f32)> {
    game.audio
        .capture
        .replace(Vec::new())
        .unwrap_or_default()
        .into_iter()
        .map(|s| {
            assert_eq!(s.source, mod_api::SoundSource::Neutral as usize);
            (s.event, s.volume, s.pitch)
        })
        .collect()
}

/// Each effect's requirements are tested with the level random just before
/// it applies, and only a matching effect draws from the projectile's random:
/// a coin-flip sound draws twice when it plays and not at all when it
/// doesn't, and the next effect's draws follow on.
/// The stack's effects run before the weapon's, in the stack's enchantment
/// order and each enchantment's effect order; an arrow fired from its own
/// item runs that item's effects once.
#[test]
#[ignore = "daily"]
fn stack_then_weapon_in_component_order() {
    let defs = [
        (
            "first",
            json!([{"effect": {"type": "minecraft:all_of", "effects": [
                sound("minecraft:block.note_block.bell"),
                // On a projectile: nothing, and no draws.
                {"type": "minecraft:apply_mob_effect", "to_apply": "minecraft:speed",
                    "min_duration": 1, "max_duration": 9, "min_amplifier": 0, "max_amplifier": 3},
                ignite(2.0),
            ]}}]),
        ),
        (
            "second",
            json!([{"effect": sound("minecraft:block.note_block.chime")}]),
        ),
        (
            "weapon",
            json!([
                {"effect": sound("minecraft:block.note_block.flute")},
                {"effect": ignite(5.0)},
            ]),
        ),
    ];
    with_enchantments(&defs, |game| {
        let ammo = enchanted(
            "minecraft:arrow",
            1,
            &[("spawn_test:first", 1), ("spawn_test:second", 1)],
        );
        let bow = enchanted("minecraft:bow", 1, &[("spawn_test:weapon", 1)]);
        let arrow = |weapon: &ItemStack| {
            let mut p = Projectile::new(
                ProjectileKind::Arrow {
                    base_damage: 2.0,
                    critical: false,
                    effect: None,
                },
                Vec3::new(0.5, 70.0, 0.5),
                Some(Who::Player),
            )
            .with_entity_random(EntityRandom::own(LegacyRandom::new(7)));
            let a = p.arrow.as_mut().unwrap();
            a.set_item(&ammo);
            a.fired_from_weapon = Some(weapon.clone());
            p
        };
        let names = |sounds: Vec<(String, f32, f32)>| -> Vec<String> {
            sounds.into_iter().map(|(name, _, _)| name).collect()
        };
        let order: Vec<String> = ammo
            .enchantments()
            .iter()
            .map(|(id, _)| match &**id {
                "spawn_test:first" => "minecraft:block.note_block.bell",
                _ => "minecraft:block.note_block.chime",
            })
            .chain(["minecraft:block.note_block.flute"])
            .map(str::to_owned)
            .collect();
        game.audio.capture = Some(Vec::new());
        game.spawn_projectile(arrow(&bow), &ammo);
        assert_eq!(names(played(game)), order);
        // The weapon is the ammo's own item: its effects run once.
        game.spawn_projectile(arrow(&ammo), &ammo);
        assert_eq!(names(played(game)).len(), 2);
    });
}

/// The mod host's owner launch applies the supplied stack's effects once,
/// whatever its count, leaves the stack alone, and refuses a stack with an
/// effect the engine does not apply yet before drawing anything.
#[test]
#[ignore = "daily"]
fn owner_launch_applies_the_supplied_stacks_effects() {
    let defs = [
        ("hot", json!([{"effect": ignite(4.0)}])),
        (
            "flags",
            json!([{"requirements": {"condition":"minecraft:entity_properties", "entity":"this", "predicate":{"minecraft:flags":{"is_on_fire":true}}}, "effect": ignite(4.0)}]),
        ),
        (
            "bad_clamp",
            json!([{"effect":{"type":"minecraft:ignite", "duration":{"type":"minecraft:clamped", "value":1,"min":2,"max":0}}}]),
        ),
        (
            "boom",
            json!([{"effect": {"type": "minecraft:explode", "radius": 1,
            "block_interaction": "none", "small_particle": {"type": "minecraft:explosion"},
            "large_particle": {"type": "minecraft:explosion_emitter"},
            "sound": "minecraft:entity.generic.explode"}}]),
        ),
    ];
    with_enchantments(&defs, |game| {
        let kind = game
            .res
            .mob_types()
            .get("minecraft:zombie")
            .unwrap()
            .clone();
        game.mobs
            .push(Mob::new(21, kind, Vec3::new(0.0, 64.0, 0.0), 0.0));
        let source = game.entity(21).unwrap();
        let owner = game.living(source).unwrap();
        let snowball = Identifier::parse("minecraft:snowball").unwrap();
        let stack = enchanted("minecraft:snowball", 16, &[("spawn_test:hot", 2)]);
        let shot = game
            .spawn_projectile_from_rotation(owner, &snowball, &stack, 0.0, 1.5, 1.0)
            .unwrap();
        let p = game.ecs.projectile_view(shot.entity().id()).unwrap();
        assert!((79..=80).contains(&p.fire_ticks));
        assert_eq!(p.item.as_ref().unwrap().count, 1);
        assert_eq!(stack.count, 16);
        let boom = enchanted("minecraft:snowball", 1, &[("spawn_test:boom", 1)]);
        let count = game.ecs.entity_count();
        let mut before = game.world.random.clone();
        assert!(
            game.spawn_projectile_from_rotation(owner, &snowball, &boom, 0.0, 1.5, 1.0)
                .is_none()
        );
        assert_eq!(game.ecs.entity_count(), count);
        assert_eq!(game.world.random.next_long(), before.next_long());
        for enchantment in ["spawn_test:flags", "spawn_test:bad_clamp"] {
            let invalid = enchanted("minecraft:snowball", 1, &[(enchantment, 1)]);
            let mut before = game.world.random.clone();
            assert!(
                game.spawn_projectile_from_rotation(owner, &snowball, &invalid, 0.0, 1.5, 1.0)
                    .is_none()
            );
            assert_eq!(game.ecs.entity_count(), count);
            assert_eq!(game.world.random.next_long(), before.next_long());
        }
    });
}
