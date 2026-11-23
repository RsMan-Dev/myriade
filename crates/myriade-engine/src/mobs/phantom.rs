use horizon_core::geometry::trig::Mth;
use horizon_core::{geometry::Vec3, random::RandomSource};

/// `PhantomAttackPlayerTargetGoal`: every three seconds, the player
/// within 16 blocks across and 64 up or down.
#[derive(Clone, Debug)]
struct AttackPlayerTarget {
    next_scan: i32,
}
/// `PhantomAttackStrategyGoal`: circle, then swoop.
#[derive(Clone, Debug)]
struct AttackStrategy {
    next_sweep: i32,
}
impl AttackStrategy {
}
/// `PhantomCircleAroundAnchorGoal`.
#[derive(Clone, Debug, Default)]
struct CircleAroundAnchor {
    angle: f32,
    distance: f32,
    height: f32,
    clockwise: f32,
}
impl CircleAroundAnchor {
}
/// `PhantomSweepAttackGoal`: dive at the target, bite on contact.
#[derive(Clone, Debug)]
struct SweepAttack {
    scared: bool,
    cat_search: u32,
}
fn approach(from: f32, to: f32, step: f32) -> f32 {
    if from < to {
        (from + step).min(to)
    } else {
        (from - step).max(to)
    }
}
fn wrap(d: f32) -> f32 {
    let r = d.rem_euclid(360.0);
    if r >= 180.0 { r - 360.0 } else { r }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobs::world::{test_player, with_built_test_world};

    /// The press geometry shared by the pane tests (#4928): a pane wall at
    /// x=2 (its post spans x 2.4375..2.5625), the player pressed against its
    /// east face at (2.8625, 64, 2.5) and the phantom pressed against the
    /// west face at (1.9875, 64.2, 2.5), so the two boxes are 0.125 (the
    /// pane's thickness) apart and `inflate(0.2)` reaches through.
    fn press_setup(
        wall_top: i32,
    ) -> impl FnOnce(&mut crate::world::World, &horizon_world::registry::BlockRegistry) {
        move |world, registry| {
            for x in -1..=1 {
                for z in -1..=1 {
                    world.chunks.insert(
                        (x, z),
                        std::sync::Arc::new(crate::chunk::Chunk::empty(x, z)),
                    );
                }
            }
            let pane = registry.default_state("minecraft:glass_pane").unwrap();
            for y in 64..wall_top {
                world.set(2, y, 2, pane).unwrap();
            }
        }
    }



}

#[cfg(test)]
mod anchor_save_tests {
    use super::*;
    use crate::entity_nbt::{mob_from_nbt, mob_to_nbt};
    use horizon_core::nbt::Tag;
    use std::collections::HashMap;
    use std::sync::Arc;

    /// A phantom at (0.5, 75, 0.5) with the vanilla size 2, and `anchor_pos`
    /// when it has one. `storeNullable` writes the key only for an anchor.
    fn saved_phantom(anchor: Option<[i32; 3]>) -> Tag {
        let mut fields = vec![
            ("id", Tag::String("minecraft:phantom".into())),
            (
                "Pos",
                Tag::List(vec![
                    Tag::Double(0.5_f64.to_bits()),
                    Tag::Double(75.0_f64.to_bits()),
                    Tag::Double(0.5_f64.to_bits()),
                ]),
            ),
            ("size", Tag::Int(2)),
        ];
        if let Some(anchor) = anchor {
            fields.push(("anchor_pos", Tag::IntArray(anchor.to_vec())));
        }
        Tag::compound(fields)
    }

    fn phantom_types() -> HashMap<String, Arc<crate::mobs::MobType>> {
        let mut kind = crate::mobs::sensor_scan_tests::kind();
        let ty = Arc::make_mut(&mut kind);
        ty.id = "minecraft:phantom".into();
        ty.species = Species::Phantom;
        [("minecraft:phantom".into(), kind)].into()
    }

    /// `Phantom.addAdditionalSaveData` writes `anchor_pos` with `storeNullable`
    /// and `readAdditionalSaveData` reads it back
    /// (`input.read("anchor_pos", BlockPos.CODEC).orElse(null)`). Horizon models
    /// the anchor in `extra.home` (vanilla's `anchorPoint`) and neither side
    /// carried it: a phantom saved and reloaded kept the anchor `register` gives
    /// every new phantom — its own position five blocks up — and circled there
    /// instead of around the anchor its save named, or around itself as vanilla
    /// does for a save with no anchor at all.
    #[test]
    fn a_phantoms_circling_anchor_survives_the_chunk_save() {
        let types = phantom_types();
        let mut m = mob_from_nbt(&saved_phantom(Some([100, 64, -40])), &types, 1, None).unwrap();
        assert_eq!(m.extra.home, Some([100, 64, -40]), "the loaded anchor");
        // `PhantomCircleAroundAnchorGoal.selectNext` flies around the anchor:
        // `anchor + (distance*cos(angle), -4 + height, distance*sin(angle))`.
        let mut circle = CircleAroundAnchor {
            distance: 10.0,
            height: 2.0,
            clockwise: 1.0,
            angle: 0.0,
        };
        circle.select_next(&mut m);
        let angle = 15.0f32.to_radians();
        let around = [
            100.0 + f64::from(10.0f32 * angle.mcos()),
            64.0 + f64::from(-4.0 + 2.0f32),
            -40.0 + f64::from(10.0f32 * angle.msin()),
        ];
        assert_eq!(
            m.extra.vector, around,
            "the circle point, 109.66 / 62 / -37.41 around the anchor"
        );
        // What the phantom circles now is what its chunk save keeps.
        m.extra.home = Some([9, 70, -3]);
        let source = saved_phantom(Some([100, 64, -40]));
        let saved = mob_to_nbt(&m, [0; 4], Some(&source), None, |_| None);
        assert_eq!(
            saved.get("anchor_pos"),
            Some(&Tag::IntArray(vec![9, 70, -3]))
        );
        // A phantom with no anchor: `orElse(null)` on the way in, and no
        // `anchor_pos` on the way out even when the loaded save had one.
        let mut loose = mob_from_nbt(&saved_phantom(None), &types, 2, None).unwrap();
        assert_eq!(loose.extra.home, None, "a save without anchor_pos");
        loose.extra.home = None;
        let stale = saved_phantom(Some([100, 64, -40]));
        let saved = mob_to_nbt(&loose, [0; 4], Some(&stale), None, |_| None);
        assert_eq!(saved.get("anchor_pos"), None, "no anchor, no key");
    }

    #[test]
    fn a_phantoms_look_control_keeps_the_flight_pitch() {
        let types = phantom_types();
        let mut m = mob_from_nbt(&saved_phantom(None), &types, 3, None).unwrap();
        // `PhantomLookControl.tick` is empty, unlike `LookControl.tick`.
        m.head_pitch = -22.0;
        m.look_control_tick();
        assert_eq!(m.head_pitch, -22.0);
    }
}
