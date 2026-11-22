//! Item frames and paintings through the level: hung, used, hit and aimed at, by network id. A new frame or painting is linked to its
//! id by `alloc` when the host needs it. On an inert level (a client) each answer is empty.
use super::level::EcsLevel;
use crate::items::ItemStack;
use crate::mobs::MobWorld;
use horizon_ecs_proto::targeting::Who;

impl EcsLevel {
    /// `ItemFrameItem.useOn` for a frame (glow or not) placed by `who`: its id, None when the wall does not hold.
    pub fn hang_frame(&mut self, mw: &MobWorld<'_>, at: [i32; 3], face: u8, glow: bool, who: Option<Who>, alloc: &mut dyn FnMut() -> u32) -> Option<u32> {
        let e = self.ecs_mut()?.hang_frame(mw, at, face, glow, who)?;
        Some(self.id_for(e, alloc))
    }

    /// `Painting.create` for a painting on `face` of `at`: the motive `variant` by name, or one drawn when None (`draw`). Its id, None
    /// when it is not hung.
    pub fn hang_painting(
        &mut self,
        mw: &MobWorld<'_>,
        at: [i32; 3],
        face: u8,
        variant: Option<&str>,
        who: Option<Who>,
        draw: &mut dyn FnMut(usize) -> usize,
        alloc: &mut dyn FnMut() -> u32,
    ) -> Option<u32> {
        let e = self.ecs_mut()?.hang_painting(mw, at, face, variant, who, draw)?;
        Some(self.id_for(e, alloc))
    }

    /// `ItemFrame.interact` by `who` with `stack` in hand; true when the frame answered.
    pub fn frame_use(&mut self, mw: &MobWorld<'_>, id: u32, who: Who, stack: Option<ItemStack>) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.frame_use(mw, e, who, stack))
    }

    /// `ItemFrame.hurtServer` by `who`, in creative mode or not; true when the frame took the hit.
    pub fn frame_hit(&mut self, mw: &MobWorld<'_>, id: u32, who: Who, creative: bool) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.frame_hit(mw, e, who, creative))
    }

    /// `HangingEntity.hurtServer` on a painting by `who`; true when the painting broke.
    pub fn painting_hit(&mut self, mw: &MobWorld<'_>, id: u32, who: Who, creative: bool) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.painting_hit(mw, e, who, creative))
    }

    /// The frame the ray from `eye` along `dir` hits first within `reach`: its network id and the distance. None on an inert level,
    /// or when the frame has no id yet (it was never seen by a player).
    pub fn aimed_frame(&self, eye: [f64; 3], dir: [f64; 3], reach: f64) -> Option<(u32, f64)> {
        let ecs = self.ecs()?;
        let (e, t) = ecs.aimed_frame(eye, dir, reach)?;
        Some((ecs.id_of(e)?, t))
    }

    /// The painting the ray from `eye` along `dir` hits first within `reach`: its network id and the distance. None as `aimed_frame`.
    pub fn aimed_painting(&self, eye: [f64; 3], dir: [f64; 3], reach: f64) -> Option<(u32, f64)> {
        let ecs = self.ecs()?;
        let (e, t) = ecs.aimed_painting(eye, dir, reach)?;
        Some((ecs.id_of(e)?, t))
    }

    /// `/item` on the frame `id`: the stack takes its slot; false when the frame is unknown or not a frame.
    pub fn set_frame_item(&mut self, mw: &MobWorld<'_>, id: u32, stack: ItemStack) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.set_frame_item(mw, e, stack))
    }

    /// Whether a painting of `variant` (None: any motive) would hang on `facing` of `at`; false on an inert level.
    pub fn painting_fits(&self, mw: &MobWorld<'_>, at: [i32; 3], facing: u8, variant: Option<&str>) -> bool {
        self.ecs().is_some_and(|w| w.painting_fits(mw, at, facing, variant))
    }

    /// The `fixed` flag of the frame `id` takes `on`; false when the frame is unknown or not a frame.
    pub fn set_frame_fixed(&mut self, mw: &MobWorld<'_>, id: u32, on: bool) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.set_frame_fixed(mw, e, on))
    }

    /// `Entity.setDirection` of the hanging entity `id` to `face`; false when it is unknown or the wall does not hold there.
    pub fn set_hanging_facing(&mut self, mw: &MobWorld<'_>, id: u32, face: u8) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.set_hanging_facing(mw, e, face))
    }

    /// The rotation of the frame `id` takes `rotation % 8`; false when the frame is unknown or not a frame.
    pub fn set_frame_rotation(&mut self, mw: &MobWorld<'_>, id: u32, rotation: u8) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.set_frame_rotation(mw, e, rotation))
    }

    /// The `invisible` flag of the frame `id` takes `on`; false when the frame is unknown or not a frame.
    pub fn set_frame_invisible(&mut self, mw: &MobWorld<'_>, id: u32, on: bool) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.set_frame_invisible(mw, e, on))
    }
}
