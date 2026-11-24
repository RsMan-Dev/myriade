//! `Dolphin.DATA_MOISTNESS_LEVEL` (index 19, default 2400) and its `tick`: in water or rain it is 2400; out of both it falls by 1 a
//! tick, and at 0 or less the dolphin takes 1 of `dry_out` damage every tick. The water entry/exit event (`swim::water_changed`) starts
//! the countdown; it runs as a deadline of the wheel only while the dolphin is dry (a wet dolphin costs nothing).
use crate::cow::{after, Body, Cause, Contact, DamageQueue, Level, Moving, Pos, Rng, Vel, Yaw};
use crate::dragon::flight::NoAi;
use bevy_ecs::prelude::*;

pub const FULL: i32 = 2400;

/// The moisture of a dolphin; `dry` is whether it is out of the water, `ticking` whether its countdown deadline is armed.
#[derive(Component, Clone, Copy, Debug)]
pub struct Moisture {
    pub level: i32,
    pub dry: bool,
    pub ticking: bool,
}

pub static MOISTURE: crate::rules::Rule = crate::rules::Rule { name: "moisture", equip, attrs: crate::rules::no_attrs, interact: crate::rules::no_interact };
fn equip(w: &mut World, e: Entity, _: &'static (dyn std::any::Any + Sync)) {
    // Out of the water until the first event says otherwise (as `swim`'s `Dry`): the countdown runs from the birth.
    w.entity_mut(e).insert(Moisture { level: FULL, dry: true, ticking: true });
    after(w, e, 1, tick);
}

/// The entity entered the water (`true`) or left it.
pub fn water_changed(w: &mut World, e: Entity, in_water: bool) {
    let Some(mut m) = w.get_mut::<Moisture>(e) else { return };
    m.dry = !in_water;
    if in_water {
        if m.level != FULL {
            m.level = FULL;
            crate::bridge::view::mark(w, e);
        }
    } else if !m.ticking {
        m.ticking = true;
        after(w, e, 1, tick);
    }
}

/// `Dolphin.tick` (the part after `isNoAi`): out of water and rain the level falls, and at 0 the dolphin dries out.
fn tick(w: &mut World, e: Entity) {
    let Some(m) = w.get::<Moisture>(e).copied() else { return };
    if w.get::<NoAi>(e).is_some() {
        w.get_mut::<Moisture>(e).unwrap().ticking = false;
        return;
    }
    let rain = !m.dry || in_rain(w, e);
    let mut now = m;
    if rain {
        now.level = FULL;
    } else {
        now.level -= 1;
        if now.level <= 0 {
            w.resource_mut::<DamageQueue>().add(e, 1.0, Cause::DryOut);
        }
        flop(w, e);
    }
    now.ticking = m.dry;
    *w.get_mut::<Moisture>(e).unwrap() = now;
    if now.level != m.level {
        crate::bridge::view::mark(w, e);
    }
    if now.ticking {
        after(w, e, 1, tick);
    }
}

/// `Entity.isInRain`: rain at the feet or at the top of the box.
fn in_rain(w: &World, e: Entity) -> bool {
    let mw = w.resource::<Level>().mw.0;
    if !mw.raining {
        return false;
    }
    let (Some(p), Some(b)) = (w.get::<Pos>(e), w.get::<Body>(e)) else { return false };
    let (x, y, z) = (p.0.x.floor() as i32, p.0.y.floor() as i32, p.0.z.floor() as i32);
    mw.raining_at(x, y, z) || mw.raining_at(x, (p.0.y + f64::from(b.height)).floor() as i32, z)
}

/// `Dolphin.tick`: on the ground out of the water it hops at random (+/-0.2 sideways, 0.5 up) and turns to a random yaw.
fn flop(w: &mut World, e: Entity) {
    if !w.get::<Contact>(e).is_some_and(|c| c.on_ground) {
        return;
    }
    let Some(mut rng) = w.get_mut::<Rng>(e) else { return };
    let (a, b, y) = (rng.next_f64() as f32, rng.next_f64() as f32, rng.next_f64() as f32);
    if let Some(mut v) = w.get_mut::<Vel>(e) {
        v.0.x += f64::from((a * 2.0 - 1.0) * 0.2);
        v.0.y += 0.5;
        v.0.z += f64::from((b * 2.0 - 1.0) * 0.2);
    }
    if let Some(mut yaw) = w.get_mut::<Yaw>(e) {
        yaw.0 = y * 360.0;
    }
    w.get_mut::<Contact>(e).unwrap().on_ground = false;
    w.entity_mut(e).insert(Moving);
    crate::bridge::view::mark(w, e);
}
