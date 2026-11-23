//! The edits made to a spawn before its entity exists (the listeners of `finalize_spawn`, the spawner's rolls), kept as data: the pack's
//! group, the persistence, and the sparse edits of gear, attributes, tags and name. An absent field is no edit: nothing is written to the
//! entity for it. Applied by `EcsWorld::spawn_prepared`, then finalized by `finalize_prepared`. Nothing here names a species.
use crate::attr_mod::Modifier;
use crate::spawn_finalize::Group;
use horizon_client_lib::items::ItemStack;

/// One attribute edit, in the order it was made.
#[derive(Clone, Debug, PartialEq)]
pub enum AttrEdit {
    /// `setBaseValue`.
    Base(f64),
    /// `setBaseValue` of the species' own base: the written base is dropped.
    ResetBase,
    /// `addPermanentModifier` or `addTransientModifier`, by the modifier's own lifetime.
    Add(Modifier),
    /// `removeModifier`, by the modifier's id.
    Remove(String),
}

/// The sheet of one spawn: every field is empty or None by default, which means no edit.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SpawnSheet {
    /// The pack's group, read by finalization and taken by the next member of the pack.
    pub group: Option<Group>,
    /// `setPersistenceRequired`, as the trial spawner keeps its mobs.
    pub persistent: bool,
    /// The stack put in each slot (its index in `EquipmentSlot` order); None empties it. A later edit of a slot replaces the earlier one.
    pub equipment: Vec<(usize, Option<ItemStack>)>,
    /// The attribute edits (name, edit), in order.
    pub attributes: Vec<(String, AttrEdit)>,
    /// The tags added, in order.
    pub tags: Vec<String>,
    /// The custom name's text (JSON), None when no name is set.
    pub name: Option<String>,
    /// The passenger attachments of a vehicle (one per seat), replacing its species' ones when not empty.
    pub passenger_attachments: Vec<[f32; 3]>,
}

impl SpawnSheet {
    /// Sets the stack of `slot`, replacing the edit it had.
    pub fn set_equipment(&mut self, slot: usize, stack: Option<ItemStack>) {
        self.equipment.retain(|(s, _)| *s != slot);
        self.equipment.push((slot, stack));
    }

    /// Records an attribute edit on `name`.
    pub fn attribute(&mut self, name: &str, edit: AttrEdit) {
        self.attributes.push((name.to_owned(), edit));
    }
}

#[cfg(test)]
mod tests {
    use super::SpawnSheet;
    use crate::bridge::host::{EcsWorld, HostSetup};
    use crate::mount::PassengerSeats;

    /// `SpawnSheet.passenger_attachments` through `apply_sheet`: the entity gets the seats of the birth, in order, and no baby seat.
    #[test]
    fn a_sheet_with_passenger_attachments_gives_the_seats_of_the_birth() {
        let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
        let setup = HostSetup { seed: 1, dimension: "minecraft:overworld", simulation_radius: 0 };
        let (mw, types, _) = horizon_client_lib::mobs::world::leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
        let mut h = EcsWorld::new(&setup, mw, types, &tags);
        let e = h.world_mut().spawn_empty().id();
        let sheet = SpawnSheet { passenger_attachments: vec![[0.0, 1.0, 0.5], [0.0, 1.0, -0.5]], ..Default::default() };
        h.apply_sheet(e, &sheet);
        let seats = h.world_mut().get::<PassengerSeats>(e).unwrap();
        assert_eq!(seats.adult.as_ref(), &[[0.0, 1.0, 0.5], [0.0, 1.0, -0.5]]);
        assert!(seats.baby.is_none());
    }
}
