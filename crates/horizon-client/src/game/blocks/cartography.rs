//! The cartography table (`CartographyTableMenu` on the menu engine,
//! `CartographyTableScreen`): zooming a map out with paper, cloning it with
//! an empty map, locking it with a glass pane. The engine reads the level's
//! maps through `LevelMaps`; the scaled and locked copies a take makes
//! (`MapItem.scaleMap`, `lockMap`) are registered after the click. The
//! screen draws the preview with its error and lock sprites.
use super::{
    Game,
    maps::{MapData, map_id},
};
use horizon_client::{items::ItemStack, menu::Maps};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
};

/// The level's maps as a menu sees them (`ContainerLevelAccess`): the
/// states of the maps the click can reach, and the copies it makes.
#[derive(Clone)]
pub(super) struct LevelMaps {
    states: RefCell<HashMap<i32, (bool, u8)>>,
    next_id: Cell<i32>,
    /// (new id, source id, scaled) for each copy made.
    made: RefCell<Vec<(i32, i32, bool)>>,
}

impl Maps for LevelMaps {
    fn with_staged(&self, edit: &mut dyn FnMut(&dyn Maps) -> bool) -> bool {
        let staged = self.clone();
        if !edit(&staged) {
            return false;
        }
        self.states.replace(staged.states.into_inner());
        self.next_id.set(staged.next_id.get());
        self.made.replace(staged.made.into_inner());
        true
    }
    fn state(&self, id: i32) -> Option<(bool, u8)> {
        self.states.borrow().get(&id).copied()
    }

    fn derive(&self, id: i32, scale: bool) -> Option<i32> {
        let (_, s) = self.state(id)?;
        let new = self.next_id.get();
        self.next_id.set(new + 1);
        let made = if scale {
            (false, (s + 1).min(4))
        } else {
            (true, s)
        };
        self.states.borrow_mut().insert(new, made);
        self.made.borrow_mut().push((new, id, scale));
        Some(new)
    }
}

impl Game<'_> {
    /// The maps of `stacks` (loaded as needed) for a menu over this level.
    pub(super) fn level_maps<'s>(
        &mut self,
        stacks: impl IntoIterator<Item = &'s ItemStack>,
    ) -> LevelMaps {
        let ids: Vec<i32> = stacks.into_iter().filter_map(map_id).collect();
        let mut states = HashMap::new();
        for id in ids {
            if let Some(m) = self.map(id) {
                states.insert(id, (m.locked, m.scale));
            }
        }
        LevelMaps {
            states: RefCell::new(states),
            next_id: Cell::new(self.peek_map_id()),
            made: RefCell::new(Vec::new()),
        }
    }

    /// The scaled and locked copies a menu made (`level.setMapData`).
    pub(super) fn register_made_maps(&mut self, maps: LevelMaps) {
        for (id, source, scale) in maps.made.into_inner() {
            let Some(data) = self
                .map(source)
                .map(|m| if scale { m.scaled() } else { m.locked_copy() })
            else {
                continue;
            };
            let new = self.new_map_id(data);
            debug_assert_eq!(new, id, "map ids follow peek_map_id");
        }
    }
}
impl Game<'_> {
}

impl MapData {
    /// `MapItemSavedData.scaled`: a fresh map one scale up around the same centre.
    pub(super) fn scaled(&self) -> MapData {
        let mut m = MapData::fresh(
            f64::from(self.center[0]),
            f64::from(self.center[1]),
            (self.scale + 1).min(4),
            &self.dimension,
        );
        m.tracking = self.tracking;
        m.unlimited = self.unlimited;
        m
    }
    /// `MapItemSavedData.locked`: the same pixels, banner markers and
    /// decorations, locked (the frame markers are not copied).
    pub(super) fn locked_copy(&self) -> MapData {
        let mut m = MapData::fresh(
            f64::from(self.center[0]),
            f64::from(self.center[1]),
            self.scale,
            &self.dimension,
        );
        m.center = self.center;
        m.colors = self.colors.clone();
        m.banners = self.banners.clone();
        m.decorations = self.decorations.clone();
        m.tracking = self.tracking;
        m.unlimited = self.unlimited;
        m.locked = true;
        m
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn staged_maps_restore_id_reservations_and_pending_copies_on_refusal() {
        for accepted in [false, true] {
            let maps = LevelMaps {
                states: RefCell::new(HashMap::from([(7, (false, 0))])),
                next_id: Cell::new(42),
                made: RefCell::new(Vec::new()),
            };
            assert_eq!(
                maps.with_staged(&mut |staged| {
                    assert_eq!(staged.derive(7, true), Some(42));
                    assert_eq!(staged.state(42), Some((false, 1)));
                    accepted
                }),
                accepted
            );
            assert_eq!(maps.next_id.get(), if accepted { 43 } else { 42 });
            assert_eq!(maps.state(42), accepted.then_some((false, 1)));
            assert_eq!(
                maps.made.borrow().as_slice(),
                if accepted { &[(42, 7, true)][..] } else { &[] }
            );
        }
    }

    #[test]
    #[ignore = "daily"]
    fn crafting_map_takes_use_real_saved_maps() {
        use horizon_client::menu::{Env, Input, Kind, Menu};
        super::super::native_entity_refs::tests::with_game(|game| {
            for mode in [Input::Swap, Input::Pickup, Input::QuickMove] {
                let source = game.new_map_id(MapData::fresh(4.5, 4.5, 0, "minecraft:overworld"));
                let mut input = ItemStack::new("minecraft:filled_map", 1);
                input.data_mut().map_id = Some(source);
                let maps = game.level_maps([&input]);
                let env = Env {
                    maps: Some(&maps),
                    ..game.menu_env()
                };
                let mut menu = Menu::new(Kind::Crafting);
                for i in [1, 2, 3, 4, 6, 7, 8, 9] {
                    menu.set_slot(&env, i, Some(ItemStack::new("minecraft:paper", 1)));
                }
                menu.set_slot(&env, 5, Some(input));
                let preview = menu.slot(0).expect("real scaling recipe admitted");
                assert_eq!(preview.data().and_then(|d| d.map_id), Some(source));
                assert_eq!(preview.data().and_then(|d| d.map_post_processing), Some(1));
                menu.click(&env, 0, if mode == Input::Swap { 2 } else { 0 }, mode)
                    .unwrap();
                let output = match mode {
                    Input::Swap => menu.player_item(2),
                    Input::Pickup => menu.carried.as_ref(),
                    _ => menu.player_item(8),
                }
                .expect("crafted map remains present");
                assert_eq!(output.count, 1);
                assert_eq!(output.data().and_then(|d| d.map_id), Some(source + 1));
                assert_eq!(output.data().and_then(|d| d.map_post_processing), None);
                assert!((1..10).all(|i| menu.slot(i).is_none()));
                game.register_made_maps(maps);
                let original = game.map(source).unwrap();
                assert_eq!((original.scale, original.center), (0, [0, 0]));
                let derived = game.map(source + 1).unwrap();
                assert_eq!((derived.scale, derived.center), (1, [64, 64]));
            }
        });
    }
}
