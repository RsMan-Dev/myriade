//! Retained copies feed the same per-view submission route as transient batches.
use mod_api::{GroupError, InstanceGroupRef, ModelInstance, ModelRef};

use glam::Mat4;

const MOD_CAPACITY: usize = 65_536;

struct Group {
    generation: u64,
    owner: Option<&'static str>,
    model: ModelRef,
    copies: Vec<ModelInstance>,
    len: usize,
    transform: Mat4,
    composed: Vec<ModelInstance>,
}

#[derive(Default)]
pub(super) struct InstanceGroupStorage {
    next: u64,
    groups: Vec<Option<Group>>,
}
impl InstanceGroupStorage {
    pub(super) fn clear(&mut self) {
        self.groups.clear();
    }
    pub(super) fn retire_disabled(&mut self, disabled: &std::collections::BTreeSet<String>) {
        for slot in &mut self.groups {
            if slot
                .as_ref()
                .is_some_and(|g| g.owner.is_some_and(|id| disabled.contains(id)))
            {
                *slot = None;
            }
        }
    }
    fn get(
        &mut self,
        _handle: InstanceGroupRef,
        _model_generation: u64,
    ) -> Result<&mut Group, GroupError> {
        Err(GroupError::Stale)
    }
}
