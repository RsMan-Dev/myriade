//! Endermen: anger from being stared at, teleporting (randomly in daylight,
//! when hit by projectiles or water, and towards a distant target), freezing
//! while looked at, and picking up / placing `enderman_holdable` blocks.
use super::{
    Damage, MobEvent, MobWorld, Species, Who,
    anim::{Poses, State},
    goals::{
        Cx, Float, Goal, HurtByTarget, JUMP, LookAtPlayer, MOVE, MeleeAttack, NearestTarget,
        RandomLookAround, Stroll, TARGET, goal_clone, reduced,
    },
};
use crate::items::{BlockLootRoll, ItemStack};
use horizon_core::{
    geometry::{Aabb, Vec3},
    random::RandomSource,
};
use horizon_world::native_load_context::LoopContext;

/// An enderman or a shulker is about to teleport on its own
/// (`EntityTeleportEvent.EnderEntity`). A mod may move `target` or cancel.
#[inline(never)]
pub fn native_ender_entity_teleport(
    context: &LoopContext,
    entity: u32,
    kind: &str,
    from: [f64; 3],
    target: &mut [f64; 3],
    cancel: &mut bool,
) {
    crate::native_level_hooks::ender_entity_teleport(context, entity, kind, from, target, cancel);
}
/// EndermanFreezeWhenLookedAt
#[derive(Clone, Debug)]
struct FreezeWhenLookedAt;
impl Goal for FreezeWhenLookedAt {
    goal_clone!();
    fn flags(&self) -> u8 {
        JUMP | MOVE
    }
}
/// EndermanLookForPlayerGoal: anger after being stared at for 5 ticks,
/// teleport away when stared at close up, or towards a far target.
#[derive(Clone, Debug)]
struct LookForPlayer {
    pending: bool,
    aggro: i32,
    teleport: i32,
}
impl Goal for LookForPlayer {
    goal_clone!();
    fn flags(&self) -> u8 {
        TARGET
    }
}
/// EndermanTakeBlockGoal
#[derive(Clone, Debug)]
struct TakeBlock;
impl Goal for TakeBlock {
    goal_clone!();
    fn flags(&self) -> u8 {
        0
    }
}
/// EndermanLeaveBlockGoal
#[derive(Clone, Debug)]
struct LeaveBlock;
impl Goal for LeaveBlock {
    goal_clone!();
    fn flags(&self) -> u8 {
        0
    }
}

/// The tool `Enderman.dropCustomDeathLoot` rolls the carried block's drops
/// with: a diamond axe enchanted by `VanillaEnchantmentProviders.ENDERMAN_LOOT_DROP`
/// (`enchantment_provider/enderman_loot_drop.json`, a single Silk Touch I),
/// so the block itself comes back.
pub fn carried_block_tool() -> ItemStack {
    let mut axe = ItemStack::new("minecraft:diamond_axe", 1);
    axe.enchant("minecraft:silk_touch", 1);
    axe
}

/// `Enderman.dropCustomDeathLoot`: `BlockState.getDrops`' parameters for a
/// carried block, `tool` being [`carried_block_tool`] and the enderman `this`.
pub fn carried_block_roll(tool: &ItemStack) -> BlockLootRoll<'_> {
    BlockLootRoll {
        held: Some(tool),
        breaker: true,
        ..Default::default()
    }
}

/// `spawnAtLocation` leaves out empty stacks.
pub fn carried_block_drop_kept(stack: &ItemStack) -> bool {
    stack.count > 0 && &*stack.item != "minecraft:air"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobs::DamageKind;
    use horizon_world::registry::BlockRegistry;
    use std::sync::Arc;


    /// Enderman.hurtServer: a projectile makes a free enderman teleport away
    /// unhurt, but one that rides something (`isPassenger`) takes the hit.
    #[test]
    fn projectiles_hit_a_seated_enderman() {
        for kind in [DamageKind::Arrow, DamageKind::Thrown, DamageKind::Fireball] {
            let mut free = enderman();
            assert!(!on_damage(&mut free, &Damage::of(kind)), "{kind:?}");
            assert_eq!(free.extra.counter2, 64);
            // The arrow handler keeps the arrow flying when the enderman dodges.
            assert!(dodges(&free, &Damage::of(kind)), "{kind:?}");

            let mut seated = enderman();
            seated.extra.passenger = true;
            assert!(on_damage(&mut seated, &Damage::of(kind)), "{kind:?}");
            assert_eq!(seated.extra.counter2, 0, "no teleport while seated");
            assert!(!dodges(&seated, &Damage::of(kind)), "{kind:?}");
        }
    }

    /// Enderman.teleport(x, y, z) fails for a passenger (#4865: one in a boat
    /// played the teleport sound every tick and snapped back to its seat).
    /// `repeatedlyTryToTeleport` returns before drawing; `teleport()` still
    /// draws its three values first.
    #[test]
    #[ignore = "daily"]
    fn a_seated_enderman_never_teleports() {
        let build = |world: &mut crate::world::World, registry: &BlockRegistry| {
            let stone = registry.default_state("minecraft:stone").unwrap();
            for cx in -2..2 {
                for cz in -2..2 {
                    world
                        .chunks
                        .insert((cx, cz), Arc::new(crate::chunk::Chunk::empty(cx, cz)));
                }
            }
            for x in -32..32 {
                for z in -32..32 {
                    world.set(x, -1, z, stone).unwrap();
                }
            }
        };
        crate::mobs::world::with_built_test_world(&[], build, |w, _| {
            let mut events = Vec::new();
            let mut free = enderman();
            assert!((0..64).any(|_| teleport_random(&mut free, w, &mut events)));
            assert!(
                events
                    .iter()
                    .any(|e| matches!(e, MobEvent::EntityEvent { event: 46, .. })),
                "entity event 46: {events:?}"
            );

            // No mod installed: a teleport try draws its three values and nothing else.
            let mut tried = enderman();
            let mut expected = enderman().random;
            teleport_random(&mut tried, w, &mut events);
            expected.next_double();
            expected.next_int_bound(64).unwrap();
            expected.next_double();
            assert_eq!(tried.random.next_int(), expected.next_int());

            let mut seated = enderman();
            seated.extra.passenger = true;
            events.clear();
            assert!(!(0..64).any(|_| teleport_random(&mut seated, w, &mut events)));
            assert!(events.is_empty());
            assert_eq!(seated.position, Vec3::ZERO);

            for tries in [64, 1] {
                let mut seated = enderman();
                seated.extra.passenger = true;
                let mut expected = enderman().random;
                if tries == 1 {
                    expected.next_double();
                    expected.next_int_bound(64).unwrap();
                    expected.next_double();
                }
                seated.extra.counter2 = tries;
                ai_step(&mut seated, w, &mut events);
                assert!(events.is_empty(), "{tries}");
                assert_eq!(seated.random.next_int(), expected.next_int(), "{tries}");
            }
        });
    }

    /// LivingEntity.randomTeleport clamps the target into the world border
    /// (`WorldBorder.clampVec3ToBound`), so an enderman never lands outside
    /// it (#7080). A border of size 10 around the origin ends at x = 5.
    #[test]
    #[ignore = "daily"]
    fn an_enderman_teleports_inside_the_world_border() {
        let build = |world: &mut crate::world::World, registry: &BlockRegistry| {
            let stone = registry.default_state("minecraft:stone").unwrap();
            for cx in -2..2 {
                for cz in -2..2 {
                    world
                        .chunks
                        .insert((cx, cz), Arc::new(crate::chunk::Chunk::empty(cx, cz)));
                }
            }
            for x in -32..32 {
                for z in -32..32 {
                    world.set(x, -1, z, stone).unwrap();
                }
            }
        };
        let border =
            horizon_core::geometry::border::BorderGeometry::new([0.0, 0.0], 10.0, 29_999_984)
                .unwrap();
        crate::mobs::world::with_built_test_world(&[], build, |w, _| {
            let mut free = enderman();
            assert!(teleport_to(&mut free, w, 20.5, 0.0, 0.5, &mut Vec::new()));
            assert_eq!(free.position, Vec3::new(20.5, 0.0, 0.5), "no border");

            let w = MobWorld {
                border: Some(&border),
                ..w.clone()
            };
            let mut bounded = enderman();
            assert!(teleport_to(
                &mut bounded,
                &w,
                20.5,
                0.0,
                -20.5,
                &mut Vec::new()
            ));
            // Mth.clamp to [min, max - 1.0E-5F] on x and z.
            let edge = 5.0 - f64::from(1.0e-5_f32);
            assert_eq!(bounded.position, Vec3::new(edge, 0.0, -5.0));
        });
    }

    /// Enderman.dropCustomDeathLoot (#3513): a killed enderman drops the
    /// block it carries, mined with a silk touch diamond axe, so a grass
    /// block comes back as a grass block (not dirt) and a nylium as itself.
    #[test]
    fn a_killed_enderman_drops_its_carried_block() {
        use crate::items::Items;
        use crate::mobs::drops;
        use crate::random_sequences::RandomSequences;
        use horizon_world::loot::{LootData, PackOverlay};
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/myriade-engine")
            .join(horizon_core::VANILLA_VERSION);
        if !root.join("items.json").exists() || !root.join("data").exists() {
            return;
        }
        let assets = crate::assets::Assets::load(&root).unwrap();
        let registry = assets.registry;
        let facts = crate::blocks::BlockFacts::load(&root, registry).unwrap();
        let defaults = horizon_world::item_defaults::read(&root).unwrap();
        let items = Items::load(&root, registry, &facts, &PackOverlay::NONE, &defaults).unwrap();
        let loot = LootData::load(&root, &PackOverlay::NONE, &defaults).unwrap();
        let death = |mob_drops| drops::Death {
            loot: &loot,
            damage: None,
            player: None,
            player_killed: false,
            luck: 0.0,
            vehicle: None,
            mob_drops,
            world_seed: 0,
        };
        let mut sequences = RandomSequences::default();
        let mut random = horizon_core::random::LegacyRandom::new(0);
        for block in [
            "minecraft:grass_block",
            "minecraft:crimson_nylium",
            "minecraft:tnt",
        ] {
            let mut m = enderman();
            m.extra.carried = registry.default_state(block);
            let (out, _) = drops::die(
                &mut m,
                &death(true),
                &mut sequences,
                &mut crate::mobs::world::level_random(0),
            );
            let dropped: Vec<String> = out
                .into_iter()
                .flat_map(|d| match d {
                    drops::Drop::CarriedBlock(state) => {
                        let tool = carried_block_tool();
                        let roll = carried_block_roll(&tool);
                        let stacks = Some((&mut sequences, 0));
                        let stacks = items.block_drops(registry, state, roll, &mut random, stacks);
                        stacks.into_iter().filter(carried_block_drop_kept).collect()
                    }
                    _ => Vec::new(),
                })
                .map(|s| format!("{} {}", s.item, s.count))
                .collect();
            assert_eq!(dropped, [format!("{block} 1")], "{block}");
        }
        // doMobLoot off: no dropCustomDeathLoot, so no block either.
        let mut m = enderman();
        m.extra.carried = registry.default_state("minecraft:grass_block");
        let (out, _) = drops::die(
            &mut m,
            &death(false),
            &mut sequences,
            &mut crate::mobs::world::level_random(0),
        );
        assert!(
            !out.iter()
                .any(|d| matches!(d, drops::Drop::CarriedBlock(_))),
            "{out:?}"
        );
    }

    /// `EndermanLookForPlayerGoal` finds its player with
    /// `TargetingConditions.forCombat()`, which never picks a spectator: a
    /// spectator staring at an enderman does not start the anger goal.
    #[test]
    #[ignore = "daily"]
    fn a_spectator_staring_does_not_anger_an_enderman() {
        use crate::mobs::world::{test_player, with_test_world};
        let mut outcomes = Vec::new();
        if !with_test_world(&[], |w, _| {
            for spectator in [false, true] {
                let mut world = w.clone();
                let mut m = enderman();
                m.position = Vec3::new(0.5, 64.0, 0.5);
                let mut p = test_player(Vec3::new(2.5, 64.0, 0.5));
                p.spectator = spectator;
                let dx = m.position.x - p.position.x;
                let dy = m.eye_y() - p.eye[1];
                let dz = m.position.z - p.position.z;
                let len = (dx * dx + dy * dy + dz * dz).sqrt();
                p.look = [dx / len, dy / len, dz / len];
                world.player = Some(p);
                m.ai.goals.add(
                    1,
                    LookForPlayer {
                        pending: false,
                        aggro: 0,
                        teleport: 0,
                    },
                );
                let mut lr = crate::health::TestLevel::new(0);
                let mut events = Vec::new();
                let mut cx = Cx {
                    w: &world,
                    level: &mut lr.level(),
                    events: &mut events,
                };
                let mut goals = std::mem::take(&mut m.ai.goals);
                goals.tick(&mut m, &mut cx);
                outcomes.push(goals.is_running("LookForPlayer"));
                m.ai.goals = goals;
            }
        }) {
            return;
        }
        assert_eq!(
            outcomes,
            vec![true, false],
            "a survival player's stare angers it; a spectator's does not"
        );
    }
}
