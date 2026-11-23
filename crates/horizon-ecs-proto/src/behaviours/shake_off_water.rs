//! `Wolf.aiStep` and `Wolf.tick`, shaking off water: a wet wolf that is out of water, stands on the ground and is not pathing starts
//! to shake and tells the clients (entity event 8). The shake plays its sound and `ENTITY_ACTION` game event on the first tick, then
//! `shakeAnim` goes up 0.05 a tick until 2, and the wolf is dry. Entering water cancels the shake (entity event 56), and the wolf stays
//! wet. `Wet` and `Shaking` exist only while they apply: a dry wolf keeps nothing and costs nothing per tick.
use super::*;
use crate::{cow::*, goals::*};

/// Marks a wolf: its module entry gives it the shake.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Shakes;
/// `Wolf.flag2` (`isWet`): the wolf has been in water and has not shaken it off yet.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Wet;
/// `Wolf.counter2` (`isShaking`) with `shakeAnim`: the shake's progress, from 0 to 2.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Shaking(pub f32);

#[derive(Clone, Copy, Debug)]
pub struct Params;
impl Params {
    pub const DEFAULT: Self = Self;
}

pub static MODULE: Module = Module {
    name: "shake_off_water",
    equip,
    systems,
    ..Module::DEFAULT
};

fn equip(w: &mut World, e: Entity, _: Inst, _: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Shakes);
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems((water_changes, wet_by_rain_on_crossing, start_shake, shake_tick, dried_by_death).in_set(slot));
}

/// The block cell of the last rain reading of a shakable wolf: the rain is read again only when it crosses into another cell.
#[derive(Component, Clone, Copy, Debug)]
pub struct RainCell(pub [i32; 3]);

/// `Entity.isInRain` at each block a moving wolf enters, while it rains (`isInWaterOrRain`). `Changed<Pos>` visits only the wolves that
/// moved, and a dry or resting wolf costs nothing; with no rain nothing is read.
fn wet_by_rain_on_crossing(
    level: Option<Res<Level>>,
    mut commands: Commands,
    mut q: Query<(Entity, &Pos, Option<&mut RainCell>, Has<Wet>), (With<Shakes>, Changed<Pos>)>,
) {
    let Some(mw) = level.map(|l| l.mw.0) else { return };
    if !mw.raining {
        return;
    }
    for (e, p, cell, wet) in &mut q {
        let c = [p.0.x.floor() as i32, p.0.y.floor() as i32, p.0.z.floor() as i32];
        match cell {
            Some(mut last) if last.0 == c => continue,
            Some(mut last) => last.0 = c,
            None => {
                commands.entity(e).insert(RainCell(c));
            }
        }
        if !wet && mw.raining_at(c[0], c[1], c[2]) {
            commands.entity(e).insert(Wet);
        }
    }
}

/// Entering water: the wolf is wet, and a shake in progress is cancelled for the clients (56).
fn water_changes(mut ev: ResMut<Events>, mut commands: Commands, shakes: Query<(), With<Shakes>>, shaking: Query<(), With<Shaking>>) {
    let ev = ev.into_inner();
    for e in ev.entered_fluid.iter() {
        if shakes.get(e).is_err() {
            continue;
        }
        commands.entity(e).insert(Wet);
        if shaking.get(e).is_ok() {
            ev.host.entity_event(e, 56);
            commands.entity(e).remove::<Shaking>();
        }
    }
}

/// `Wolf.aiStep`: a wet wolf on the ground, out of the water and the rain, and not pathing starts to shake (8).
fn start_shake(
    mut ev: ResMut<Events>,
    level: Option<Res<Level>>,
    mut commands: Commands,
    q: Query<(Entity, &Pos, &Nav, &Contact, Option<&FluidState>), (With<Wet>, With<Shakes>, Without<Shaking>)>,
) {
    let ev = ev.into_inner();
    for (e, p, nav, contact, fluid) in &q {
        let rained = level.as_ref().is_some_and(|l| l.mw.0.raining_at(p.0.x.floor() as i32, p.0.y.floor() as i32, p.0.z.floor() as i32));
        if nav.path.is_none() && contact.on_ground && !rained && !fluid.is_some_and(|f| f.in_water) {
            commands.entity(e).insert(Shaking(0.0));
            ev.host.entity_event(e, 8);
        }
    }
}

/// The weather changed: a shakable wolf under the sky in the rain is wet (`Wolf.tick`, `isInWaterOrRain`). Read at the event, not per tick.
pub fn weather_changed(w: &mut World) {
    let Some(mw) = w.get_resource::<Level>().map(|l| l.mw.0) else { return };
    if !mw.raining {
        return;
    }
    let dry: Vec<(Entity, [i32; 3])> = w
        .query_filtered::<(Entity, &Pos), (With<Shakes>, Without<Wet>)>()
        .iter(w)
        .map(|(e, p)| (e, [p.0.x.floor() as i32, p.0.y.floor() as i32, p.0.z.floor() as i32]))
        .collect();
    for (e, c) in dry {
        if mw.raining_at(c[0], c[1], c[2]) {
            w.entity_mut(e).insert(Wet);
        }
    }
}

/// `Wolf.tick` while shaking: the first tick plays the shake and its game event, then `shakeAnim` counts up; a wolf at 2 is dry.
fn shake_tick(mut ev: ResMut<Events>, mut commands: Commands, mut q: Query<(Entity, &Pos, &mut Shaking, Option<&mut Rng>)>) {
    let ev = ev.into_inner();
    for (e, pos, mut shaking, rng) in &mut q {
        if shaking.0 >= 2.0 {
            commands.entity(e).remove::<(Wet, Shaking)>();
            continue;
        }
        let at = [pos.0.x, pos.0.y, pos.0.z];
        if shaking.0 == 0.0 {
            let pitch = rng.map_or(1.0, |mut r| {
                let a = r.next_float();
                (a - r.next_float()) * 0.2 + 1.0
            });
            ev.host.sound("entity.wolf.shake", at, 0.4, pitch);
            ev.host.game_event_by(e, "minecraft:entity_action", at, false, false, false);
        }
        shaking.0 += 0.05;
    }
}

/// `Wolf.die`: a wolf killed mid-shake is dry and still, and no clients are told.
fn dried_by_death(ev: Res<Events>, mut commands: Commands, shakes: Query<(), With<Shakes>>) {
    for k in ev.killed.iter() {
        if shakes.get(k.victim).is_ok() {
            commands.entity(k.victim).remove::<(Wet, Shaking)>();
        }
    }
}
