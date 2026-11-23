//! A mob's fluid heights and the fluid at its eyes (`Entity.updateFluidHeightAndDoFluidPushing`,
//! `updateFluidOnEyes`), as its tick reads them over the scene.
use super::super::MobWorld;
use super::Fluids;
use crate::blocks::Fluid;
use horizon_core::geometry::{Aabb, Vec3};

impl MobWorld<'_> {
    /// FluidState.getHeight: a full block under the same fluid, else amount / 9.
    pub fn fluid_height(&self, x: i32, y: i32, z: i32) -> (Fluid, f64) {
        let f = self.facts.get(self.world.block(x, y, z));
        if f.fluid == Fluid::None {
            return (Fluid::None, 0.0);
        }
        let above = self.facts.get(self.world.block(x, y + 1, z));
        if above.fluid == f.fluid {
            return (f.fluid, 1.0);
        }
        (f.fluid, f64::from(f.fluid_level) / 9.0)
    }
    /// Entity.updateFluidHeightAndDoFluidPushing heights over the deflated box,
    /// measured from the box's own bottom.
    pub fn fluids(&self, bounds: Aabb, eye_y: f64) -> Fluids {
        self.fluids_from(bounds, bounds.min_position().y, eye_y)
    }
    /// [`Self::fluids`] for a fluid interaction box another than the entity's
    /// (a boat passenger's): `EntityFluidInteraction.update` finds the fluids
    /// in `bounds` deflated by 0.001 and measures each depth from `bottom`, the
    /// entity's own box bottom (`getBoundingBox().minY`).
    pub fn fluids_from(&self, bounds: Aabb, bottom: f64, eye_y: f64) -> Fluids {
        let b = bounds.deflate(Vec3::new(0.001, 0.001, 0.001));
        let (lo, hi) = (b.min_position(), b.max_position());
        let mut out = Fluids::default();
        // The eye column below, in the box but for a box thinner than the
        // deflation.
        let (ex, ez) = (
            ((lo.x + hi.x) / 2.0).floor() as i32,
            ((lo.z + hi.z) / 2.0).floor() as i32,
        );
        // Most boxes are in sections without a fluid: no cell to read.
        let cells = (
            [
                (lo.x.floor() as i32).min(ex),
                lo.y.floor() as i32,
                (lo.z.floor() as i32).min(ez),
            ],
            [
                (hi.x.ceil() as i32 - 1).max(ex),
                hi.y.ceil() as i32 - 1,
                (hi.z.ceil() as i32 - 1).max(ez),
            ],
        );
        if !self
            .world
            .may_hold(cells.0, cells.1, |s| self.facts.get(s).fluid != Fluid::None)
        {
            return out;
        }
        for x in lo.x.floor() as i32..=(hi.x.ceil() as i32 - 1) {
            for y in lo.y.floor() as i32..=(hi.y.ceil() as i32 - 1) {
                for z in lo.z.floor() as i32..=(hi.z.ceil() as i32 - 1) {
                    let (fluid, h) = self.fluid_height(x, y, z);
                    if fluid == Fluid::None {
                        continue;
                    }
                    let top = f64::from(y) + h;
                    if top >= lo.y {
                        let depth = top - bottom;
                        match fluid {
                            Fluid::Water => {
                                out.water = out.water.max(depth);
                                out.in_water = true;
                            }
                            Fluid::Lava => {
                                out.lava = out.lava.max(depth);
                                out.in_lava = true;
                            }
                            // A mod's fluid is in neither tag: no entity swims in it.
                            Fluid::None | Fluid::Modded => {}
                        }
                    }
                }
            }
        }
        // EntityFluidInteraction.update: the eyes are in a fluid block of
        // the box in the entity's column, between its bottom and its height
        // for the camera, for water and for lava.
        for y in lo.y.floor() as i32..=(hi.y.ceil() as i32 - 1) {
            let (fluid, h) = self.fluid_height(ex, y, ez);
            // `updateFluidOnEyes` tracks every fluid tag: water and lava.
            let eyes = match fluid {
                Fluid::Water => &mut out.eye_in_water,
                Fluid::Lava => &mut out.eye_in_lava,
                Fluid::None | Fluid::Modded => continue,
            };
            if f64::from(y) + h < lo.y || eye_y < f64::from(y) {
                continue;
            }
            let state = self.world.block(ex, y, ez);
            let source = self
                .registry
                .state(state)
                .and_then(|s| s.properties.get("level"))
                .is_none_or(|l| l == "0");
            let above = self.facts.get(self.world.block(ex, y + 1, ez));
            let camera = if source && above.sturdy_full & 1 != 0 {
                1.0
            } else {
                h
            };
            if eye_y <= f64::from(y) + camera {
                *eyes = true;
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use horizon_core::geometry::{Aabb, Vec3};
    use std::sync::Arc;

}
