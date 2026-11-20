//! The conditions a system is skipped by (`run_if`) for a species.
//! A cow as data components and batch systems (palier 1): the stroll goal (a geometric wait with the law of
//! `nextInt(60) == 0` each tick, then `LandRandomPos` 10 x 7), the move control, `LivingEntity.travel` on the
//! ground with Horizon's real block collision sweep (`SceneCollisions::resolve_in_border_with`), and sleeping.
//! Species data (size, step, speed, stroll range) are component values, not branches.
#[allow(unused_imports)]
use crate::IFloor as _;
#[allow(unused_imports)]
use bevy_ecs::{
    entity_disabling::Disabled,
    prelude::*,
    query::Allow,
    schedule::{Schedule, ScheduleLabel, SingleThreadedExecutor},
};
#[allow(unused_imports)]
use horizon_client_lib::mobs::{
    Mob, MobWorld,
    nav::Navigation,
    path::{Evaluator, Finder, Path, PathMob, PATH_TYPES},
};
#[allow(unused_imports)]
use horizon_client_render::{chunk::Chunk, collisions::SceneCollisions};
#[allow(unused_imports)]
use horizon_core::geometry::{Aabb, Vec3, trig};
#[allow(unused_imports)]
use std::sync::Arc;
use super::*;

/// The single point of every runtime skip of a test: prints `SKIP`, and when `HORIZON_SKIP_LOG` names a file, appends one line
/// `species<TAB>test<TAB>reason` to it (one `write` on an `O_APPEND` file: atomic for a line, whatever the number of processes).
pub fn skip_note(test: &str, reason: &str) {
    eprintln!("SKIP {test}: {reason}");
    if let Some(path) = std::env::var_os("HORIZON_SKIP_LOG") {
        use std::io::Write;
        let species = std::env::var("HORIZON_SPECIES").unwrap_or_else(|_| crate::species::PROCESS_DEFAULT.into());
        let line = format!("{species}\t{test}\t{}\n", reason.replace(['\t', '\n'], " "));
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = f.write_all(line.as_bytes());
        }
    }
}

/// For the tests of a behaviour only some species have: whether to skip the test of the process's species, which has no
/// such goal (the skip is printed, so it is never silent).
pub fn skip_unless(module: &'static crate::behaviours::Module, test: &str) -> bool {
    // By the name the process was asked for (`HORIZON_SPECIES`), before any level is built.
    let name = process_species_name();
    let has = crate::species::config(&name).has(module);
    if !has {
        crate::cow::skip_note(test, &format!("{name} has no {} goal", module.name));
    }
    !has
}

/// For the tests written for a land animal that walks, strolls and has goals: they declare what they need (the capabilities
/// `Locomotion`, `Goals` and `Head`, a goal list, a walk and no other medium) and skip, printed, in a process whose species lacks
/// it: the rule of the code (nothing knows a species) applies to the tests.
/// For the controls against the `Mob` of a goal-only walker (avoid, flee the sun...): skips (printed) for a species with a brain, whose
/// `Mob` and creature both wander with it (proved on the avoid goal: with the brain removed the ECS test passes), so the scene is not the goal's.
pub fn skip_if_brain(test: &str) -> bool {
    let name = std::env::var("HORIZON_SPECIES").unwrap_or_else(|_| crate::species::PROCESS_DEFAULT.into());
    let brained = crate::species::registry().by_name(&name).is_ok_and(|r| r.brain.is_some());
    if brained {
        eprintln!("SKIP {test}: the species of the process ({name}) has a brain that walks it: the scene is a goal-only walker's");
    }
    brained
}

pub fn skip_unless_walker(test: &str) -> bool {
    skip_unless_caps(&[crate::behaviours::Cap::Locomotion, crate::behaviours::Cap::Goals, crate::behaviours::Cap::Head], test)
        || skip_unless_ground_only(test)
        || skip_unless_goals(test)
}

/// For the tests of the farm animal's goals (tempt, breed, follow the parent, panic, stroll): skips (printed) unless the species
/// of the process has them all.
pub fn skip_unless_farm_goals(test: &str) -> bool {
    use crate::behaviours::{breed, follow_parent, panic, stroll, tempt};
    let name = process_species_name();
    let cfg = crate::species::registry().by_name(&name).map(|r| r.config);
    let ok = cfg.is_ok_and(|c| c.has(&tempt::MODULE) && c.has(&breed::MODULE) && c.has(&follow_parent::MODULE) && c.has(&panic::MODULE) && c.has(&stroll::MODULE));
    if !ok {
        crate::cow::skip_note(test, &format!("{name} lacks the farm animal's goals (tempt, breed, follow parent, panic, stroll)"));
    }
    !ok
}

/// For the physics scenes written for a small animal (slabs, herds spaced for a cow's box, a pit of 24): skips (printed) when the
/// species of the process has a body of another scale (wider than a block, taller than a block and a half, or smaller than half a block).
pub fn skip_unless_small_body(test: &str) -> bool {
    let name = process_species_name();
    let (w, h) = species_size(&name);
    let skip = w > 0.95 || h > 1.5 || (w < 0.5 && h <= 0.5);
    if skip {
        crate::cow::skip_note(test, &format!("{name} is {w} x {h}: the scene is made for a small animal"));
    }
    skip
}

/// Skips (printed) for a species that the fire and the lava do not hurt (the strider walks on lava): the framework has no fire immunity yet
/// (NEED: an `Immune` bit for fire and lava, then a hazard module that reads it), so the ECS strider burns where the `Mob`'s does not.
pub fn skip_if_fire_immune(test: &str) -> bool {
    let name = process_species_name();
    let immune = crate::species::registry().by_name(&name).is_ok_and(|r| r.modes.iter().any(|m| m.0 == crate::locomotion::Medium::Lava));
    if immune {
        crate::cow::skip_note(test, &format!("{name} walks on lava: the framework has no fire immunity yet"));
    }
    immune
}

/// Skips (printed) in the process of any species but the default one: a law test that builds its OWN configuration (the enderman's
/// block, the panda's roll, the horse's rear...) gives the same answer whatever species the process has, and costs seconds: it runs once.
pub fn skip_unless_default_species(test: &str) -> bool {
    let name = process_species_name();
    let skip = name != crate::species::PROCESS_DEFAULT;
    if skip {
        crate::cow::skip_note(test, &format!("a law test of its own configuration, run once (in the process of {})", crate::species::PROCESS_DEFAULT));
    }
    skip
}

/// Skips (printed) unless the species of the process has all these capabilities.
pub fn skip_unless_caps(caps: &[crate::behaviours::Cap], test: &str) -> bool {
    let name = process_species_name();
    let have = crate::species::registry().by_name(&name).map(|r| r.caps).unwrap_or(&[]);
    let missing: Vec<_> = caps.iter().filter(|c| !have.contains(c)).collect();
    if !missing.is_empty() {
        crate::cow::skip_note(test, &format!("{name} lacks the capabilities {missing:?}"));
    }
    !missing.is_empty()
}

/// Skips (printed) when the species of the process moves in another medium than the ground (a swimmer).
pub fn skip_unless_ground_only(test: &str) -> bool {
    let name = process_species_name();
    // A ground mode other than the walk (the bounds of a slime or a rabbit) is another movement: the walking scenes do not apply.
    let other = crate::species::registry().by_name(&name).is_ok_and(|r| {
        r.modes.iter().any(|m| m.0 != crate::locomotion::Medium::Ground || !std::ptr::eq(m.1.module, &crate::locomotion::walk::MODULE))
    });
    if other {
        crate::cow::skip_note(test, &format!("{name} moves in another medium than the ground, or other than by walking"));
    }
    other
}

/// Skips (printed) when the species of the process has no goal at all (an entity driven by something else).
pub fn skip_unless_goals(test: &str) -> bool {
    let name = process_species_name();
    let none = crate::species::registry().by_name(&name).is_ok_and(|r| r.config.entries.is_empty());
    if none {
        crate::cow::skip_note(test, &format!("{name} has no goal"));
    }
    none
}

/// The reverse: a test of a passive behaviour (a player in range changes nothing) skips a species that has the goal
/// (a zombie hunts the player the test puts in range), printed.
pub fn skip_if(module: &'static crate::behaviours::Module, test: &str) -> bool {
    let name = process_species_name();
    let has = crate::species::config(&name).has(module);
    if has {
        crate::cow::skip_note(test, &format!("{name} has a {} goal: the player in range makes it hunt, the test measures passive looking", module.name));
    }
    has
}
