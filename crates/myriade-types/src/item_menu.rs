use crate::*;


/// Identity and location of a player pressing a mod container button.
pub struct MenuButtonContext {
    pub player: u128,
    pub player_entity_id: i32,
    pub pos: [i32; 3],
    pub id: i32,
}

/// What `AnvilUpdateEvent` asks about once `createResult` made vanilla's
/// result with a left input, and what the mods may change: the `output`,
/// its level `cost` and `material_cost`, the units of `right` used (0 or
/// more than it holds: all of it), each starting at vanilla's.
#[derive(Clone, Debug, PartialEq)]
pub struct AnvilUpdate {
    pub left: ItemStack,
    pub right: Option<ItemStack>,
    /// The rename box's text, `None` when the client sent none.
    pub name: Option<String>,
    /// Vanilla's result for these inputs (`getVanillaResult`), whatever
    /// the mods set.
    pub vanilla: AnvilVanilla,
    pub output: Option<ItemStack>,
    /// Signed level cost, preserved as set by `AnvilUpdateEvent.setXpCost`.
    /// A negative cost prevents taking the result (as does zero), not a level refund.
    pub xp_cost: i32,
    pub material_cost: i32,
}

/// `AnvilUpdateEvent.VanillaResult`: the output, level cost and material
/// cost `createResult` made.
#[derive(Clone, Debug, PartialEq)]
pub struct AnvilVanilla {
    pub output: Option<ItemStack>,
    pub xp_cost: i32,
    pub material_cost: i32,
}

impl AnvilUpdate {
    /// The question about `vanilla`, the result `createResult` made from
    /// these inputs and name: the output, level cost and material cost
    /// start at its own.
    pub fn new(
        left: ItemStack,
        right: Option<ItemStack>,
        name: Option<String>,
        vanilla: &crate::AnvilResult,
    ) -> Self {
        let vanilla = AnvilVanilla {
            output: vanilla.result.clone(),
            xp_cost: vanilla.cost,
            material_cost: i32::try_from(vanilla.repair_item_count_cost).unwrap_or(i32::MAX),
        };
        Self {
            left,
            right,
            name,
            output: vanilla.output.clone(),
            xp_cost: vanilla.xp_cost,
            material_cost: vanilla.material_cost,
            vanilla,
        }
    }

    /// The mods' answer, in place of `result`: a cancel leaves no result at
    /// cost 0; otherwise their output, signed level cost and nonnegative
    /// material cost. A changed output is no longer only a rename.
    pub fn answer(self, cancel: bool, result: &mut crate::AnvilResult) {
        if cancel {
            *result = crate::AnvilResult::default();
            return;
        }
        result.only_renaming &= self.output == self.vanilla.output;
        result.cost = self.xp_cost;
        result.repair_item_count_cost = self.material_cost.max(0) as u32;
        result.result = self.output;
    }
}

/// What `GrindstoneEvent.OnPlaceItem` asks about once the inputs changed,
/// and what the mods may set: an `output` (none until a mod sets one) and,
/// with it, `xp` (-1 for vanilla's experience). `vanilla_output` is the
/// vanilla's result, to read; set `output` to replace it.
#[derive(Clone, Debug, PartialEq)]
pub struct GrindstonePlace {
    pub top: Option<ItemStack>,
    pub bottom: Option<ItemStack>,
    pub vanilla_output: Option<ItemStack>,
    pub output: Option<ItemStack>,
    pub xp: i32,
}

/// A grindstone result taken, as `GrindstoneEvent.OnTakeItem` reports it:
/// the inputs, the experience (-1 for vanilla's, or what OnPlaceItem set)
/// and what the inputs hold afterwards (empty, as vanilla's).
#[derive(Clone, Debug, PartialEq)]
pub struct GrindstoneTake {
    pub top: Option<ItemStack>,
    pub bottom: Option<ItemStack>,
    pub xp: i32,
    pub new_top: Option<ItemStack>,
    pub new_bottom: Option<ItemStack>,
}

/// An anvil craft as `AnvilCraftEvent` reports it: the inputs and the result
/// taken, before the crafting changed anything.
#[derive(Clone, Debug, PartialEq)]
pub struct AnvilCraft {
    pub left: Option<ItemStack>,
    pub right: Option<ItemStack>,
    pub output: ItemStack,
}

/// One of a menu's own (non-player) containers: the key a `menu_slot_rules` ask names its slot's
/// container by (`event.container`, with `event.slot` indexing inside it) and
/// [`MenuSlotContext::slot`] reads by. A plain menu has `Main` only; a mod menu type with a
/// crafting grid keeps its grid slots in `Grid` and its crafted result in `Result` (slot 0);
/// a slot of the player's inventory is `Player`'s.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum MenuContainer {
    /// The menu's own slots (a chest's, a machine's).
    #[default]
    Main,
    /// A crafting grid's slots, row by row.
    Grid,
    /// A crafting grid's result slot.
    Result,
    /// The player's inventory, which an ask can be about but the context does not copy
    /// ([`MenuSlotContext::slot`] reads nothing there).
    Player,
}

/// Who and where a `menu_slot_rules` ask is about: the menu's player, its block entity and
/// its own containers' slots. The game fills it the same way on the server and on the client's
/// prediction, once per click plan; mods read it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MenuSlotContext {
    /// The UUID of the player whose menu it is; None where no player is known.
    pub player: Option<u128>,
    /// The block entity the menu belongs to; None when the menu is not a block's
    /// (opened by a mod with no block, a vehicle's, the player's own inventory).
    pub pos: Option<[i32; 3]>,
    /// The menu's own containers' slots as they stand when the click plans, read only (not
    /// the player's inventory): one entry per [`MenuContainer`] in its order (`Main`, `Grid`,
    /// `Result`), each in that container's slot order. Read one with [`Self::slot`].
    pub slots: Vec<Vec<Option<ItemStack>>>,
}

impl MenuSlotContext {
    /// A context of `player`, the block entity at `pos` and the containers' `slots`, one entry
    /// per [`MenuContainer`] in its order.
    pub fn new(
        player: Option<u128>,
        pos: Option<[i32; 3]>,
        slots: Vec<Vec<Option<ItemStack>>>,
    ) -> Self {
        Self { player, pos, slots }
    }
    /// What slot `index` of `container` holds, keyed as the ask names its own slot; None when
    /// it is empty or the menu has no such slot.
    pub fn slot(&self, container: MenuContainer, index: usize) -> Option<&ItemStack> {
        self.slots.get(container as usize)?.get(index)?.as_ref()
    }
}
