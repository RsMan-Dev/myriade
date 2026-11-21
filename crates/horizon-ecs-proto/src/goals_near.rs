//! Helpers the goal modules call through `goals::`: repellents, a player's held item, the head, a spot away.
use crate::IFloor as _;
use crate::area::species_name;
use crate::blocks::world_of;
use crate::brain;
use crate::cow::{Index, Pos};
use crate::goals::Online;
use crate::items;
use crate::spots::{self, Pick};
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;

fn arr(v: Vec3) -> [f64; 3] {
    [v.x, v.y, v.z]
}
fn dist2(a: [f64; 3], b: [f64; 3]) -> f64 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}
fn nearer(best: &mut Option<(f64, [f64; 3])>, at: [f64; 3], p: [f64; 3]) {
    let d = dist2(at, p);
    if best.is_none_or(|(b, _)| d < b) {
        *best = Some((d, p));
    }
}

/// The nearest repellent of `e`: a block named in `blocks` in the box of `horizontal` blocks (`vertical` up and down), or a
/// creature named in `species` in the same box. Its centre (blocks) or its position (creatures).
pub fn repellent_near(
    w: &World,
    e: Entity,
    blocks: &[&str],
    species: &[&str],
    horizontal: f64,
    vertical: i32,
) -> Option<[f64; 3]> {
    let at = arr(w.get::<Pos>(e)?.0);
    let mut best = None;
    let r = horizontal.ifloor();
    let (cx, cy, cz) = (at[0].ifloor(), at[1].ifloor(), at[2].ifloor());
    if !blocks.is_empty() {
        let mw = world_of(w);
        for x in cx - r..=cx + r {
            for z in cz - r..=cz + r {
                for y in cy - vertical..=cy + vertical {
                    if blocks.contains(&mw.block_name(x, y, z)) {
                        nearer(&mut best, at, [f64::from(x) + 0.5, f64::from(y) + 0.5, f64::from(z) + 0.5]);
                    }
                }
            }
        }
    }
    if !species.is_empty() {
        w.resource::<Index>().for_radius(at[0], at[2], horizontal, |o| {
            if o == e {
                return;
            }
            let Some(p) = w.get::<Pos>(o).map(|p| arr(p.0)) else { return };
            let inside = (p[0] - at[0]).abs() <= horizontal
                && (p[2] - at[2]).abs() <= horizontal
                && (p[1] - at[1]).abs() <= f64::from(vertical);
            if inside && species.contains(&species_name(w, o)) {
                nearer(&mut best, at, p);
            }
        });
    }
    best.map(|(_, p)| p)
}

/// The eye of the nearest player within `range` that holds one of the `items` (names or `#tags`), in either hand.
pub fn nearest_player_holding(w: &World, e: Entity, items: &[&str], range: f64) -> Option<[f64; 3]> {
    let at = arr(w.get::<Pos>(e)?.0);
    let mut best = None;
    w.resource::<Online>().for_near(at[0], at[2], range, |p| {
        let eye = [p.x, p.eye(), p.z];
        let holds = items.iter().any(|n| items::is(n, p.main) || items::is(n, p.off));
        // `PiglinAi.isPlayerHoldingLovedItem`: the distance is to the feet, and the player must be in sight (`can_see_player`).
        if holds && dist2(at, [p.x, p.y, p.z]) <= range * range && crate::targeting::sees(w, e, crate::targeting::Who::Player(p.id)) {
            nearer(&mut best, at, eye);
        }
    });
    best.map(|(_, p)| p)
}

/// The head turns toward `at` (`brain::look_at`: the look memory of the Mob).
pub fn look_at(w: &mut World, e: Entity, at: [f64; 3]) {
    brain::look_at(w, e, at);
}

/// `LandRandomPos.getPosAway` between 0 and `distance` blocks of `from`, `vertical` up and down (`spots::pick_for`).
pub fn land_random_pos_away(w: &mut World, e: Entity, from: Vec3, distance: f64, vertical: i32) -> Option<[f64; 3]> {
    let from = arr(from);
    spots::pick_for(w, e, Pick::LandAway { min_h: 0.0, max_h: distance, v: vertical, from })
}
