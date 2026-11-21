//! The species' synched slots of the second half (greffe L0, lot 06), from `mob_meta_sync`'s branches from creeper to wither. Each
//! reader reads a component that exists in the ECS; a state with none is not here (see the greftes report). The indices are vanilla's
//! (the entry's own index after its parents'), and a reader gives an entry only when the value differs from the default.
use crate::behaviours::{
    armadillo_state, attached_face, burst_shot, cat_state, charged_shot, convert_timer, flier_goals, golem_state, panda_state, random_stand,
    turtle_state,
};
use super::sync::{SyncSlot, SyncValue};
use crate::{explosion, raids};
use bevy_ecs::prelude::*;
use horizon_protocol::entity_data::serializer;

/// Creeper.DATA_SWELL_DIR: the fuse's direction, -1 when it falls (sent always, as the Mob does).
fn creeper_swell(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<explosion::Fuse>(e).map(|f| SyncValue::Int(i32::from(f.dir)))
}

/// Creeper.DATA_IS_POWERED: struck by lightning.
fn creeper_charged(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<explosion::Charged>(e).map(|_| SyncValue::Bool(true))
}

/// Creeper.DATA_IS_IGNITED: lit by an outside hand.
fn creeper_ignited(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<explosion::Fuse>(e).filter(|f| f.ignited).map(|_| SyncValue::Bool(true))
}

/// Raider.IS_CELEBRATING (after Mob's flags).
fn raider_celebrating(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<raids::Celebrating>(e).map(|_| SyncValue::Bool(true))
}

/// Pillager.IS_CHARGING_CROSSBOW.
fn pillager_charging(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<charged_shot::Charging>(e).map(|_| SyncValue::Bool(true))
}

/// Panda.EAT_COUNTER (after AgeableMob's two and the unhappy and sneeze counters).
fn panda_eat(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<panda_state::PandaState>(e).map(|p| p.eat).filter(|v| *v != 0).map(SyncValue::Int)
}

/// Panda.MAIN_GENE_ID, the gene the panda shows (vanilla's ids: brown 4 has no component here).
fn panda_gene(w: &World, e: Entity) -> Option<SyncValue> {
    let id = match w.get::<panda_state::Gene>(e).copied().unwrap_or_default() {
        panda_state::Gene::Normal => 0,
        panda_state::Gene::Lazy => 1,
        panda_state::Gene::Worried => 2,
        panda_state::Gene::Playful => 3,
        panda_state::Gene::Brown => 4,
        panda_state::Gene::Weak => 5,
        panda_state::Gene::Aggressive => 6,
    };
    (id != 0).then_some(SyncValue::Byte(id))
}

/// Panda.DATA_ID_FLAGS: what the goals set.
fn panda_flags(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<panda_state::PandaState>(e).filter(|p| p.flags != 0).map(|p| SyncValue::Byte(p.flags as i8))
}

/// PolarBear.DATA_STANDING_ID: the bear rears up.
fn polar_bear_standing(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<random_stand::Standing>(e).map(|_| SyncValue::Bool(true))
}

/// Vex.DATA_FLAGS_ID: charging in bit 0.
fn vex_charging(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<flier_goals::Charging>(e).map(|_| SyncValue::Byte(1))
}

/// Ocelot.DATA_TRUSTING.
fn ocelot_trusting(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<cat_state::Trusting>(e).map(|_| SyncValue::Bool(true))
}

/// Shulker.DATA_ATTACH_FACE_ID (0 is the default, the first face).
fn shulker_face(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<attached_face::Attached>(e).filter(|a| a.face != 0).map(|a| SyncValue::Id(serializer::DIRECTION, i32::from(a.face)))
}

/// Shulker.DATA_PEEK_ID: the raw peek the goals set.
fn shulker_peek(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<attached_face::Attached>(e).filter(|a| a.raw != 0).map(|a| SyncValue::Byte(a.raw as i8))
}

/// Shulker.DATA_COLOR_ID (16 for none, the default).
fn shulker_color(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<attached_face::Attached>(e).filter(|a| a.color != 16).map(|a| SyncValue::Byte(a.color as i8))
}

/// Armadillo.ARMADILLO_STATE, IDLE (0) is the default.
fn armadillo(w: &World, e: Entity) -> Option<SyncValue> {
    let state = armadillo_state::of(w, e);
    (state != armadillo_state::IDLE).then_some(SyncValue::Id(serializer::ARMADILLO_STATE, i32::from(state)))
}

/// CopperGolem.DATA_WEATHER_STATE, UNAFFECTED (0) is the default.
fn copper_weather(w: &World, e: Entity) -> Option<SyncValue> {
    let stage = w.get::<crate::coating::Coat>(e).map_or(0, |c| c.stage);
    (stage != 0).then_some(SyncValue::Id(serializer::WEATHERING_COPPER_STATE, i32::from(stage)))
}

/// CopperGolem.COPPER_GOLEM_STATE, IDLE (0) is the default.
fn copper_golem(w: &World, e: Entity) -> Option<SyncValue> {
    let state = golem_state::of(w, e);
    (state != 0).then_some(SyncValue::Id(serializer::COPPER_GOLEM_STATE, i32::from(state)))
}

/// The conversion in progress (a zombie's shaking under water, a skeleton's freezing).
fn converting(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<convert_timer::Converting>(e).map(|_| SyncValue::Bool(true))
}

/// Blaze.DATA_FLAGS_ID: charged 1, which makes it burn.
fn blaze_charged(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<burst_shot::Charged>(e).map(|_| SyncValue::Byte(1))
}

/// Turtle.HAS_EGG.
fn turtle_egg(w: &World, e: Entity) -> Option<SyncValue> {
    turtle_state::has_egg(w, e).then_some(SyncValue::Bool(true))
}

/// Turtle.LAYING_EGG.
fn turtle_laying(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<turtle_state::Turtle>(e).filter(|t| t.laying).map(|_| SyncValue::Bool(true))
}

/// Camel.DASH (index 19, after AbstractHorse's flags): the `Dashing` marker (set by the dash, cleared by `mount::ridden_drive` at the
/// landing once the cooldown is under 50); sent only while true (the default is false).
fn camel_dashing(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<crate::mount::Dashing>(e).map(|_| SyncValue::Bool(true))
}

/// Sniffer.DATA_STATE (index 18, after AgeableMob's two): IDLING (0) is the default.
fn sniffer_state(w: &World, e: Entity) -> Option<SyncValue> {
    let s = crate::brain::behaviours::sniff::state(w, e);
    (s != crate::brain::behaviours::sniff::IDLING).then_some(SyncValue::Id(serializer::SNIFFER_STATE, i32::from(s)))
}

/// Sniffer.DATA_DROP_SEED_AT_TICK (index 19, an int in the sniffer's own `tickCount`): 0 is the default.
fn sniffer_seed_at(w: &World, e: Entity) -> Option<SyncValue> {
    Some(crate::brain::behaviours::sniff::seed_at(w, e)).filter(|t| *t != 0).map(SyncValue::Int)
}

/// Camel.LAST_POSE_CHANGE_TICK (index 20, a long): the game time of the last change of pose, negative while sitting. Always sent: the
/// default is 0 and a standing camel born at game time 0 is the only one that would match it.
fn camel_pose_tick(w: &World, e: Entity) -> Option<SyncValue> {
    crate::behaviours::posture::last_pose_tick(w, e).map(SyncValue::Long)
}

/// Each species' slots, by registry id (a species not listed has none here).
pub static TABLE: &[(&str, &[SyncSlot])] = &[
    ("minecraft:camel", &[SyncSlot { index: 19, read: camel_dashing }, SyncSlot { index: 20, read: camel_pose_tick }]),
    ("minecraft:camel_husk", &[SyncSlot { index: 19, read: camel_dashing }, SyncSlot { index: 20, read: camel_pose_tick }]),
    (
        "minecraft:creeper",
        &[
            SyncSlot { index: 16, read: creeper_swell },
            SyncSlot { index: 17, read: creeper_charged },
            SyncSlot { index: 18, read: creeper_ignited },
        ],
    ),
    (
        "minecraft:pillager",
        &[SyncSlot { index: 16, read: raider_celebrating }, SyncSlot { index: 17, read: pillager_charging }],
    ),
    (
        "minecraft:panda",
        &[
            SyncSlot { index: 20, read: panda_eat },
            SyncSlot { index: 21, read: panda_gene },
            SyncSlot { index: 23, read: panda_flags },
        ],
    ),
    ("minecraft:polar_bear", &[SyncSlot { index: 18, read: polar_bear_standing }]),
    ("minecraft:evoker", &[SyncSlot { index: 16, read: raider_celebrating }]),
    ("minecraft:illusioner", &[SyncSlot { index: 16, read: raider_celebrating }]),
    ("minecraft:vex", &[SyncSlot { index: 16, read: vex_charging }]),
    ("minecraft:ocelot", &[SyncSlot { index: 18, read: ocelot_trusting }]),
    (
        "minecraft:shulker",
        &[
            SyncSlot { index: 16, read: shulker_face },
            SyncSlot { index: 17, read: shulker_peek },
            SyncSlot { index: 18, read: shulker_color },
        ],
    ),
    ("minecraft:copper_golem", &[SyncSlot { index: 16, read: copper_weather }, SyncSlot { index: 17, read: copper_golem }]),
    ("minecraft:sniffer", &[SyncSlot { index: 18, read: sniffer_state }, SyncSlot { index: 19, read: sniffer_seed_at }]),
    ("minecraft:armadillo",&[SyncSlot { index: 18, read: armadillo }]),
    ("minecraft:zombie", &[SyncSlot { index: 18, read: converting }]),
    ("minecraft:husk", &[SyncSlot { index: 18, read: converting }]),
    ("minecraft:skeleton", &[SyncSlot { index: 16, read: converting }]),
    ("minecraft:blaze", &[SyncSlot { index: 16, read: blaze_charged }]),
    (
        "minecraft:turtle",
        &[SyncSlot { index: 18, read: turtle_egg }, SyncSlot { index: 19, read: turtle_laying }],
    ),
];
