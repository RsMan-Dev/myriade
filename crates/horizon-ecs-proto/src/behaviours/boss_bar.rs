//! The wither's boss bar (`game/interface/bossbars.rs`: colour 5, progress health / max). The bar opens with the animal (`equip`),
//! and each health change sends its progress (`progress`, called by the regeneration and by the accepted hurts). Its end (death,
//! despawn) belongs to the death path, not here.
use super::*;
use crate::cow::*;
use crate::goals::*;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub color: u8,
    pub name: &'static str,
}
impl Params {
    pub const WITHER: Self = Self { color: 5, name: "entity.minecraft.wither" };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Bar {
    color: u8,
    name: &'static str,
}
pub static MODULE: Module = Module {
    name: "boss_bar",
    flags: 0,
    equip,
    requires: &[Cap::Living],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, _: Inst, p: &(dyn std::any::Any + Sync)) {
    let p = *params::<Params>(p);
    w.entity_mut(e).insert(Bar { color: p.color, name: p.name });
    progress(w, e);
}
/// The bar's progress now: health over its maximum.
pub fn progress(w: &mut World, e: Entity) {
    let (Some(b), Some(h)) = (w.get::<Bar>(e).copied(), w.get::<Health>(e).map(|h| h.0)) else { return };
    let max = w.get::<MaxHealth>(e).map_or(h, |m| m.0);
    w.resource_mut::<Events>().host.boss_bar(e.index().index(), b.name, (h / max).clamp(0.0, 1.0), b.color);
}
