//! A cart's `applyEffectsFromBlocks`, client display only: bubble columns,
//! powder snow and cobwebs. Cactus, lava and fire hits are the ECS's.
use super::*;

impl Game<'_> {
    /// `applyEffectsFromBlocks` for a cart: a bubble column lifts or drags it
    /// and resets the fall distance.
    pub(in crate::game) fn cart_block_effects(&self, c: &mut Minecart) {
        if c.push_by_bubble_column(
            &self.world,
            self.pack.assets.registry,
            self.res.facts(),
            self.collision,
        ) {
            c.fall_distance = 0.0;
        }
        // PowderSnowBlock's FREEZE contact is counted here too. The cactus is
        // the ECS's (`inside_more::vehicle_scan`), not a hit taken here.
        // WebBlock.entityInside: makeStuckInBlock(0.25, 0.05, 0.25) for every
        // cobweb the box (deflated by 1.0E-5) is in; it resets the fall distance.
        let b = c.bounds().deflate(Vec3::new(1e-5, 1e-5, 1e-5));
        let (lo, hi) = (b.min_position(), b.max_position());
        let facts = self.res.facts();
        let mut froze = false;
        for x in lo.x.floor() as i32..=hi.x.floor() as i32 {
            for y in lo.y.floor() as i32..=hi.y.floor() as i32 {
                for z in lo.z.floor() as i32..=hi.z.floor() as i32 {
                    let name = &*facts.get(self.world.block(x, y, z)).name;
                    if name == "minecraft:powder_snow" {
                        froze = true;
                    }
                    if name == "minecraft:cobweb" {
                        c.fall_distance = 0.0;
                        c.stuck = Vec3::new(0.25, f64::from(0.05_f32), 0.25);
                    }
                }
            }
        }
        if froze {
            // InsideBlockEffectType.FREEZE: once per applyEffectsFromBlocks.
            c.ticks_frozen = (c.ticks_frozen + 1)
                .min(horizon_client::motion::PlayerMotion::TICKS_REQUIRED_TO_FREEZE);
        }
    }
}
