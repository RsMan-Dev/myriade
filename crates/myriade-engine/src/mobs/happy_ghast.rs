use horizon_core::{
    geometry::{Aabb, Vec3},
    random::RandomSource,
};

/// `createAttributes`' default `Attributes.FLYING_SPEED`, and the speed `travel` uses at it
/// (`* 5 / 3`). A mob reads its own value through [`flying_speed`].
pub const FLYING_SPEED: f64 = 0.05;
pub const AIR_SPEED: f32 = 0.05 * 5.0 / 3.0;


fn player_collision(alive: bool, baby: bool, still: bool, feet_y: f64, top: f64) -> bool {
    alive && !baby && (feet_y >= top || still)
}
/// `HappyGhastFloatGoal`: a `FloatGoal` off a still timeout.
#[derive(Clone, Debug)]
struct HappyGhastFloat;
/// `TemptGoal.ForNonPathfinders`: drift toward a player holding food, or
/// food and harnesses for a bare adult.
#[derive(Clone, Debug, Default)]
struct GhastTempt {
    calm_down: i32,
}
impl GhastTempt {
}
/// `Ghast.RandomFloatAroundGoal` with a restriction and a wish for open
/// air next to something solid.
#[derive(Clone, Debug)]
struct FloatAround {
    blocks: i32,
}
impl FloatAround {
}
/// Texture of the harness layer for the worn item.
pub fn harness_texture(item: &str) -> Option<String> {
    let name = item.strip_prefix("minecraft:")?;
    name.ends_with("_harness")
        .then(|| format!("entity/equipment/happy_ghast_body/{name}"))
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_collision_matches_vanilla_top_and_still_timeout_rules() {
        assert!(player_collision(true, false, false, 84.0, 84.0));
        assert!(player_collision(true, false, false, 85.0, 84.0));
        assert!(!player_collision(true, false, false, 83.99, 84.0));
        assert!(player_collision(true, false, true, 83.99, 84.0));
        assert!(!player_collision(true, true, true, 84.0, 84.0));
        assert!(!player_collision(false, false, true, 84.0, 84.0));
    }

    #[test]
    fn a_harness_names_its_texture() {
        assert_eq!(
            harness_texture("minecraft:red_harness").as_deref(),
            Some("entity/equipment/happy_ghast_body/red_harness")
        );
        assert_eq!(harness_texture("minecraft:saddle"), None);
    }
    #[test]
    fn travel_speed_is_five_thirds_of_the_flying_speed() {
        assert!((f64::from(AIR_SPEED) - FLYING_SPEED * 5.0 / 3.0).abs() < 1e-7);
    }


}

#[cfg(test)]
mod still_float_tests {
    use super::*;
    use crate::mobs::{Species, world::with_test_world};

}

#[cfg(test)]
mod cloud_heal_tests {
    use super::*;
    use crate::mobs::world::{with_built_test_world, with_test_world};
    use horizon_world::dimension_type::DimensionFacts;



    /// The overworld with its clouds at 120, their colour as opaque as `alpha`
    /// makes it (0 is a dimension without clouds).
    fn cloudy(alpha: f32) -> &'static DimensionFacts {
        let mut d = DimensionFacts::overworld().clone();
        d.cloud_height = 120.0;
        d.cloud_color_alpha = alpha;
        Box::leak(Box::new(d))
    }

    /// `Entity.isInClouds` is half of `isFastHealing`: a hurt ghast whose box
    /// reaches the cloud band heals on tick 20, one just below it on tick 600.
    #[test]
    #[ignore = "daily"]
    fn a_happy_ghast_in_the_clouds_heals_every_second() {
        let dim = cloudy(0.8);
        let mut seen = None;
        let ran = with_test_world(&[], |w, types| {
            let mut w = MobWorld::clone(w);
            w.dimension_type = dim;
            let mut above = hurt_ghast(types, 120.0);
            let mut below = hurt_ghast(types, 115.0);
            seen = Some((
                health_after(&mut above, &w, 20),
                health_after(&mut below, &w, 20),
                health_after(&mut below, &w, 600),
            ));
        });
        if !ran {
            return;
        }
        assert_eq!(
            seen,
            Some((5.0, 4.0, 5.0)),
            "in the clouds heals on tick 20; below them only on tick 600"
        );
    }

    /// The band runs from `visual/cloud_height` to four blocks above it, and
    /// the box counts, not the feet: a four-block box whose top only touches
    /// the bottom is in (`getY() + getBbHeight() < cloudBottom` is false), and
    /// so are feet exactly at the top (`getY() <= cloudTop`), but not above it.
    #[test]
    #[ignore = "daily"]
    fn the_cloud_band_is_the_four_blocks_over_its_bottom() {
        let dim = cloudy(0.8);
        let mut seen = None;
        let ran = with_test_world(&[], |w, types| {
            let mut w = MobWorld::clone(w);
            w.dimension_type = dim;
            // 116 + 4 == 120: the box top exactly at the bottom of the band.
            let mut top_at_bottom = hurt_ghast(types, 116.0);
            // 120 + 4: the feet exactly at the top of the band, and above it.
            let mut feet_at_top = hurt_ghast(types, 124.0);
            let mut feet_above = hurt_ghast(types, 124.5);
            seen = Some((
                health_after(&mut top_at_bottom, &w, 20),
                health_after(&mut feet_at_top, &w, 20),
                health_after(&mut feet_above, &w, 20),
            ));
        });
        if !ran {
            return;
        }
        assert_eq!(
            seen,
            Some((5.0, 5.0, 4.0)),
            "a box top at 120 and feet at 124 are in the clouds, feet at 124.5 are not"
        );
    }

    /// A dimension whose `visual/cloud_color` is transparent has no clouds to
    /// stand in (the Nether and the End), so the slow interval stays.
    #[test]
    #[ignore = "daily"]
    fn a_dimension_without_clouds_never_counts_as_in_them() {
        let dim = cloudy(0.0);
        let mut seen = None;
        let ran = with_test_world(&[], |w, types| {
            let mut w = MobWorld::clone(w);
            w.dimension_type = dim;
            let mut m = hurt_ghast(types, 120.0);
            seen = Some((health_after(&mut m, &w, 20), health_after(&mut m, &w, 600)));
        });
        if !ran {
            return;
        }
        assert_eq!(seen, Some((4.0, 5.0)), "no cloud colour: the slow heal");
    }

    /// The other half of `isFastHealing`: `Level.precipitationAt` at the
    /// block position, which the cloud test must not replace.
    #[test]
    fn a_happy_ghast_in_the_rain_still_heals_every_second() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/myriade-engine")
            .join(horizon_core::VANILLA_VERSION);
        let Ok(biomes) = crate::biome::Biomes::load(
            &horizon_assets::resource_source::ResourceRoot::directory(root),
        ) else {
            return;
        };
        let plains = biomes.id("minecraft:plains");
        let dim = cloudy(0.0);
        let mut seen = None;
        let ran = with_built_test_world(
            &[],
            |world, _| {
                let mut chunk = crate::chunk::Chunk::empty(0, 0);
                let qy = (120 - chunk.min_y()) >> 2;
                for qx in 0..4 {
                    for qz in 0..4 {
                        chunk.set_biome_cell(qx, qy, qz, plains);
                    }
                }
                world.chunks.insert((0, 0), std::sync::Arc::new(chunk));
            },
            |w, types| {
                let mut w = MobWorld::clone(w);
                w.raining = true;
                w.dimension_type = dim;
                let mut m = hurt_ghast(types, 120.0);
                seen = Some(health_after(&mut m, &w, 20));
            },
        );
        if !ran {
            return;
        }
        assert_eq!(seen, Some(5.0), "rain over a wet biome: the fast heal");
    }
}
