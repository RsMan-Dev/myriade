use crate::projectiles::{Projectile, ProjectileWorld, STEPS};
use horizon_core::geometry::trig::Mth;
use horizon_core::{
    geometry::{Aabb, Direction, ShapeOp, Vec3, VoxelShape},
    random::{LevelRandom, RandomSource},
};
use std::f32::consts::PI;

// Shulker state: `variant3` the attach face (3D data value), `counter` the
// raw peek (0..100), `color` the shell colour (16 none). AnimState slot 0
// the current peek amount. On the client, `scratch[0]` is
// `clientSideTeleportInterpolation` and `scratch[1..3]` is
// `clientOldAttachPosition`.
pub const PEEK: usize = 0;
pub const NO_COLOR: u8 = 16;
/// `Direction.DOWN`'s 3D data value.
const DOWN: u8 = 0;




fn face_step(face: u8) -> [i32; 3] {
    STEPS[face.min(5) as usize]
}
/// `Direction.getOpposite` for a 3D data value.
fn opposite(face: u8) -> u8 {
    face.min(5) ^ 1
}
/// `Shulker.getProgressDeltaAabb`: the shell's box of side `size` from `pos`
/// (the bottom centre), grown `from`..`to` along `direction`.
fn progress_aabb(direction: u8, from: f64, to: f64, pos: Vec3, size: f32) -> Aabb {
    let s = face_step(direction).map(f64::from);
    let size = f64::from(size);
    let (max, min) = (from.max(to), from.min(to));
    let half = size * 0.5;
    Aabb::new(Vec3::new(-half, 0.0, -half), Vec3::new(half, size, half))
        .expand_towards(Vec3::new(
            s[0] * max * size,
            s[1] * max * size,
            s[2] * max * size,
        ))
        .contract(Vec3::new(
            -s[0] * (1.0 + min) * size,
            -s[1] * (1.0 + min) * size,
            -s[2] * (1.0 + min) * size,
        ))
        .move_by(pos)
}
/// Block queries shared by attachment and teleport candidates. Entity overlap
/// is checked separately, using the nearby-entity index from MobWorld.
struct BlockSpace<'w, 'r> {
    world: &'w crate::world::World,
    registry: &'r horizon_world::registry::BlockRegistry,
    facts: &'w crate::blocks::BlockFacts,
    binding: &'w horizon_world::block_collision::BlockCollisionRegistry<'r>,
    powder_walkable: bool,
}
impl<'w, 'r> BlockSpace<'w, 'r> {
}
/// `canStayAt`'s `getProgressAabb(getScale(), ...)`: the shell of `size`
/// fully opened from face `face` of block `p`.
fn attachment_box(p: [i32; 3], face: u8, size: f32) -> Aabb {
    progress_aabb(
        opposite(face),
        -1.0,
        1.0,
        Vec3::new(
            f64::from(p[0]) + 0.5,
            f64::from(p[1]),
            f64::from(p[2]) + 0.5,
        ),
        size,
    )
    .deflate(Vec3::new(1.0e-6, 1.0e-6, 1.0e-6))
}
/// `BlockUtil.packDifferenceInPosition` within 8 blocks of `pos`.
fn pack_diff(pos: [i32; 3], to: [i32; 3]) -> i32 {
    ((to[0] - pos[0] + 8) & 0xFF) << 16
        | ((to[1] - pos[1] + 8) & 0xFF) << 8
        | (to[2] - pos[2] + 8) & 0xFF
}
/// `Shulker.getPhysicalPeek`: the lid's lift for a peek amount.
fn physical_peek(amount: f32) -> f32 {
    0.5 - ((0.5 + amount) * PI).msin() * 0.5
}
/// `ShulkerAttackGoal`: open up and fire a bullet every one to five and a
/// half seconds at a target within 20 blocks.
#[derive(Clone, Debug)]
struct ShulkerAttack {
    attack_time: i32,
}
/// `ShulkerPeekGoal`: idle peeks of one to three seconds.
#[derive(Clone, Debug)]
struct Peek {
    time: i32,
}
/// `attachFace.getOpposite().getRotation()` by attach face: the open
/// direction's rotation (`Direction.getRotation` of `face ^ 1`).
fn face_quat(face: u8) -> glam::Quat {
    match face.min(5) {
        // Attached below: upright.
        0 => glam::Quat::IDENTITY,
        1 => glam::Quat::from_rotation_x(PI),
        2 => glam::Quat::from_rotation_x(PI / 2.0),
        3 => glam::Quat::from_euler(glam::EulerRot::XYZ, PI / 2.0, 0.0, PI),
        4 => glam::Quat::from_euler(glam::EulerRot::XYZ, PI / 2.0, 0.0, -PI / 2.0),
        _ => glam::Quat::from_euler(glam::EulerRot::XYZ, PI / 2.0, 0.0, PI / 2.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobs::sensor_scan_tests::kind;
    use horizon_core::nbt::Tag;
    use std::sync::Arc;

    use horizon_world::{
        attribute_map::ValueKind,
        attributes::{Modifier, Operation, entity::Catalog},
    };

    fn catalog() -> Arc<Catalog> {
        let bits = |v: f64| format!("{:016x}", v.to_bits());
        Arc::new(Catalog::from_json(&serde_json::json!({
            "data_version": horizon_world::DATA_VERSION,
            "attributes": [
                {"id":"minecraft:armor", "default":bits(0.0), "min":bits(0.0), "max":bits(30.0), "syncable":true},
                {"id":"minecraft:armor_toughness", "default":bits(0.0), "min":bits(0.0), "max":bits(20.0), "syncable":true}
            ],
            "entities": {"minecraft:shulker":{"minecraft:armor":bits(0.0), "minecraft:armor_toughness":bits(0.0)}}
        })).unwrap())
    }



    #[test]
    fn covered_is_a_permanent_map_modifier_only_at_the_peek_transition() {
        let catalog = catalog();
        for operation in [Operation::AddMultipliedTotal, Operation::AddMultipliedBase] {
            let mut m = shulker();
            // Construction defines raw peek 0, not a modifier.
            assert_eq!(armor(&m, &catalog), 0.0);
            assert!(modifiers(&m, &catalog).is_empty());
            m.extra
                .live
                .edit(
                    &catalog,
                    &m.kind.id,
                    "minecraft:armor",
                    true,
                    Vec::new,
                    |value, observer| {
                        value.set_base_value(5.0, observer)?;
                        value.add_permanent(
                            Modifier::new("test:scale", 0.1, operation).unwrap(),
                            observer,
                        )
                    },
                )
                .unwrap();
            let mut events = Vec::new();
            set_peek(&mut m, 30, &mut events, &catalog);
            assert_eq!(armor(&m, &catalog), 5.5);
            set_peek(&mut m, 0, &mut events, &catalog);
            // (5 + 20) * 1.1 = 27.5 for either multiplier; damage alone floors.
            assert!((armor(&m, &catalog) - 27.5).abs() < 1e-12);
            assert_eq!(super::super::armor::values(&m), (27.0, 0.0));
            let covered = modifiers(&m, &catalog)
                .into_iter()
                .find(|m| m.id() == "minecraft:covered")
                .unwrap();
            assert_eq!(covered.amount(), 20.0);
            assert_eq!(covered.operation(), Operation::AddValue);
            set_peek(&mut m, 0, &mut events, &catalog);
            assert_eq!(modifiers(&m, &catalog).len(), 2);
            let live = m.extra.live.built().unwrap();
            assert_eq!(
                live.value(live.key("minecraft:armor"), ValueKind::Base)
                    .unwrap(),
                5.0
            );
            // Raw Peek loads independently of saved covered, not through the setter.
            m.extra.counter = 30;
            let types = [(m.kind.id.clone(), m.kind.clone())].into_iter().collect();
            let tag = crate::entity_nbt::mob_to_nbt(&m, [0; 4], None, None, |_| None);
            let loaded = crate::entity_nbt::mob_from_nbt(&tag, &types, 2, None).unwrap();
            assert_eq!(loaded.extra.counter, 30);
            assert!(loaded.extra.live.built().is_none());
            super::super::armor::prepare(&loaded, &catalog);
            assert_eq!(super::super::armor::values(&loaded), (27.0, 0.0));
            assert_eq!(modifiers(&loaded, &catalog).len(), 2);
            m.extra.live.invalidate();
            assert!((armor(&m, &catalog) - 27.5).abs() < 1e-12);
            set_peek(&mut m, 100, &mut events, &catalog);
            assert_eq!(armor(&m, &catalog), 5.5);
            m.extra.live.invalidate();
            assert_eq!(modifiers(&m, &catalog).len(), 1);
            assert_eq!(armor(&m, &catalog), 5.5);
            set_peek(&mut m, 0, &mut events, &catalog);
            m.extra
                .live
                .edit(
                    &catalog,
                    &m.kind.id,
                    "minecraft:armor",
                    true,
                    Vec::new,
                    |value, observer| value.set_base_value(20.0, observer),
                )
                .unwrap();
            assert_eq!(armor(&m, &catalog), 30.0);
            assert_eq!(super::super::armor::values(&m), (30.0, 0.0));
        }
    }

    #[test]
    #[ignore = "daily"]
    fn teleport_closes_raw_peek_without_adding_covered() {
        assert!(super::super::world::with_built_test_world(
            &[],
            |world, registry| {
                for x in -2..=1 {
                    for z in -2..=1 {
                        world
                            .chunks
                            .insert((x, z), Arc::new(crate::chunk::Chunk::empty(x, z)));
                    }
                }
                let stone = registry.default_state("minecraft:stone").unwrap();
                for x in -24..=24 {
                    for z in -24..=24 {
                        world.set(x, 63, z, stone).unwrap();
                    }
                }
            },
            |world, _| {
                let mut m = shulker();
                m.position = Vec3::new(0.5, 64.0, 0.5);
                m.random = horizon_core::random::EntityRandom::own(
                    horizon_core::random::LegacyRandom::new(0),
                );
                let mut events = Vec::new();
                set_peek(&mut m, 100, &mut events, world.attributes);
                assert!(modifiers(&m, world.attributes).is_empty());
                assert!((0..100).any(|_| teleport(&mut m, world, &mut events)));
                assert_eq!(m.extra.counter, 0);
                assert!(modifiers(&m, world.attributes).is_empty());
                assert_eq!(super::super::armor::values(&m), (0.0, 0.0));
            }
        ));
    }

    #[test]
    fn loading_a_closed_peek_does_not_add_covered() {
        let m = shulker();
        let types = [(m.kind.id.clone(), m.kind)].into_iter().collect();
        let tag = Tag::compound([
            (
                "id",
                myriade_data::nbt_value::string("minecraft:shulker"),
            ),
            ("Pos", Tag::List(vec![Tag::Double(0); 3])),
            ("Peek", Tag::Byte(0)),
        ]);
        let loaded = crate::entity_nbt::mob_from_nbt(&tag, &types, 2, None).unwrap();
        assert_eq!(armor(&loaded, &catalog()), 0.0);
        assert!(modifiers(&loaded, &catalog()).is_empty());
    }

    #[test]
    fn opening_forgets_a_removed_transient_covered() {
        let catalog = catalog();
        for peek in [100, 0] {
            let mut m = shulker();
            let modifier = Modifier::new("minecraft:covered", 7.0, Operation::AddValue).unwrap();
            m.extra
                .live
                .edit(
                    &catalog,
                    &m.kind.id,
                    "minecraft:armor",
                    true,
                    Vec::new,
                    |value, observer| value.add_transient(modifier.clone(), observer),
                )
                .unwrap();
            m.extra
                .live
                .keep_transient("minecraft:armor", "minecraft:covered", Some(modifier));
            set_peek(&mut m, peek, &mut Vec::new(), &catalog);
            m.extra.live.invalidate();
            assert_eq!(armor(&m, &catalog), if peek == 0 { 20.0 } else { 0.0 });
        }
    }

    /// `Shulker.setPos`: a piston's push lands on the block centre, the y
    /// rounded by `floor(y + 0.5)`, not left at the pushed offset.
    #[test]
    fn a_pushed_position_snaps_to_the_block_centre() {
        let mut m = shulker();
        m.position = Vec3::new(0.5, 4.0, 0.5);
        client_set_pos(&mut m, 0.5, 3.49, 0.5);
        assert_eq!(m.position, Vec3::new(0.5, 3.0, 0.5));
        client_set_pos(&mut m, 2.9, 3.0, -0.2);
        assert_eq!(m.position, Vec3::new(2.5, 3.0, -0.5));
    }

    /// `Mth.clamp` saturates when the target is below the floor: a negative
    /// raw `Peek` eases the lid there once and then holds, no panic.
    #[test]
    fn a_negative_peek_target_does_not_panic() {
        let mut m = shulker();
        m.extra.counter = -1;
        for _ in 0..3 {
            ease_peek(&mut m);
        }
        assert_eq!(m.anim.v[PEEK], -0.01);
    }

    /// Shulker.setRawPeekAmount: the lid opening posts CONTAINER_OPEN and shutting
    /// CONTAINER_CLOSE, for the sculk sensors and wardens nearby.
    #[test]
    fn the_lid_posts_container_open_and_close() {
        let posted = |events: &[MobEvent]| -> Vec<&'static str> {
            events
                .iter()
                .filter_map(|e| match e {
                    MobEvent::GameEvent { event, .. } => Some(*event),
                    _ => None,
                })
                .collect()
        };
        let mut m = shulker();
        let mut events = Vec::new();
        set_peek(&mut m, 30, &mut events, &catalog());
        assert_eq!(posted(&events), ["minecraft:container_open"]);
        let mut events = Vec::new();
        set_peek(&mut m, 0, &mut events, &catalog());
        assert_eq!(posted(&events), ["minecraft:container_close"]);
    }

    /// `Shulker.setDeltaMovement` is empty: a hit's knockback leaves the shell where it is.
    #[test]
    fn a_hit_does_not_move_a_shulker() {
        let mut m = shulker();
        m.on_ground = true;
        m.knockback(
            &mut crate::health::TestLevel::new(0).level(),
            0.4,
            3.0,
            -4.0,
            &Damage::of(crate::mobs::DamageKind::Thrown),
            1.0,
        );
        assert_eq!(m.velocity, Vec3::ZERO);
    }


    /// A box query over many mobs (a boat's) tests their positions against
    /// `BoxReach` first. It has to reach an open lid on every face:
    /// `makeBoundingBox` grows the shell a block out from its position,
    /// sideways or below it too.
    #[test]
    fn box_reach_reaches_an_open_lid_on_every_face() {
        for face in 0..6u8 {
            let mut m = shulker();
            m.position = Vec3::new(0.5, 4.0, 0.5);
            m.extra.variant3 = face;
            m.anim.v[PEEK] = 1.0;
            let reach = crate::mobs::BoxReach::of(std::slice::from_ref(&m));
            // A small box near the lid's far end, 1.4 out from the shell's
            // centre, where only the open lid reaches.
            let s = face_step(opposite(face)).map(f64::from);
            let tip = Vec3::new(0.5 + s[0] * 1.4, 4.5 + s[1] * 1.4, 0.5 + s[2] * 1.4);
            let area = Aabb::new(
                Vec3::new(tip.x - 0.05, tip.y - 0.05, tip.z - 0.05),
                Vec3::new(tip.x + 0.05, tip.y + 0.05, tip.z + 0.05),
            );
            assert!(
                m.bounds().intersects(area),
                "face {face}: the lid reaches {tip:?}"
            );
            assert!(
                reach.may_meet(&m, area),
                "face {face}: the reach misses the lid"
            );
        }
    }

    /// `Shulker.hurtServer` returns false for an `AbstractArrow` while
    /// `isClosed` (raw peek 0). An open shell takes the same arrow.
    #[test]
    fn a_closed_shell_refuses_an_arrow() {
        let mut fixture = crate::health::TestLevel::new(0);
        let attack = crate::health::Attack {
            projectile: Some(2),
            projectile_is_arrow: true,
            ..Default::default()
        };
        let mut level = fixture.level().attacked_by(&attack);
        let source = Damage::of(super::super::DamageKind::Arrow);
        let mut closed = shulker();
        closed.extra.counter = 0;
        let health = closed.health;
        assert!(
            !closed.damage_projectile(&mut level, 2.0, source, None, [0.0, 1.0]),
            "a closed shell rejects the arrow"
        );
        assert_eq!(closed.health, health);
        let mut open = shulker();
        open.extra.counter = 30;
        assert!(open.damage_projectile(&mut level, 2.0, source, None, [0.0, 1.0]));
        assert!(open.health < health);
    }

    #[test]
    fn a_move_packet_starts_the_teleport_slide() {
        // Shulker.createInterpolationHandler is NO_OP, so the packet's end
        // position is setPos. The slide is armed there, from the block before
        // the packet, and client_tick only counts it down.
        let mut m = shulker();
        m.age = 10;
        m.position = Vec3::new(0.5, 0.0, 0.5);
        m.previous = m.position;
        client_set_pos(&mut m, 3.2, 1.0, 0.4);
        assert_eq!(m.position, Vec3::new(3.5, 1.0, 0.5));
        assert_eq!(m.previous, m.position);
        assert_eq!(m.extra.scratch[0], 6);
        assert_eq!(render_offset(&m, 0.0), glam::DVec3::new(-3.0, -1.0, 0.0));
        client_tick(&mut m);
        assert_eq!(m.extra.scratch[0], 5);
        // A tick with no new packet does not look at previous and re-arm.
        m.previous = Vec3::new(80.0, 0.0, 0.0);
        client_tick(&mut m);
        assert_eq!(m.extra.scratch[0], 4);
    }

    #[test]
    fn a_riding_shulker_closes_its_lid_but_does_not_slide() {
        // Shulker.setPos on a passenger: the exact position, the peek set to
        // 0 on a block change, and no clientSideTeleportInterpolation.
        let mut m = shulker();
        m.age = 10;
        m.extra.passenger = true;
        m.extra.counter = 30;
        m.position = Vec3::new(0.5, 0.0, 0.5);
        client_set_pos(&mut m, 2.25, 0.4, 0.5);
        assert_eq!(m.position, Vec3::new(2.25, 0.4, 0.5));
        assert_eq!(m.extra.counter, 0);
        assert_eq!(m.extra.scratch[0], 0);
        assert_eq!(render_offset(&m, 0.0), glam::DVec3::ZERO);
    }

    #[test]
    #[ignore = "daily"]
    fn loaded_rotation_survives_a_shulker_tick() {
        assert!(crate::mobs::world::with_test_world(&[], |world, _| {
            let mut m = shulker();
            m.yaw = 73.0;
            m.head_pitch = -24.0;
            register(&mut m);
            tick(&mut m, world, &mut Vec::new());
            assert_eq!([m.yaw, m.head_pitch], [73.0, -24.0]);
        }));
    }


    #[test]
    fn the_head_turns_to_its_target() {
        // ShulkerLookControl.clampHeadRotationToBody does nothing, so the
        // head is not clamped to the square body: it turns to the target.
        let mut m = shulker();
        m.ai.look.kind = LookKind::Shulker;
        for _ in 0..20 {
            // A goal keeps the look up to date every tick.
            m.ai.look.set_look_at([5.0, 0.0, 0.0], 10.0, 10.0);
            m.look_control_tick();
        }
        assert!(
            m.head_yaw.abs() > 45.0,
            "the head turns to the target, not back to the square body"
        );
        assert_eq!(m.body_yaw, 0.0);
    }

    #[test]
    fn an_open_shell_on_the_floor_stands_taller() {
        // Shulker.getDefaultDimensions: a shell on Direction.DOWN (attach
        // face 0) with currentPeekAmount p is `scale(1, 1 + p)` of its
        // 1 x 1 x 0.5 box — the width is untouched, the height and the eyes
        // grow. A client eases the peek 0.05 a tick towards the synced raw
        // peek, 100 while ShulkerAttackGoal holds the lid open.
        let mut m = shulker();
        m.extra.variant3 = DOWN;
        m.extra.counter = 100;
        for _ in 0..10 {
            client_tick(&mut m);
        }
        assert!((m.anim.v[PEEK] - 0.5).abs() < 1e-6, "{}", m.anim.v[PEEK]);
        let (w, h, e) = m.dims();
        assert!(
            (w - 1.0).abs() < 1e-9 && (h - 1.5).abs() < 1e-5 && (e - 0.75).abs() < 1e-5,
            "a half open shell is 1 x 1.5 with its eyes at 0.75: {w} x {h} x {e}"
        );
        // The culling (and the eye height) use `height`; the collision box
        // is `bounds_at` (`Shulker.makeBoundingBox`), not built from it.
        assert!((f64::from(m.height()) - 1.5).abs() < 1e-5, "{}", m.height());
    }

    #[test]
    fn only_a_shell_on_the_floor_grows() {
        // Every other attach face keeps the plain 1 x 1 x 0.5 box, however
        // far the lid opens.
        for face in [1u8, 2, 3, 4, 5] {
            let mut m = shulker();
            m.extra.variant3 = face;
            m.extra.counter = 100;
            for _ in 0..20 {
                client_tick(&mut m);
            }
            assert_eq!(m.anim.v[PEEK], 1.0);
            let (w, h, e) = m.dims();
            assert!(
                (w - 1.0).abs() < 1e-9 && (h - 1.0).abs() < 1e-9 && (e - 0.5).abs() < 1e-9,
                "a shell on face {face} stays 1 x 1: {w} x {h} x {e}"
            );
        }
        // A closed one on the floor does not grow either.
        let mut m = shulker();
        m.extra.variant3 = DOWN;
        let (_, h, _) = m.dims();
        assert!((h - 1.0).abs() < 1e-9, "{h}");
    }
}

#[cfg(test)]
mod saved_bytes_tests {
    use super::*;

    /// `Shulker.readAdditionalSaveData`: `AttachFace` is
    /// `Direction.LEGACY_ID_CODEC` (`from3DDataValue` wraps the id through
    /// `abs(id % 6)`), while `Peek` and `Color` are raw `getByteOr` bytes. The
    /// load clamped all three to their setters' ranges instead.
    #[test]
    fn a_shulker_reads_its_face_and_lid_bytes_like_vanilla() {
        use crate::entity_nbt::mob_from_nbt;
        use horizon_core::nbt::Tag;
        let mut kind = crate::mobs::sensor_scan_tests::kind();
        let k = std::sync::Arc::make_mut(&mut kind);
        k.id = "minecraft:shulker".into();
        k.species = Species::Shulker;
        let types = [("minecraft:shulker".into(), kind)].into();
        let tag = Tag::compound([
            ("id", Tag::String("minecraft:shulker".into())),
            (
                "Pos",
                Tag::List(vec![
                    Tag::Double(0.0f64.to_bits()),
                    Tag::Double(64.0f64.to_bits()),
                    Tag::Double(0.0f64.to_bits()),
                ]),
            ),
            ("AttachFace", Tag::Byte(7)),
            ("Peek", Tag::Byte(-1)),
            ("Color", Tag::Byte(-1)),
        ]);
        let m = mob_from_nbt(&tag, &types, 1, None).unwrap();
        assert_eq!(m.extra.variant3, 1, "abs(7 % 6) is up");
        assert_eq!(m.extra.counter, -1, "the peek byte is kept");
        assert_eq!(m.extra.color, 255, "the color byte is kept");
        assert_eq!(
            texture(&m),
            "entity/shulker/shulker_white",
            "DyeColor.byId(-1)"
        );
    }
}
