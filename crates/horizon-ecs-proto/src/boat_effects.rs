//! The effects of the blocks a boat touches. A bubble column arms a throw `BUBBLE_TIME` ticks ahead (`Wobble`), powder snow and cobwebs
//! stick it (`Hull.stuck`), fire lights it and burns it at the deadlines of its fire (`Hull.fire`), water puts the fire out, lava is
//! announced. `boat_cells` reads the cells of the moving boats and of the boats beside a block that changed; everything else is a deadline.
use crate::block_events::BlockEvents;
use crate::blocks::{property, world_of};
use crate::boat::Boat;
use crate::boat_hurt::{self, HurtFlags};
use crate::boat_state::{Hull, Wobble};
use crate::cow::{after, block_changes, reindex, Index, Moving, Pos, Vel};
use crate::flat::Rng;
use crate::host_out_boat_effects::BoatEffectEvent;
use crate::rules::boat::bounds_at;
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;
use horizon_items::blocks::Fluid;

/// The countdown of a bubble column (jar: `bipush 60` in `AbstractBoat`).
pub const BUBBLE_TIME: i32 = 60;
/// `igniteForSeconds(8)`: a fire block sets a boat on fire for 160 ticks (host `boat_fire_passes`).
const FIRE_TICKS: i32 = 160;
/// `WebBlock.entityInside`'s `makeStuckInBlock` multiplier (jar: 0.25, 0.05, 0.25).
pub const WEB_STUCK: [f64; 3] = [0.25, 0.05_f32 as f64, 0.25];
/// `PowderSnowBlock.entityInside`'s `makeStuckInBlock` multiplier (jar: 0.9, 1.5, 0.9).
pub const POWDER_SNOW_STUCK: [f64; 3] = [0.9_f32 as f64, 1.5, 0.9_f32 as f64];

/// `Entity.lavaHurt`'s damage (jar: 4.0).
const LAVA_DAMAGE: f32 = 4.0;

/// A fire or lava hit on boat `e`, applied here through `boat_hurt::hit` (no attacker; the entity-drops rule is taken as on).
fn hurt(w: &mut World, e: Entity, amount: f32) {
    boat_hurt::hit(w, e, amount, None, false, HurtFlags { invulnerable: false, entity_drops: true });
}

/// A unit draw from the boats' random source, for the sounds P draws (the paddle's pitch).
pub fn draw_unit(w: &mut World) -> f32 {
    unit(snow_rng(w))
}

/// The events queued since the host last took them.
#[derive(Resource, Default)]
pub struct BoatEffectEvents(pub Vec<BoatEffectEvent>);

/// The random source of the snowflakes, made once.
#[derive(Resource)]
pub struct SnowRng(pub Rng);

pub fn queue(w: &mut World, ev: BoatEffectEvent) {
    w.get_resource_or_insert_with(BoatEffectEvents::default).0.push(ev);
}

/// Empties the queue: the host applies what it holds.
pub fn take_effects(w: &mut World) -> Vec<BoatEffectEvent> {
    std::mem::take(&mut w.get_resource_or_insert_with(BoatEffectEvents::default).0)
}

/// `AbstractBoat.onInsideBubbleColumn`'s wobble: grows while the bubble time runs, fades otherwise; `tick` is the boat's tick count.
pub fn wobble_step(w: &mut Wobble, tick: i32) {
    let m = if w.time > 0 { w.multiplier + 0.05 } else { w.multiplier - 0.1 };
    w.multiplier = m.clamp(0.0, 1.0);
    w.angle_old = w.angle;
    w.angle = 10.0 * ((0.5 * f64::from(tick)).sin() as f32) * w.multiplier;
}

/// `Entity.move` of a stuck boat: its velocity times the multiplier, per axis. The caller clears the multiplier after the move.
pub fn stuck_slow(stuck: [f64; 3], vel: [f64; 3]) -> [f64; 3] {
    [vel[0] * stuck[0], vel[1] * stuck[1], vel[2] * stuck[2]]
}

/// The ticks of the next fire deadline: 20 while more than 20 ticks are left, the rest otherwise.
pub fn fire_step(f: i32) -> i32 {
    if f % 20 == 0 { 20 } else { f % 20 }
}

/// The cells a box of a boat at `p` reaches into, deflated by 1e-5 as `Entity.checkInsideBlocks` does, in x, y, z order.
fn cells_of(p: Vec3) -> Vec<[i32; 3]> {
    let e = 1.0e-5;
    let b = bounds_at(p);
    let (lo, hi) = (b.min_position(), b.max_position());
    let mut cells = Vec::new();
    for x in (lo.x + e).floor() as i32..=(hi.x - e).floor() as i32 {
        for y in (lo.y + e).floor() as i32..=(hi.y - e).floor() as i32 {
            for z in (lo.z + e).floor() as i32..=(hi.z - e).floor() as i32 {
                cells.push([x, y, z]);
            }
        }
    }
    cells
}

fn cells_at(w: &World, e: Entity) -> Vec<[i32; 3]> {
    w.get::<Pos>(e).map_or_else(Vec::new, |p| cells_of(p.0))
}

/// The drag of the bubble column under boat `e` (`Some(down)`), None when no column is in its box.
fn column_of(w: &World, e: Entity) -> Option<bool> {
    let mw = world_of(w);
    cells_at(w, e).into_iter().find(|&c| &*mw.facts.get(mw.world.block(c[0], c[1], c[2])).name == "minecraft:bubble_column").map(|c| property(mw, c, "drag").is_some_and(|d| d == "true"))
}

/// Sets the fire of boat `e` to `ticks`; its first deadline is armed when the boat was not burning.
pub fn set_fire(w: &mut World, e: Entity, ticks: i32) {
    let armed = w.get::<Hull>(e).is_some_and(|h| h.fire > 0);
    if let Some(mut h) = w.get_mut::<Hull>(e) {
        h.fire = ticks;
    }
    if ticks > 0 && !armed {
        after(w, e, fire_step(ticks), fire_due);
    }
}

/// The effects of the cells boat `e` reaches into, as the host applies them: the last stuck cell sets the multiplier, fire cells burn it
/// and light it, water puts it out, a column arms its throw, lava is announced, powder snow throws snowflakes.
pub fn inside(w: &mut World, e: Entity, cells: &[[i32; 3]]) {
    let mw = world_of(w);
    let (mut stuck, mut fires, mut snow, mut water, mut lava, mut column) = (None, Vec::new(), Vec::new(), false, false, None);
    for &c in cells {
        let f = mw.facts.get(mw.world.block(c[0], c[1], c[2]));
        let name = &*f.name;
        if name == "minecraft:cobweb" {
            stuck = Some(WEB_STUCK);
        } else if name == "minecraft:powder_snow" {
            stuck = Some(POWDER_SNOW_STUCK);
            snow.push(c);
        }
        match name {
            "minecraft:fire" => fires.push(1.0),
            "minecraft:soul_fire" => fires.push(2.0),
            _ => {}
        }
        water |= f.fluid == Fluid::Water;
        lava |= f.fluid == Fluid::Lava;
        if name == "minecraft:bubble_column" {
            column = Some(property(mw, c, "drag").is_some_and(|d| d == "true"));
        }
    }
    if let Some(m) = stuck {
        if let Some(mut h) = w.get_mut::<Hull>(e) {
            h.stuck = m;
            h.fall = 0.0;
        }
    }
    for damage in fires {
        hurt(w, e, damage);
        let ticks = w.get::<Hull>(e).map_or(0, |h| h.fire).max(FIRE_TICKS);
        set_fire(w, e, ticks);
    }
    if water {
        if let Some(mut h) = w.get_mut::<Hull>(e) {
            h.fire = h.fire.min(0);
        }
    }
    if lava {
        hurt(w, e, LAVA_DAMAGE);
        if let Some(at) = w.get::<Pos>(e).map(|p| [p.0.x, p.0.y, p.0.z]) {
            queue(w, BoatEffectEvent::LavaHurt { at });
        }
    }
    if let Some(down) = column {
        let mut wb = w.get::<Wobble>(e).copied().unwrap_or_default();
        let arm = wb.time == 0;
        if arm {
            wb.time = BUBBLE_TIME;
        }
        wb.above = true;
        wb.down = down;
        w.entity_mut(e).insert(wb);
        if arm {
            after(w, e, BUBBLE_TIME, bubble_due);
        }
    }
    if !snow.is_empty() {
        snowflakes(w, e, &snow);
    }
}

/// The throw of a bubble column, at its deadline: a boat still over a column is thrown up or pulled down, and its next throw is armed.
/// A boat that left the column stops the countdown.
fn bubble_due(w: &mut World, e: Entity) {
    let Some(mut wb) = w.get::<Wobble>(e).copied() else { return };
    if wb.time == 0 {
        return;
    }
    match column_of(w, e) {
        None => {
            wb.time = 0;
            wb.above = false;
            w.entity_mut(e).insert(wb);
        }
        Some(down) => {
            wb.time = BUBBLE_TIME;
            wb.above = true;
            wb.down = down;
            w.entity_mut(e).insert(wb);
            queue(w, BoatEffectEvent::Bubble { entity: e, down });
            after(w, e, BUBBLE_TIME, bubble_due);
        }
    }
}

/// `Entity.baseTick`: a fire hurts every 20 ticks, by 1.
const FIRE_PERIOD: i32 = 20;
const ON_FIRE_DAMAGE: f32 = 1.0;

/// A burning boat's deadline: water puts the fire out, otherwise the fire's remaining ticks go down by `fire_step` and the next deadline
/// is armed. The damage of a block is `inside`'s; the fire's own (1 each 20 ticks) is here.
fn fire_due(w: &mut World, e: Entity) {
    let Some(f) = w.get::<Hull>(e).map(|h| h.fire).filter(|&f| f > 0) else { return };
    let cells = cells_at(w, e);
    let mw = world_of(w);
    if cells.iter().any(|c| mw.facts.get(mw.world.block(c[0], c[1], c[2])).fluid == Fluid::Water) {
        if let Some(mut h) = w.get_mut::<Hull>(e) {
            h.fire = 0;
        }
        return;
    }
    // `Entity.baseTick`: every 20th tick of a fire, out of lava (its own hit is the block's), the `on_fire` source hurts 1.
    if f % FIRE_PERIOD == 0 && !cells.iter().any(|c| mw.facts.get(mw.world.block(c[0], c[1], c[2])).fluid == Fluid::Lava) {
        crate::boat_hurt::hit(w, e, ON_FIRE_DAMAGE, None, false, crate::boat_hurt::HurtFlags { invulnerable: false, entity_drops: true });
        if w.get_entity(e).is_err() {
            return;
        }
    }
    let step = fire_step(f);
    let left = f - step;
    if let Some(mut h) = w.get_mut::<Hull>(e) {
        h.fire = left;
    }
    if left > 0 {
        after(w, e, step, fire_due);
    }
}

fn snow_rng(w: &mut World) -> &mut Rng {
    &mut w.get_resource_or_insert_with(|| SnowRng(Rng(0x2545_F491_4F6C_DD1D))).into_inner().0
}

/// A unit draw from `rng` (24 bits, as `Random.nextFloat`).
fn unit(rng: &mut Rng) -> f32 {
    (rng.next() >> 8) as f32 / 16_777_216.0
}

/// `ServerLevel.sendParticles`' snowflake of a boat that moves: none when it does not move horizontally or on a coin toss, else a
/// small drift of the horizontal speed and a rise of 0.05.
fn snowflake_velocity(known: Vec3, rng: &mut Rng) -> Option<[f64; 3]> {
    if (known.x == 0.0 && known.z == 0.0) || rng.next() & 1 == 0 {
        return None;
    }
    let xa = (unit(rng) * 2.0 - 1.0) * 0.083_333_336_f32;
    let za = (unit(rng) * 2.0 - 1.0) * 0.083_333_336_f32;
    Some([f64::from(xa), f64::from(0.05_f32), f64::from(za)])
}

/// Two snowflake draws per powder snow cell, at the boat's position above each cell.
fn snowflakes(w: &mut World, e: Entity, cells: &[[i32; 3]]) {
    let Some(p) = w.get::<Pos>(e).map(|p| p.0) else { return };
    let known = w.get::<Vel>(e).map_or(Vec3::new(0.0, 0.0, 0.0), |v| v.0);
    for _ in 0..2 {
        for c in cells {
            let v = snowflake_velocity(known, snow_rng(w));
            if let Some(velocity) = v {
                queue(w, BoatEffectEvent::Snowflakes { at: [p.x, f64::from(c[1] + 1), p.z], velocity });
            }
        }
    }
}

/// The boats that move, and the boats beside a block that changed, are read for the cells they reach into; a boat at rest with no
/// change near it is not in any query.
fn boat_cells(w: &mut World) {
    let mut boats: Vec<Entity> = crate::queries::entities::<(With<Boat>, With<Moving>)>(w);
    let changed: Vec<[i32; 3]> = w.get_resource::<BlockEvents>().map_or_else(Vec::new, |b| b.changed.clone());
    for c in changed {
        let mut near = Vec::new();
        if let Some(ix) = w.get_resource::<Index>() {
            ix.for_radius(f64::from(c[0]) + 0.5, f64::from(c[2]) + 0.5, 2.0, |e| near.push(e));
        }
        for e in near {
            if w.get::<Boat>(e).is_some() && !boats.contains(&e) {
                boats.push(e);
            }
        }
    }
    for e in boats {
        let cells = cells_at(w, e);
        inside(w, e, &cells);
    }
}

pub fn register(s: &mut bevy_ecs::schedule::Schedule) {
    s.add_systems(boat_cells.after(block_changes).before(reindex));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The deadlines a fire of `f` ticks goes through, as the sum of their steps.
    fn walk(mut f: i32) -> (i32, usize) {
        let (mut ticks, mut deadlines) = (0, 0);
        while f > 0 {
            let step = fire_step(f);
            ticks += step;
            deadlines += 1;
            f -= step;
        }
        (ticks, deadlines)
    }

    #[test]
    fn a_fire_of_160_ticks_ends_after_eight_deadlines_of_twenty() {
        assert_eq!(walk(160), (160, 8));
    }

    #[test]
    fn a_fire_of_25_ticks_ends_after_a_step_of_5_then_20() {
        assert_eq!(walk(25), (25, 2));
    }

    #[test]
    fn a_stuck_boat_moves_its_velocity_times_the_multiplier_per_axis() {
        assert_eq!(stuck_slow([0.5, 0.25, 0.5], [2.0, 4.0, -2.0]), [1.0, 1.0, -1.0]);
    }

    #[test]
    fn a_boat_that_does_not_move_horizontally_throws_no_snowflake() {
        let mut rng = Rng(7);
        assert_eq!(snowflake_velocity(Vec3::new(0.0, 0.1, 0.0), &mut rng), None);
    }

    #[test]
    fn a_resting_boat_has_no_countdown_or_move_armed() {
        let mut w = World::new();
        let e = crate::boat_state::spawn(&mut w, 0, false, [0.0; 3], 0.0);
        assert!(w.get::<Wobble>(e).is_none());
        assert!(w.get::<Moving>(e).is_none());
    }
}
