//! Illagers outside raids: pillagers (crossbow charging and volleys) and
//! vindicators (axes, Johnny), with `IllagerModel`'s arm poses.
use super::{
    MobEvent, Species, Who,
    anim::{Poses, State},
    goals::{
        Cx, Float, Goal, HurtByTarget, LOOK, LookAtPlayer, MOVE, MeleeAttack, NearestTarget, Prey,
        Stroll, goal_clone, reduced,
    },
};
use crate::items::ItemStack;
use crate::projectiles::{Projectile, ProjectileKind};
use horizon_core::geometry::trig::{DEG_TO_RAD, Mth};
use horizon_core::{geometry::Vec3, random::RandomSource};
use std::f32::consts::PI;

/// `AvoidEntityGoal<>(this, Creaking.class, 8.0F, walk, sprint)`.
fn avoid_creaking(walk: f64, sprint: f64) -> super::goals::AvoidEntity {
    super::goals::AvoidEntity::new(
        super::goals::Avoid::Species(vec![Species::Creaking]),
        8.0,
        walk,
        sprint,
    )
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Spell {
    Vexes = 1,
    Fangs = 2,
    Wololo = 3,
}
/// `IllagerSpell.spellColor` as `(float)` red, green and blue, by spell id
/// (1 summon vex, 2 fangs, 3 wololo, 4 disappear, 5 blindness).
fn spell_color(id: u8) -> [f32; 3] {
    match id {
        1 => [0.7, 0.7, 0.8],
        2 => [0.4, 0.3, 0.35],
        3 => [0.7, 0.5, 0.2],
        4 => [0.3, 0.3, 0.8],
        5 => [0.1, 0.1, 0.2],
        _ => [0.0; 3],
    }
}
/// `SpellcasterCastingSpellGoal`: stand still facing the target while a
/// spell is cast.
#[derive(Clone, Debug)]
pub(super) struct CastingSpell;
impl Goal for CastingSpell {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE | LOOK
    }
}
/// `SpellcasterUseSpellGoal` with the evoker's three spells.
#[derive(Clone, Debug)]
struct UseSpell {
    spell: Spell,
    warmup: i32,
    next: i64,
}
impl UseSpell {
    fn new(spell: Spell) -> Self {
        Self {
            spell,
            warmup: 0,
            next: 0,
        }
    }
    /// (warm-up, casting time, interval, prepare sound).
    fn timing(&self) -> (i32, i32, i64, &'static str) {
        match self.spell {
            Spell::Vexes => (20, 100, 340, "entity.evoker.prepare_summon"),
            Spell::Fangs => (20, 40, 100, "entity.evoker.prepare_attack"),
            Spell::Wololo => (40, 60, 140, "entity.evoker.prepare_wololo"),
        }
    }
}
impl Goal for UseSpell {
    goal_clone!();
    fn flags(&self) -> u8 {
        0
    }
}
/// `createSpellEntity`: the first block below `max_y` whose up face is
/// sturdy, standing on the top of whatever collision shape is in the way.
///
/// The floor test is `BlockState.isFaceSturdy(.., UP)`, not a full cube: a
/// soul sand or mud block is 14/16 high and still supports a fang, where
/// requiring a full collision shape drops the fang into the block.
fn fang_floor(w: &super::MobWorld, x: f64, z: f64, min_y: f64, max_y: f64) -> Option<f64> {
    let (bx, bz) = (x.floor() as i32, z.floor() as i32);
    let mut y = max_y.floor() as i32;
    while y >= min_y.floor() as i32 - 1 {
        let below = w.world.block(bx, y - 1, bz);
        if w.data.state(below).flags & super::data::STURDY_TOP != 0 {
            let here = w.world.block(bx, y, bz);
            let top = if w.facts.is_air(here) {
                0.0
            } else {
                f64::from(w.data.state(here).top)
            };
            return Some(f64::from(y) + top);
        }
        y -= 1;
    }
    None
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Crossbow {
    Uncharged,
    Charging,
    Charged,
    Ready,
}
/// `RangedCrossbowAttackGoal`.
#[derive(Clone, Debug)]
struct CrossbowAttack {
    speed: f64,
    radius_sqr: f64,
    state: Crossbow,
    see_time: i32,
    attack_delay: i32,
    path_delay: i32,
}
impl CrossbowAttack {
    fn new(speed: f64, radius: f64) -> Self {
        Self {
            speed,
            radius_sqr: radius * radius,
            state: Crossbow::Uncharged,
            see_time: 0,
            attack_delay: 0,
            path_delay: 0,
        }
    }
}
/// The crossbow goal for other crossbow users (piglins).
pub fn crossbow_goal(speed: f64, radius: f64) -> impl Goal {
    CrossbowAttack::new(speed, radius)
}

impl Goal for CrossbowAttack {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE | LOOK
    }
    fn every_tick(&self) -> bool {
        true
    }
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ArmPose {
    BowAndArrow,
    Spellcasting,
    Crossed,
    Celebrating,
    Neutral,
    Attacking,
    CrossbowHold,
    CrossbowCharge,
    Bow,
}
fn hide(p: &mut Poses, part: &str) {
    p.visible(part, false);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobs::Species;
    use crate::mobs::sensor_scan_tests::kind;
    use std::sync::Arc;


    /// `SpellcasterIllager.tick`: two coloured particles at the hands while a
    /// spell is cast, none otherwise; the colours are `IllagerSpell.spellColor`.
    #[test]
    fn a_casting_evoker_has_a_particle_in_each_hand() {
        let mut events = Vec::new();
        cast_particles(&evoker(0), &mut events);
        assert!(events.is_empty());
        for (id, want) in [
            (1, [0.7, 0.7, 0.8]),
            (2, [0.4, 0.3, 0.35]),
            (3, [0.7, 0.5, 0.2]),
            (4, [0.3, 0.3, 0.8]),
            (5, [0.1, 0.1, 0.2]),
        ] {
            events.clear();
            cast_particles(&evoker(id), &mut events);
            assert_eq!(events.len(), 2);
            let hands: Vec<([f64; 3], [f32; 3])> = events
                .iter()
                .map(|e| match e {
                    MobEvent::ParticleColored { at, color } => (*at, *color),
                    _ => panic!("not a coloured particle"),
                })
                .collect();
            assert_eq!(hands[0].1, want);
            assert_eq!(hands[1].1, want);
            // Both at y + 1.8, mirrored about the mob, 0.6 away.
            assert!((hands[0].0[1] - 65.8).abs() < 1e-9);
            assert!((hands[0].0[0] + hands[1].0[0] - 20.0).abs() < 1e-9);
            assert!((hands[0].0[2] + hands[1].0[2] + 10.0).abs() < 1e-9);
            let (dx, dz) = (hands[0].0[0] - 10.0, hands[0].0[2] + 5.0);
            assert!((dx.hypot(dz) - 0.6).abs() < 1e-6);
        }
    }



    /// `CrossbowItem.setCharged`: the arrow lives in the stack's components,
    /// and the model draws whatever the stack carries.
    #[test]
    fn a_charged_crossbow_carries_its_arrow() {
        let mut m = pillager();
        m.extra
            .set_main_hand_stack(Some(ItemStack::new("minecraft:crossbow", 1)));
        set_crossbow_charged(&mut m, true);
        let stack = m.extra.main_hand_stack().unwrap();
        let charged = &stack.data().unwrap().charged;
        assert_eq!(charged.len(), 1);
        assert_eq!(&*charged[0].item, "minecraft:arrow");
        set_crossbow_charged(&mut m, false);
        assert!(
            m.extra
                .main_hand_stack()
                .unwrap()
                .data()
                .unwrap()
                .charged
                .is_empty()
        );
    }

    /// A shot crossbow keeps its stack; only the loaded arrow goes.
    #[test]
    fn charging_leaves_anything_but_a_crossbow_alone() {
        let mut m = pillager();
        m.extra
            .set_main_hand_stack(Some(ItemStack::new("minecraft:iron_sword", 1)));
        set_crossbow_charged(&mut m, true);
        assert!(
            m.extra
                .main_hand_stack()
                .unwrap()
                .data()
                .is_none_or(|d| d.charged.is_empty())
        );
    }
    /// `EvokerAttackSpellGoal.createSpellEntity` asks the block below for a
    /// sturdy up face, not for a full cube. Soul sand is 14/16 high and still
    /// supports a fang, which a full-collision test drops into the block.
    #[test]
    #[ignore = "daily"]
    fn a_fang_stands_on_a_soul_sand_floor() {
        let build = |world: &mut crate::world::World,
                     registry: &horizon_world::registry::BlockRegistry| {
            let stone = registry.default_state("minecraft:stone").unwrap();
            let soul_sand = registry.default_state("minecraft:soul_sand").unwrap();
            for cx in -1..1 {
                for cz in -1..1 {
                    world
                        .chunks
                        .insert((cx, cz), Arc::new(crate::chunk::Chunk::empty(cx, cz)));
                }
            }
            for x in -4..4 {
                for z in -4..4 {
                    world.set(x, 63, z, stone).unwrap();
                }
            }
            world.set(0, 64, 0, soul_sand).unwrap();
        };
        let ran = crate::mobs::world::with_built_test_world(&[], build, |w, _| {
            // An evoker standing on the soul sand has its feet at 64.875, so
            // the cast asks for a floor from 65 down: the soul sand at 64 is
            // the first sturdy block and the fang sits at 65.
            assert_eq!(fang_floor(w, 0.5, 0.5, 64.0, 65.875), Some(65.0));
        });
        assert!(ran, "vanilla data is missing");
    }

}
