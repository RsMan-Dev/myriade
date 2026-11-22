//! The item frames and paintings the host asks of its ECS world: hung, used, hit, aimed at, and the motive table the paintings are
//! chosen from. Each call lends the mob world for its duration (`lent`); the effects the host plays come back as `FrameEvent`s or as
//! the bool. The host decides first whether the player may interact (`mayInteract`) and checks the map markers limit at `Put`.
use crate::bridge::host::EcsWorld;
use crate::frame_item;
use crate::hanging::{self, Attached, Size};
use crate::painting::{self, Motives};
use crate::targeting::Who;
use bevy_ecs::entity::Entity;
use horizon_client_lib::items::ItemStack;
use horizon_client_lib::mobs::MobWorld;

impl EcsWorld {
    /// `ItemFrameItem.useOn`: a frame (glow or not) on `face` of the block `at`, placed by `who`; None when the wall does not hold.
    pub fn hang_frame(&mut self, mw: &MobWorld<'_>, at: [i32; 3], face: u8, glow: bool, who: Option<Who>) -> Option<Entity> {
        self.lent(mw, |w| frame_item::place(w, at, face, glow, who))
    }

    /// `Painting.create` on `face` of the block `at`: the motive `variant` by its name in the table given to `set_motives`, or when
    /// None one drawn among the largest motives that fit (`draw(n)` gives an index below `n`).
    pub fn hang_painting(
        &mut self,
        mw: &MobWorld<'_>,
        at: [i32; 3],
        face: u8,
        variant: Option<&str>,
        who: Option<Who>,
        draw: &mut dyn FnMut(usize) -> usize,
    ) -> Option<Entity> {
        self.lent(mw, |w| match variant {
            Some(name) => painting::place(w, at, face, name, who),
            None => painting::place_drawn(w, at, face, who, draw),
        })
    }

    /// `ItemFrame.interact` by `who` with `stack` in hand: an empty frame takes it (`Put`), a filled one turns (`Rotated`). False when
    /// nothing happened.
    pub fn frame_use(&mut self, mw: &MobWorld<'_>, e: Entity, who: Who, stack: Option<ItemStack>) -> bool {
        self.lent(mw, |w| match stack {
            _ if frame_item::holds(w, e) => frame_item::rotate(w, e, Some(who)),
            Some(s) => frame_item::put(w, e, s, Some(who)),
            None => false,
        })
    }

    /// `ItemFrame.hurtServer` on a frame that is not fixed (`frame_item::hit`) by `who`. `creative`: the attacker is in creative mode,
    /// so nothing falls.
    pub fn frame_hit(&mut self, mw: &MobWorld<'_>, e: Entity, who: Who, creative: bool) -> bool {
        self.lent(mw, |w| frame_item::hit(w, e, Some(who), creative))
    }

    /// `HangingEntity.hurtServer` on a painting by `who` (`painting::hit`).
    pub fn painting_hit(&mut self, mw: &MobWorld<'_>, e: Entity, who: Who, creative: bool) -> bool {
        self.lent(mw, |w| painting::hit(w, e, Some(who), creative))
    }

    /// The frame the ray from `eye` along `dir` (a unit vector) crosses first within `reach`, with the distance: read from the wall
    /// index, not from every entity.
    pub fn aimed_frame(&self, eye: [f64; 3], dir: [f64; 3], reach: f64) -> Option<(Entity, f64)> {
        frame_item::aimed(self.world(), eye, dir, reach)
    }

    /// The painting the ray crosses first within `reach` (`painting::aimed`).
    pub fn aimed_painting(&self, eye: [f64; 3], dir: [f64; 3], reach: f64) -> Option<(Entity, f64)> {
        painting::aimed(self.world(), eye, dir, reach)
    }

    /// The motives the paintings are chosen from: each name with its footprint in cells (the host's `PaintingVariants`).
    pub fn set_motives(&mut self, motives: Vec<(Box<str>, Size)>) {
        self.world_mut().insert_resource(Motives(motives));
    }

    /// `/item` on the frame `e`: the stack takes its slot (`frame_item::set_item`). False when `e` is not a frame.
    pub fn set_frame_item(&mut self, mw: &MobWorld<'_>, e: Entity, stack: ItemStack) -> bool {
        self.lent(mw, |w| frame_item::set_item(w, e, stack))
    }

    /// Whether a painting of `variant` (None: any motive) would hang on `facing` of `at`, with the rule of the placement.
    pub fn painting_fits(&self, _mw: &MobWorld<'_>, at: [i32; 3], facing: u8, variant: Option<&str>) -> bool {
        painting::fits_named(self.world(), at, facing, variant)
    }

    /// `/fixed` on the frame `e`: its `fixed` flag takes `on`. False when `e` is not a frame.
    pub fn set_frame_fixed(&mut self, mw: &MobWorld<'_>, e: Entity, on: bool) -> bool {
        self.lent(mw, |w| frame_item::set_fixed(w, e, on))
    }

    /// `/invisible` on the frame `e`: its `invisible` flag takes `on`. False when `e` is not a frame.
    pub fn set_frame_invisible(&mut self, mw: &MobWorld<'_>, e: Entity, on: bool) -> bool {
        self.lent(mw, |w| frame_item::set_invisible(w, e, on))
    }

    /// `Entity.setDirection` of the hanging entity `e` to `face`, its place kept; false when the wall does not hold there.
    pub fn set_hanging_facing(&mut self, mw: &MobWorld<'_>, e: Entity, face: u8) -> bool {
        self.lent(mw, |w| {
            let Some(pos) = w.get::<Attached>(e).map(|a| a.pos) else { return false };
            hanging::set_placement(w, e, pos, face)
        })
    }

    /// `ItemFrame.setRotation` on the frame `e`: its rotation takes `rotation % 8`. False when `e` is not a frame.
    pub fn set_frame_rotation(&mut self, mw: &MobWorld<'_>, e: Entity, rotation: u8) -> bool {
        self.lent(mw, |w| frame_item::set_rotation(w, e, rotation))
    }
}
