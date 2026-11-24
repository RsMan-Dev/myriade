//! The species' synched slots of the second half (greffe L0, lot 06), from `mob_meta_sync`'s branches from creeper to wither. Each
//! reader reads a component that exists in the ECS; a state with none is not here (see the greftes report). The indices are vanilla's
//! (the entry's own index after its parents'), and a reader gives an entry only when the value differs from the default.
use crate::behaviours::{
    armadillo_state, attached_face, burst_shot, cat_state, charged_shot, convert_timer, flier_goals, golem_state, panda_state, random_stand,
    turtle_state,
};
pub use crate::species::sync_b_table::TABLE;
use super::sync::SyncValue;
use crate::{explosion, raids};
use bevy_ecs::prelude::*;
use horizon_protocol::entity_data::serializer;

/// Creeper.DATA_SWELL_DIR: the fuse's direction, -1 (its default, not sent) when it falls.
pub(crate) fn creeper_swell(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<explosion::Fuse>(e).filter(|f| f.dir != -1).map(|f| SyncValue::Int(i32::from(f.dir)))
}

/// Creeper.DATA_IS_POWERED: struck by lightning.
pub(crate) fn creeper_charged(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<explosion::Charged>(e).map(|_| SyncValue::Bool(true))
}

/// Creeper.DATA_IS_IGNITED: lit by an outside hand.
pub(crate) fn creeper_ignited(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<explosion::Fuse>(e).filter(|f| f.ignited).map(|_| SyncValue::Bool(true))
}

/// Raider.IS_CELEBRATING (after Mob's flags).
pub(crate) fn raider_celebrating(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<raids::Celebrating>(e).map(|_| SyncValue::Bool(true))
}

/// Pillager.IS_CHARGING_CROSSBOW.
pub(crate) fn pillager_charging(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<charged_shot::Charging>(e).map(|_| SyncValue::Bool(true))
}

/// Panda.EAT_COUNTER (after AgeableMob's two and the unhappy and sneeze counters).
pub(crate) fn panda_eat(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<panda_state::PandaState>(e).map(|p| p.eat).filter(|v| *v != 0).map(SyncValue::Int)
}

/// The vanilla id of a gene (brown 4, weak 5, aggressive 6).
fn gene_id(g: panda_state::Gene) -> i8 {
    use panda_state::Gene as G;
    match g {
        G::Normal => 0,
        G::Lazy => 1,
        G::Worried => 2,
        G::Playful => 3,
        G::Brown => 4,
        G::Weak => 5,
        G::Aggressive => 6,
    }
}

/// Panda.MAIN_GENE_ID: the main gene (the shown one when a panda has no `Genes`).
pub(crate) fn panda_gene(w: &World, e: Entity) -> Option<SyncValue> {
    let g = w.get::<crate::behaviours::spawn_draw::Genes>(e).map_or_else(|| panda_state::gene(w, e), |g| g.main);
    (gene_id(g) != 0).then_some(SyncValue::Byte(gene_id(g)))
}

/// Panda.HIDDEN_GENE_ID.
pub(crate) fn panda_hidden(w: &World, e: Entity) -> Option<SyncValue> {
    let g = w.get::<crate::behaviours::spawn_draw::Genes>(e)?.hidden;
    (gene_id(g) != 0).then_some(SyncValue::Byte(gene_id(g)))
}

/// Panda.DATA_ID_FLAGS: what the goals set.
pub(crate) fn panda_flags(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<panda_state::PandaState>(e).filter(|p| p.flags != 0).map(|p| SyncValue::Byte(p.flags as i8))
}

/// PolarBear.DATA_STANDING_ID: the bear rears up.
pub(crate) fn polar_bear_standing(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<random_stand::Standing>(e).map(|_| SyncValue::Bool(true))
}

/// Vex.DATA_FLAGS_ID: charging in bit 0.
pub(crate) fn vex_charging(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<flier_goals::Charging>(e).map(|_| SyncValue::Byte(1))
}

/// Ocelot.DATA_TRUSTING.
pub(crate) fn ocelot_trusting(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<cat_state::Trusting>(e).map(|_| SyncValue::Bool(true))
}

/// Shulker.DATA_ATTACH_FACE_ID (0 is the default, the first face).
pub(crate) fn shulker_face(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<attached_face::Attached>(e).filter(|a| a.face != 0).map(|a| SyncValue::Id(serializer::DIRECTION, i32::from(a.face)))
}

/// Shulker.DATA_PEEK_ID: the raw peek the goals set.
pub(crate) fn shulker_peek(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<attached_face::Attached>(e).filter(|a| a.raw != 0).map(|a| SyncValue::Byte(a.raw as i8))
}

/// Shulker.DATA_COLOR_ID (16 for none, the default).
pub(crate) fn shulker_color(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<attached_face::Attached>(e).filter(|a| a.color != 16).map(|a| SyncValue::Byte(a.color as i8))
}

/// Armadillo.ARMADILLO_STATE, IDLE (0) is the default.
pub(crate) fn armadillo(w: &World, e: Entity) -> Option<SyncValue> {
    let state = armadillo_state::of(w, e);
    (state != armadillo_state::IDLE).then_some(SyncValue::Id(serializer::ARMADILLO_STATE, i32::from(state)))
}

/// CopperGolem.DATA_WEATHER_STATE, UNAFFECTED (0) is the default.
pub(crate) fn copper_weather(w: &World, e: Entity) -> Option<SyncValue> {
    let stage = w.get::<crate::coating::Coat>(e).map_or(0, |c| c.stage);
    (stage != 0).then_some(SyncValue::Id(serializer::WEATHERING_COPPER_STATE, i32::from(stage)))
}

/// CopperGolem.COPPER_GOLEM_STATE, IDLE (0) is the default.
pub(crate) fn copper_golem(w: &World, e: Entity) -> Option<SyncValue> {
    let state = golem_state::of(w, e);
    (state != 0).then_some(SyncValue::Id(serializer::COPPER_GOLEM_STATE, i32::from(state)))
}

/// The conversion in progress (a zombie's shaking under water, a skeleton's freezing).
pub(crate) fn converting(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<convert_timer::Converting>(e).map(|_| SyncValue::Bool(true))
}

/// Blaze.DATA_FLAGS_ID: charged 1, which makes it burn.
pub(crate) fn blaze_charged(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<burst_shot::Charged>(e).map(|_| SyncValue::Byte(1))
}

/// Turtle.HAS_EGG.
pub(crate) fn turtle_egg(w: &World, e: Entity) -> Option<SyncValue> {
    turtle_state::has_egg(w, e).then_some(SyncValue::Bool(true))
}

/// Turtle.LAYING_EGG.
pub(crate) fn turtle_laying(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<turtle_state::Turtle>(e).filter(|t| t.laying).map(|_| SyncValue::Bool(true))
}

/// Camel.DASH (index 19, after AbstractHorse's flags): the `Dashing` marker (set by the dash, cleared by `mount::ridden_drive` at the
/// landing once the cooldown is under 50); sent only while true (the default is false).
pub(crate) fn camel_dashing(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<crate::mount::Dashing>(e).map(|_| SyncValue::Bool(true))
}

/// Sniffer.DATA_STATE (index 18, after AgeableMob's two): IDLING (0) is the default.
pub(crate) fn sniffer_state(w: &World, e: Entity) -> Option<SyncValue> {
    let s = crate::brain::behaviours::sniff::state(w, e);
    (s != crate::brain::behaviours::sniff::IDLING).then_some(SyncValue::Id(serializer::SNIFFER_STATE, i32::from(s)))
}

/// Sniffer.DATA_DROP_SEED_AT_TICK (index 19, an int in the sniffer's own `tickCount`): 0 is the default.
pub(crate) fn sniffer_seed_at(w: &World, e: Entity) -> Option<SyncValue> {
    Some(crate::brain::behaviours::sniff::seed_at(w, e)).filter(|t| *t != 0).map(SyncValue::Int)
}

/// Camel.LAST_POSE_CHANGE_TICK (index 20, a long): the game time of the last change of pose, negative while sitting. Always sent: the
/// default is 0 and a standing camel born at game time 0 is the only one that would match it.
pub(crate) fn camel_pose_tick(w: &World, e: Entity) -> Option<SyncValue> {
    crate::behaviours::posture::last_pose_tick(w, e).map(SyncValue::Long)
}

