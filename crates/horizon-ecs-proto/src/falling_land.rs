//! The landing of a falling block: the damage it does to the living entities in its box (`FallingBlockEntity.causeFallDamage`; the rule is
//! `setHurtsEntities` of the jar: an anvil 2 a step at most 40, a stalactite 1 a step times its height at most 40), the wear of an anvil that
//! hurt, and where the block goes: placed where it stands when that cell is free over a solid floor, else dropped as its item.
use crate::cow::{Bounds, Cause, DamageQueue, Health, Hit, Index, Pos};
use crate::falling_block::FallingBlock;
use crate::projectile_more::{ensure_rng, ThrownRng};
use crate::rules::falling_block::{anvil_steps, anvil_wear_chance, anvil_worn_name};
use bevy_ecs::prelude::*;
use horizon_core::random::RandomSource;
use myriade_engine_lib::blocks::Fluid;
use myriade_engine_lib::items::ItemStack;
use horizon_world::registry::BlockStateId;

/// Where a landed falling block goes: placed with its state at its cell, dropped as the item of its state, or broken with nothing placed
/// (an anvil destroyed by its own fall).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Landing {
    Place(BlockStateId, [i32; 3]),
    Drop(BlockStateId),
    Break(BlockStateId),
}

fn is_stalactite(name: &str) -> bool {
    matches!(name, "minecraft:pointed_dripstone" | "minecraft:sulfur_spike")
}

/// The damage rule of a falling block named `name`: the damage a step of fall does and its cap; None for a block that hurts nothing.
pub fn hurt_rule(name: &str) -> Option<(f32, i32)> {
    if name.ends_with("anvil") {
        Some((2.0, 40))
    } else if is_stalactite(name) {
        // `spawnFallingStalactite`: 1 a step times the stalactite's height, at least 6; a single cell gives the minimum.
        Some((6.0, 40))
    } else {
        None
    }
}

/// The damage `causeFallDamage` deals for a fall of `fell`: `min(floor(ceil(fell - 1) * per_step), cap)`, none under a block's first step.
pub fn fall_damage(per: f32, cap: i32, fell: f64) -> f32 {
    let steps = anvil_steps(fell);
    if steps < 0 {
        return 0.0;
    }
    ((steps as f32 * per).floor() as i32).min(cap) as f32
}

fn damage_kind(name: &str) -> &'static str {
    if name.ends_with("anvil") {
        "minecraft:falling_anvil"
    } else if is_stalactite(name) {
        "minecraft:falling_stalactite"
    } else {
        crate::species::lists::FALLING_BLOCK
    }
}

/// The placement rule of a landed block: its cell is free and the cell under it is not.
pub fn settles(here_free: bool, below_free: bool) -> bool {
    here_free && !below_free
}

/// The item a block of `state` drops when it breaks (`Entity.spawnAtLocation`); None for air.
pub fn drop_stack(w: &World, state: BlockStateId) -> Option<ItemStack> {
    let item = crate::blocks::world_of(w).facts.get(state).item.clone();
    (&*item != "minecraft:air").then(|| ItemStack { item, count: 1, damage: 0, extra: None })
}

/// `FallingBlockEntity.causeFallDamage` at the landing of `e`: the living entities in its box take the damage, as `Hurt` events for the
/// host. The candidates come from the creature index around the box, never from a scan of the entities.
pub fn hurt_below(w: &mut World, e: Entity) {
    let Some(b) = w.get::<FallingBlock>(e).cloned() else { return };
    let name = crate::blocks::world_of(w).facts.get(b.state).name.to_string();
    let amount = fall_damage(b.settings.fall_hurt_amount, b.settings.fall_hurt_max, b.fell);
    let (Some(area), Some(at)) = (w.get::<Bounds>(e).map(|x| x.0), w.get::<Pos>(e).map(|p| p.0)) else { return };
    if amount <= 0.0 || !b.settings.hurt_entities {
        return;
    }
    let at = [at.x, at.y, at.z];
    let (mn, mx, c) = (area.min_position(), area.max_position(), area.center());
    let mut near = Vec::new();
    w.resource::<Index>().for_near_touching(c.x, c.z, (mn.x, mn.z), (mx.x, mx.z), |v| near.push(v));
    let kind = damage_kind(&name);
    let victims: Vec<Entity> = near.into_iter().filter(|&v| v != e && w.get::<Health>(v).is_some() && w.get::<Bounds>(v).is_some_and(|x| x.0.intersects(area))).collect();
    // The blow goes through the damage queue as a projectile's does, under the jar's damage type; the players are not in the ECS, so
    // no `FallingEvent::Hurt` is needed for them here.
    for victim in victims {
        w.resource_mut::<DamageQueue>().0.push(Hit { target: victim, amount, cause: Cause::Entity, damage: kind, from: Some((at[0], at[2])), by: None, direct: false, power: None, extra: None, projectile: None });
    }
}

/// The landing of `e`, a falling block on the ground: an anvil that hurt wears (one step with `anvil_wear_chance`, then it is destroyed),
/// and its cell decides (`settles`): placed there, else dropped.
pub fn land(w: &mut World, e: Entity) -> Landing {
    let b = w.get::<FallingBlock>(e).cloned().expect("a landed falling block has its body");
    let at = w.get::<Pos>(e).map(|p| p.0).expect("a landed falling block has its position");
    let cell = [at.x.floor() as i32, at.y.floor() as i32, at.z.floor() as i32];
    let mw = crate::blocks::world_of(w);
    let name = mw.facts.get(b.state).name.to_string();
    let mut state = b.state;
    if name.ends_with("anvil") && fall_damage(b.settings.fall_hurt_amount, b.settings.fall_hurt_max, b.fell) > 0.0 {
        ensure_rng(w);
        if w.resource_mut::<ThrownRng>().0.next_float() < anvil_wear_chance(anvil_steps(b.fell)) {
            let kind = name.strip_prefix("minecraft:").and_then(|n| n.strip_suffix("anvil")).unwrap_or("");
            match anvil_worn_name(kind).and_then(|n| crate::blocks::resolve(mw, n, &[])) {
                Some(worn) => state = worn,
                None => return Landing::Break(state),
            }
        }
    }
    let (here, below) = (mw.world.block(cell[0], cell[1], cell[2]), mw.world.block(cell[0], cell[1] - 1, cell[2]));
    let free = |s| mw.facts.is_air(s) || mw.facts.get(s).replaceable;
    // `ConcretePowderBlock`: powder that lands in water sets as its concrete.
    let state = match &*mw.facts.get(state).name {
        n if n.ends_with("_concrete_powder") && mw.facts.get(here).fluid == Fluid::Water => crate::blocks::resolve(mw, &n.replace("_powder", ""), &[]).unwrap_or(state),
        _ => state,
    };
    if settles(free(here), free(below)) {
        Landing::Place(state, cell)
    } else {
        Landing::Drop(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_block_settles_on_a_free_cell_over_a_solid_floor() {
        assert!(settles(true, false));
    }

    #[test]
    fn a_block_on_a_torch_is_not_placed() {
        assert!(!settles(false, false));
    }

    #[test]
    fn an_anvil_hurts_two_a_step_at_most_forty() {
        assert_eq!(fall_damage(2.0, 40, 5.0), 8.0);
        assert_eq!(fall_damage(2.0, 40, 30.0), 40.0);
        assert_eq!(fall_damage(2.0, 40, 0.5), 0.0);
    }

    #[test]
    fn a_block_with_no_damage_per_step_hurts_nothing() {
        assert_eq!(fall_damage(0.0, 40, 5.0), 0.0);
    }
}
