//! Paintings (`Painting`): a hanging entity showing a motive, sized in cells (`Size`, from `hanging.rs`). State: `Motive`
//! (`species/painting.rs`), the index of the motive in `Motives`, the table the host gives by name (`set_motives`): no name lives
//! in a component. Its survival is `hanging.rs`'s. Saved as `Painting` does: `facing` (the horizontal Direction id), `variant`
//! (the motive's name). Placed and hit with their player, as the frames are (`FrameEvent`).
use crate::frame_item::block_pos;
use crate::goals::Events;
use crate::host_out_frames::{FrameEvent, Spot};
use crate::hanging::{self, Attached, Size, Walls, PAINTING, KINDS};
use crate::rules::hanging::{left, step};
use crate::species::painting::Motive;
use crate::targeting::Who;
use bevy_ecs::prelude::*;
use horizon_core::nbt::{Compound, JavaString, Tag};

/// The motives a painting can show, by name with their footprint in cells, as the host's variant table gives them.
#[derive(Resource, Default)]
pub struct Motives(pub Vec<(Box<str>, Size)>);

/// The horizontal Direction id (`Direction.LEGACY_ID_CODEC_2D`: south, west, north, east) of each face of the 3D order.
const FACE_OF_2D: [u8; 4] = [3, 4, 2, 5];
const FACING_2D: [u8; 6] = [0, 0, 2, 0, 1, 3];

fn emit(w: &mut World, ev: FrameEvent) {
    w.resource_mut::<Events>().host.frame(ev);
}

/// The index and the footprint of the motive named `name`.
fn motive(w: &World, name: &str) -> Option<(u16, Size)> {
    let motives = &w.get_resource::<Motives>()?.0;
    let i = motives.iter().position(|m| &*m.0 == name)?;
    Some((i as u16, motives[i].1))
}

/// Whether the wall holds for a painting of footprint `size` hung on `face` of `pos`.
fn fits(w: &World, pos: [i32; 3], face: u8, size: Size) -> bool {
    w.get_resource::<Walls>().map_or(true, |f| (f.0)(w, pos, face, (size.width, size.height)))
}

/// Whether a painting of motive `variant` (None: any motive) would hang on `face` of `pos`: the rule of `place` and `place_drawn`.
pub fn fits_named(w: &World, pos: [i32; 3], face: u8, variant: Option<&str>) -> bool {
    match variant {
        Some(name) => motive(w, name).is_some_and(|(_, s)| fits(w, pos, face, s)),
        None => w.get_resource::<Motives>().is_some_and(|m| m.0.iter().any(|m| fits(w, pos, face, m.1))),
    }
}

/// Hangs the motive `index` of footprint `size` and writes its event.
fn hang_motive(w: &mut World, pos: [i32; 3], face: u8, index: u16, size: Size, who: Option<Who>) -> Entity {
    let e = hanging::hang(w, pos, face, PAINTING, Some(size));
    w.entity_mut(e).insert(Motive(index));
    emit(w, FrameEvent::PaintingPlaced { who, spot: Spot { at: pos, face, glow: false } });
    e
}

/// `Painting.create` once the motive is chosen: hung on `face` of the block `pos` when the wall holds for its footprint. None for an
/// unknown motive or a wall that does not hold.
pub fn place(w: &mut World, pos: [i32; 3], face: u8, name: &str, who: Option<Who>) -> Option<Entity> {
    let (index, size) = motive(w, name)?;
    fits(w, pos, face, size).then(|| hang_motive(w, pos, face, index, size, who))
}

/// `Painting.create` without a motive: among the motives that fit, the largest ones (by area), one drawn at random. `draw(n)` gives
/// an index below `n` (the host's random, `Util.getRandomSafe`). None when no motive fits.
pub fn place_drawn(w: &mut World, pos: [i32; 3], face: u8, who: Option<Who>, draw: &mut dyn FnMut(usize) -> usize) -> Option<Entity> {
    let fitting: Vec<(u16, Size)> = {
        let w = &*w;
        let table = &w.get_resource::<Motives>()?.0;
        table.iter().enumerate().filter(|(_, m)| fits(w, pos, face, m.1)).map(|(i, m)| (i as u16, m.1)).collect()
    };
    let area = |s: &Size| usize::from(s.width) * usize::from(s.height);
    let max = fitting.iter().map(|(_, s)| area(s)).max()?;
    let best: Vec<(u16, Size)> = fitting.into_iter().filter(|(_, s)| area(s) == max).collect();
    let (index, size) = best[draw(best.len()).min(best.len() - 1)];
    Some(hang_motive(w, pos, face, index, size, who))
}

/// The painting the ray from `eye` along `dir` (a unit vector) crosses first within `reach`, with the distance `t`.
pub fn aimed(w: &World, eye: [f64; 3], dir: [f64; 3], reach: f64) -> Option<(Entity, f64)> {
    hanging::aimed(w, eye, dir, reach, |w, e| w.get::<Motive>(e).is_some())
}

/// The point a painting of footprint `size` falls from: the cell centre, half a step back toward the wall, and the half of an even
/// side along its width and its height (`Painting.calculateBoundingBox`, `offsetForPaintingSize`).
fn drop_at(a: &Attached, size: Size) -> [f64; 3] {
    let d = step(a.face);
    let l = left(a.face);
    let half = |n: u8| if n % 2 == 0 { 0.5 } else { 0.0 };
    let (ow, oh) = (half(size.width), half(size.height));
    std::array::from_fn(|i| f64::from(a.pos[i]) + 0.5 - 0.46875 * f64::from(d[i]) + ow * f64::from(l[i]) + oh * f64::from(i == 1))
}

/// `HangingEntity.dropItem` of a painting: it goes (`PaintingBroke`), and falls unless `creative`. `who` None for a survival break.
pub(crate) fn break_out(w: &mut World, e: Entity, who: Option<Who>, creative: bool) {
    let Some(a) = w.get::<Attached>(e).copied() else { return };
    let size = w.get::<Size>(e).copied().unwrap_or(Size { width: 1, height: 1 });
    let spot = Spot { at: a.pos, face: a.face, glow: false };
    w.despawn(e);
    emit(w, FrameEvent::PaintingBroke { who, spot, drop: !creative, drop_at: drop_at(&a, size) });
}

/// `HangingEntity.hurtServer` on a painting: it breaks and goes, and falls unless `creative`. False for an entity that is not a
/// painting.
pub fn hit(w: &mut World, e: Entity, who: Option<Who>, creative: bool) -> bool {
    if w.get::<Motive>(e).is_none() {
        return false;
    }
    break_out(w, e, who, creative);
    true
}

/// The keys of a painting (`Painting.addAdditionalSaveData`, its base's `block_pos` and the entity `id` included).
pub fn save(w: &World, e: Entity) -> Option<Compound> {
    let a = w.get::<Attached>(e)?;
    let v = w.get::<Motive>(e)?;
    let name = &w.get_resource::<Motives>()?.0.get(usize::from(v.0))?.0;
    let mut c = Compound::new();
    c.insert("id".into(), Tag::String(KINDS[usize::from(a.kind)].item.into()));
    c.insert("block_pos".into(), Tag::IntArray(a.pos.to_vec()));
    c.insert("facing".into(), Tag::Byte(FACING_2D[usize::from(a.face % 6)] as i8));
    c.insert("variant".into(), Tag::String((&**name).into()));
    Some(c)
}

/// `Painting.readAdditionalSaveData`: the painting is hung on its saved face without a wall check (it was saved on one). None when
/// its `block_pos`, its `facing` or its `variant` is missing, or the motive is not in the table.
pub fn load(w: &mut World, c: &Compound) -> Option<Entity> {
    let pos = block_pos(c)?;
    let Some(Tag::Byte(f)) = c.get(&JavaString::from("facing")) else { return None };
    let face = *FACE_OF_2D.get(usize::try_from(*f).ok()?)?;
    let Some(Tag::String(name)) = c.get(&JavaString::from("variant")) else { return None };
    let (index, size) = motive(w, &name.to_string_lossy())?;
    let e = hanging::hang(w, pos, face, PAINTING, Some(size));
    w.entity_mut(e).insert(Motive(index));
    Some(e)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> World {
        let mut w = World::new();
        hanging::insert_resources(&mut w);
        w.insert_resource(Events::default());
        w.insert_resource(Motives(vec![
            ("minecraft:kebab".into(), Size { width: 1, height: 1 }),
            ("minecraft:pigscene".into(), Size { width: 4, height: 4 }),
        ]));
        w
    }

    #[test]
    fn a_painting_saved_and_loaded_keeps_its_motive_footprint_and_face() {
        let mut w = world();
        let e = place(&mut w, [1, 64, 1], 2, "minecraft:pigscene", None).expect("the wall holds");
        let c = save(&w, e).expect("a painting");
        assert_eq!(c.get(&JavaString::from("variant")), Some(&Tag::String("minecraft:pigscene".into())));
        let mut back = world();
        let l = load(&mut back, &c).expect("loaded");
        let a = back.get::<Attached>(l).expect("attached");
        assert_eq!((a.pos, a.face, a.kind), ([1, 64, 1], 2, PAINTING));
        assert_eq!(back.get::<Size>(l).map(|s| (s.width, s.height)), Some((4, 4)));
        assert_eq!(back.get::<Motive>(l).map(|v| v.0), Some(1));
    }

    #[test]
    fn an_unknown_motive_is_not_hung() {
        let mut w = world();
        assert!(place(&mut w, [1, 64, 1], 2, "minecraft:unknown", None).is_none());
    }

    #[test]
    fn a_drawn_painting_is_one_of_the_largest_motives_that_fit() {
        let mut w = world();
        let e = place_drawn(&mut w, [1, 64, 1], 2, None, &mut |n| n - 1).expect("a motive fits");
        assert_eq!(w.get::<Motive>(e).map(|m| m.0), Some(1), "the 4 x 4 motive is the largest");
    }
}
