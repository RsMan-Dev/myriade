//! A mod's edits of a creature's goals on `EcsLevel`: `GoalSpec` (the API's declarative vanilla goals) turned into entries of the ECS
//! selector (`horizon_ecs_proto::goal_edit`), the goals read back under the API's ids. One table, no species name.
//! Without an equivalent module in the ECS (a gap, answered by false): `Stroll` without `water_avoiding` (the ECS stroll is always the
//! water-avoiding one), with `swim` or `flying`; `LookAtPlayer` with `mobs`, `only` or `only_horizontal`; `Breed` (the foods come from
//! the species); `Tempt` with `can_scare`; `NearestTarget` with `must_reach`.
use super::level::EcsLevel;
use horizon_ecs_proto::behaviours::{
    Entry, Module, avoid_entity::{self, Avoid}, breath_air, breed, climb_on_powder_snow, eat_block, flee_sun, float, follow_mob,
    follow_parent, hurt_by, leap_at_target, look_at_player, melee, nearest_target, panic, random_look, restrict_sun, stroll, tempt,
    try_find_water,
};
use myriade_engine_lib::mobs::MobWorld;
use mod_api::GoalSpec;
use mod_api::MobFilter;
use mod_api::MobGoalEntry;
use std::{any::Any, collections::HashMap, sync::Mutex};

/// The API id of each module a `GoalSpec` builds (and the species' own goals of those modules); another module reads under its name.
static IDS: &[(&Module, &str)] = &[
    (&float::MODULE, "float"),
    (&climb_on_powder_snow::MODULE, "climb_on_top_of_powder_snow"),
    (&panic::MODULE, "panic"),
    (&stroll::MODULE, "stroll"),
    (&look_at_player::MODULE, "look_at_player"),
    (&random_look::MODULE, "random_look_around"),
    (&melee::MODULE, "melee_attack"),
    (&leap_at_target::MODULE, "leap_at_target"),
    (&avoid_entity::MODULE, "avoid_entity"),
    (&tempt::MODULE, "tempt"),
    (&breed::MODULE, "breed"),
    (&follow_parent::MODULE, "follow_parent"),
    (&follow_mob::MODULE, "follow_mob"),
    (&restrict_sun::MODULE, "restrict_sun"),
    (&flee_sun::MODULE, "flee_sun"),
    (&eat_block::MODULE, "eat_block"),
    (&breath_air::MODULE, "breath_air"),
    (&try_find_water::MODULE, "try_find_water"),
    (&nearest_target::MODULE, "nearest_target"),
    (&hurt_by::MODULE, "hurt_by_target"),
];

pub(super) fn api_id(m: &'static Module) -> &'static str {
    IDS.iter().find(|(k, _)| std::ptr::eq(*k, m)).map_or(m.name, |(_, id)| id)
}

fn ns(id: &str) -> String {
    if id.contains(':') { id.to_owned() } else { format!("minecraft:{id}") }
}

/// Equal specs share one allocation of parameters (and so one interned list in the ECS).
fn leak<T: Any + Sync>(spec: &GoalSpec, params: T) -> &'static (dyn Any + Sync) {
    static PARAMS: Mutex<Option<HashMap<String, &'static (dyn Any + Sync)>>> = Mutex::new(None);
    let mut map = PARAMS.lock().unwrap();
    let found: &'static (dyn Any + Sync) = *map
        .get_or_insert_with(Default::default)
        .entry(format!("{spec:?}"))
        .or_insert_with(|| &*Box::leak(Box::new(params)));
    found
}

fn names(ids: &[String]) -> &'static [&'static str] {
    let v: Vec<&'static str> = ids.iter().map(|id| &*Box::leak(ns(id).into_boxed_str())).collect();
    Box::leak(v.into_boxed_slice())
}

fn speed_ok(v: f64) -> bool {
    v.is_finite() && v >= 0.0
}

/// The entry of `spec` at `priority`; None for the numbers vanilla's constructor does not take and for the gaps above.
fn entry_of(spec: &GoalSpec, priority: i32) -> Option<Entry> {
    let priority = u8::try_from(priority).ok()?;
    let (module, params): (&'static Module, &'static (dyn Any + Sync)) = match spec {
        GoalSpec::Float => (&float::MODULE, leak(spec, float::Params::DEFAULT)),
        GoalSpec::ClimbOnTopOfPowderSnow => (&climb_on_powder_snow::MODULE, leak(spec, climb_on_powder_snow::Params::DEFAULT)),
        GoalSpec::Panic { speed, water_range } => {
            if !speed_ok(*speed) || water_range.is_some_and(|r| r < 0) {
                return None;
            }
            (&panic::MODULE, leak(spec, panic::Params { speed: *speed, water: *water_range, ..panic::Params::DEFAULT }))
        }
        GoalSpec::Stroll { speed, interval, water_avoiding: Some(p), swim: false, flying: false } => {
            if !speed_ok(*speed) || *interval < 0 || !p.is_finite() || !(0.0..=1.0).contains(p) {
                return None;
            }
            (&stroll::MODULE, leak(spec, stroll::Params { speed: *speed, interval: Some(*interval), water_avoiding: *p }))
        }
        GoalSpec::LookAtPlayer { range, probability, only_horizontal: false, mobs: false, only: None } => {
            if !range.is_finite() || *range < 0.0 || !probability.is_finite() || !(0.0..=1.0).contains(probability) || *probability == 0.0 {
                return None;
            }
            // The ECS asks once per `period` ticks: the probability must be one over a whole number.
            let period = (1.0 / f64::from(*probability)).round();
            if (period * f64::from(*probability) - 1.0).abs() > 1e-4 {
                return None;
            }
            (&look_at_player::MODULE, leak(spec, look_at_player::Params { range: f64::from(*range), period: period as i32, species: &[] }))
        }
        GoalSpec::RandomLookAround => (&random_look::MODULE, leak(spec, random_look::Params::DEFAULT)),
        GoalSpec::MeleeAttack { speed, follow_unseen } => {
            if !speed_ok(*speed) {
                return None;
            }
            (&melee::MODULE, leak(spec, melee::Params { speed: *speed, follow_unseen: *follow_unseen, ..melee::Params::DEFAULT }))
        }
        GoalSpec::LeapAtTarget { yd } => {
            if !yd.is_finite() {
                return None;
            }
            (&leap_at_target::MODULE, leak(spec, leap_at_target::Params { yd: *yd }))
        }
        GoalSpec::AvoidEntity { what, max_dist, walk, sprint } => {
            if !max_dist.is_finite() || *max_dist < 0.0 || !speed_ok(*walk) || !speed_ok(*sprint) {
                return None;
            }
            let what = match what {
                MobFilter::Player => Avoid::Player,
                MobFilter::Species(ids) if !ids.is_empty() => Avoid::Named(names(ids)),
                _ => return None,
            };
            let p = avoid_entity::Params { what, max_dist: f64::from(*max_dist), walk: *walk, sprint: *sprint, ..avoid_entity::Params::DEFAULT };
            (&avoid_entity::MODULE, leak(spec, p))
        }
        GoalSpec::Tempt { speed, items, can_scare: false } => {
            if !speed_ok(*speed) || items.is_empty() {
                return None;
            }
            (&tempt::MODULE, leak(spec, tempt::Params { speed: *speed, items: names(items), ..tempt::Params::DEFAULT }))
        }
        GoalSpec::FollowParent { speed } => {
            if !speed_ok(*speed) {
                return None;
            }
            (&follow_parent::MODULE, leak(spec, follow_parent::Params { speed: *speed }))
        }
        GoalSpec::FollowMob { speed, stop, area } => {
            if !speed_ok(*speed) || !stop.is_finite() || *stop < 0.0 || !area.is_finite() || *area < 0.0 {
                return None;
            }
            let p = follow_mob::Params { speed: *speed, stop: f64::from(*stop), area: f64::from(*area), ..follow_mob::Params::DEFAULT };
            (&follow_mob::MODULE, leak(spec, p))
        }
        GoalSpec::RestrictSun => (&restrict_sun::MODULE, leak(spec, restrict_sun::Params)),
        GoalSpec::FleeSun { speed } => {
            if !speed_ok(*speed) {
                return None;
            }
            (&flee_sun::MODULE, leak(spec, flee_sun::Params { speed: *speed }))
        }
        GoalSpec::EatBlock => (&eat_block::MODULE, leak(spec, eat_block::Params::DEFAULT)),
        GoalSpec::BreathAir => (&breath_air::MODULE, leak(spec, breath_air::Params::DEFAULT)),
        GoalSpec::TryFindWater => (&try_find_water::MODULE, leak(spec, try_find_water::Params::DEFAULT)),
        GoalSpec::NearestTarget { prey, must_see, must_reach: false, interval } => {
            if *interval < 0 {
                return None;
            }
            let base = nearest_target::Params { must_see: *must_see, interval: *interval, ..nearest_target::Params::DEFAULT };
            let p = match prey {
                MobFilter::Player => nearest_target::Params { players: true, ..base },
                MobFilter::Species(ids) if !ids.is_empty() => nearest_target::Params { types: names(ids), ..base },
                _ => return None,
            };
            (&nearest_target::MODULE, leak(spec, p))
        }
        GoalSpec::HurtByTarget { alert } => (&hurt_by::MODULE, leak(spec, hurt_by::Params { alert: *alert, ..hurt_by::Params::DEFAULT })),
        _ => return None,
    };
    Some(Entry::new(module, priority, params))
}

impl EcsLevel {
    /// `GoalSelector.addGoal` (the goal or the target selector by the kind of goal); false for a creature the module does not suit,
    /// for numbers vanilla refuses and for a kind with no ECS module.
    pub fn add_goal(&mut self, mw: &MobWorld<'_>, id: u32, priority: i32, goal: &GoalSpec) -> bool {
        let Some(entry) = entry_of(goal, priority) else { return false };
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.add_goal_entry(mw, e, entry))
    }

    /// The same as `add_goal`: the selector follows from the kind of goal.
    pub fn add_target_goal(&mut self, mw: &MobWorld<'_>, id: u32, priority: i32, goal: &GoalSpec) -> bool {
        self.add_goal(mw, id, priority, goal)
    }

    /// `removeAllGoals` of the kind `kind` (its API id) on the target selector (`target`) or the goal selector: how many went.
    pub fn remove_goal(&mut self, id: u32, kind: &str, target: bool) -> Option<usize> {
        let e = self.entity_of(id)?;
        self.ecs_mut()?.remove_goals_where(e, target, &|m| api_id(m) == kind)
    }

    /// `Mob.registerGoals` again.
    pub fn reset_goals(&mut self, mw: &MobWorld<'_>, id: u32) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.reset_goals(mw, e))
    }

    fn goal_entries(&self, id: u32, target: bool) -> Option<Vec<MobGoalEntry>> {
        let list = self.ecs()?.goal_list(self.entity_of(id)?, target)?;
        Some(list.into_iter().map(|(priority, m, running)| MobGoalEntry { priority: i32::from(priority), id: api_id(m), running }).collect())
    }

    /// The goal selector's goals, as `goals` reads them.
    pub fn goals(&self, id: u32) -> Option<Vec<MobGoalEntry>> {
        self.goal_entries(id, false)
    }

    /// The target selector's goals.
    pub fn target_goals(&self, id: u32) -> Option<Vec<MobGoalEntry>> {
        self.goal_entries(id, true)
    }
}
