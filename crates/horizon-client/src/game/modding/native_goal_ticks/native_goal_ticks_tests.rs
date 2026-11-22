//! Tests for `native_goal_ticks` (arismoko/horizon#1077): registration order,
//! priorities and interruption, per-mob state and random streams, the read
//! view, `set_target`, and the no-mod-goals fast path.
//!
//! The `zombie` fixture is shared with the behavior tests (arismoko/horizon#824).
use super::super::native_entity_refs::tests::with_game;
use horizon_client::mobs::Mob;
use horizon_core::geometry::Vec3;
use horizon_core::random::RandomSource as _;
use mod_api::{
    EntityPart, EntityRef, Goal, GoalContext, GoalFlags, GoalRegistrar, Hooks, LivingRef, ModEntry,
    Registrar, RegistrationContext,
};
use std::sync::Mutex;

/// What one test goal did, by mob id.
static EVENTS: Mutex<Vec<(u32, &'static str)>> = Mutex::new(Vec::new());
/// The other mob a tick reads through the view, as a handle.
static OTHER: Mutex<Option<EntityRef>> = Mutex::new(None);
/// What the last tick read of `OTHER`: its health, position and a refused
/// read (a potion check, which the snapshot does not carry).
static READS: Mutex<Vec<(Option<f32>, Option<[f64; 3]>, bool)>> = Mutex::new(Vec::new());

fn record(id: u32, call: &'static str) {
    EVENTS.lock().unwrap().push((id, call));
}

/// The mod goal of every test: counts its calls in its own state and in the
/// shared journal, draws from the mob's own random at start, and reads the
/// other mob through the tick's view on tick.
#[derive(Default)]
struct Counter {
    ticks: u32,
    draw: i32,
}
impl Goal for Counter {
    fn flags(&self) -> GoalFlags {
        GoalFlags::MOVE
    }
    fn name(&self) -> &'static str {
        "Counter"
    }
    fn can_use(&mut self, cx: &mut GoalContext<'_>) -> bool {
        record(cx.mob().entity().id(), "can_use");
        true
    }
    fn start(&mut self, cx: &mut GoalContext<'_>) {
        record(cx.mob().entity().id(), "start");
        self.draw = cx.random().next_int_bound(1000).unwrap_or(0);
    }
    fn tick(&mut self, cx: &mut GoalContext<'_>) {
        self.ticks += 1;
        record(cx.mob().entity().id(), "tick");
        if let Some(other) = OTHER.lock().unwrap().as_ref().copied() {
            let health = LivingRef::new(other).health(cx.host());
            let position = other.position(cx.host());
            // A read the view refuses: the snapshot carries no rotation of
            // another mob (only the ticking mob's own answers).
            let refused = other.rotation(cx.host()).is_some();
            READS.lock().unwrap().push((health, position, refused));
        }
    }
    fn stop(&mut self, cx: &mut GoalContext<'_>) {
        record(cx.mob().entity().id(), "stop");
    }
}

struct ZombieGoals;
#[mod_api::horizon_part]
impl EntityPart for ZombieGoals {
    fn register_goals(&self, goals: &mut GoalRegistrar<'_>) {
        assert_eq!(goals.mob().id(), "minecraft:zombie");
        goals.add_goal(7, Counter::default());
    }
}

/// Registers the zombie's mod goal and listens to `finalize_spawn` without
/// changing anything.
struct GoalsAndFinalize;
#[mod_api::__subscriptions]
impl Hooks for GoalsAndFinalize {
    fn register(&self, r: &mut dyn Registrar, _cx: &RegistrationContext) {
        r.entity_part("minecraft:zombie", ZombieGoals);
    }
    fn finalize_spawn(
        &self,
        _: &dyn mod_api::Host,
        _: &mut mod_api::FinalizeSpawnEvent<'_>,
    ) -> mod_api::Flow {
        mod_api::Flow::Continue
    }
}

/// A `finalize_spawn` listener lends the mob for its call: the mod goal
/// attached when the mob was made (`part_goals::attach`, whose copies carry
/// no goal and name themselves `ModGoal`) is still the mob's own afterwards.
#[test]
#[ignore = "daily"]
fn a_finalize_spawn_listener_keeps_the_mod_goals() {
    // The mods install before the game opens: a game that opens resolves
    // the parts of whatever is installed, and the worker block metadata
    // with them (installing inside the game leaves it unpublished).
    let entries = vec![ModEntry::new("finalize-goals", Box::new(GoalsAndFinalize))];
    mod_platform::__private::install(entries.into());
    crate::game::api_values::install_parts(&[ModEntry::new(
        "finalize-goals",
        Box::new(GoalsAndFinalize),
    )])
    .unwrap();
    with_game(|game| {
        game.resolve_parts().unwrap();
        let kind = game
            .res
            .mob_types()
            .get("minecraft:zombie")
            .unwrap()
            .clone();
        let mut m = Mob::new(60, kind, Vec3::new(3.5, 65.0, 3.5), 0.0);
        m.on_ground = true;
        let reason = horizon_client::mobs::spawn_rules::Reason::Natural;
        game.finalize_with(&mut m, reason, None);
        assert!(
            m.ai.goals.entries().any(|(_, name, _)| name == "Counter"),
            "the mod goal survives the lend"
        );
    });
}
