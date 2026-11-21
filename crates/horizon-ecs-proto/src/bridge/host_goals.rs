//! The host's edits of a creature's goal lists (`goal_edit.rs`): add, remove by module, reset, read.
use crate::behaviours::{Entry, Module};
use crate::bridge::host::EcsWorld;
use crate::goal_edit;
use bevy_ecs::entity::Entity;

impl EcsWorld {
    /// `GoalSelector.addGoal` with a ready entry; false when the entity cannot hold it.
    pub fn add_goal_entry(&mut self, e: Entity, entry: Entry) -> bool {
        goal_edit::add(self.world_mut(), e, entry)
    }

    /// `removeAllGoals` on the goal selector (`target` false) or the target selector, for the modules `pred` accepts.
    pub fn remove_goals_where(&mut self, e: Entity, target: bool, pred: &dyn Fn(&'static Module) -> bool) -> Option<usize> {
        goal_edit::remove_where(self.world_mut(), e, target, pred)
    }

    /// `Mob.registerGoals` again.
    pub fn reset_goals(&mut self, e: Entity) -> bool {
        goal_edit::reset(self.world_mut(), e)
    }

    /// The goals of a selector in registration order: priority, module, running.
    pub fn goal_list(&self, e: Entity, target: bool) -> Option<Vec<(u8, &'static Module, bool)>> {
        goal_edit::list(self.world(), e, target)
    }
}
