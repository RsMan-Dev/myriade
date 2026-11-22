//! The carts on the server's side of the bridge (`cart_more.rs`): their contents, the cart a ray enters first and the carts in a box, by the
//! network ids already bound. Nothing here allocates an id. An inert level answers as an empty world.
use super::level::EcsLevel;
use horizon_client_lib::command_block::CommandBlock;
use horizon_client_lib::items::ItemStack;
use horizon_client_lib::text::Text;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::minecart::{Cart, CartHurt};

impl EcsLevel {
    /// A copy of the command block state of the cart `id` as the host reads it; None when it is not a command cart or the level is inert.
    /// `powered`, `auto` and `condition_met` have no component: they read as false.
    pub fn cart_command(&self, id: u32) -> Option<CommandBlock> {
        let e = self.entity_of(id)?;
        let c = self.ecs()?.cart_command(e)?;
        Some(CommandBlock {
            command: c.command,
            success_count: c.success,
            custom_name: c.custom_name,
            track_output: c.track,
            last_output: (!c.last_output.is_empty()).then(|| Text::literal(c.last_output)),
            update_last_execution: c.update_last_execution,
            last_execution: c.last_execution.unwrap_or(-1),
            ..Default::default()
        })
    }
    /// Writes the host's command block state `state` into the cart `id`; false when it is not a command cart or the level is inert.
    /// The host's `powered`, `auto` and `condition_met` have no component and are not kept; `last_activated` is kept.
    pub fn set_cart_command(&mut self, id: u32, state: &CommandBlock) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        let Some(w) = self.ecs_mut() else { return false };
        let Some(mut c) = w.cart_command(e) else { return false };
        c.command = state.command.clone();
        c.success = state.success_count;
        c.custom_name = state.custom_name.clone();
        c.track = state.track_output;
        c.last_output = state.last_output.as_ref().map_or_else(String::new, Text::plain);
        c.update_last_execution = state.update_last_execution;
        c.last_execution = (state.last_execution != -1).then_some(state.last_execution);
        w.set_cart_command(e, c)
    }
    /// The damage the cart `id` last took (`CartHurt`, 0 before any blow); None when it is not a cart or the level is inert.
    /// The last output the command block of cart `id` synced (`last_output`, raw text); None when it is not a command cart.
    pub fn cart_last_output(&self, id: u32) -> Option<String> {
        let e = self.entity_of(id)?;
        self.ecs()?.cart_command(e).map(|c| c.last_output)
    }
    pub fn cart_damage(&self, id: u32) -> Option<f32> {
        let e = self.entity_of(id)?;
        let w = self.ecs()?.world();
        if w.get::<Cart>(e).is_none() {
            return None;
        }
        Some(w.get::<CartHurt>(e).map_or(0.0, |h| h.damage))
    }
    /// The flipped rotation of the cart `id`; None when it has no such state or the level is inert.
    pub fn cart_flipped(&self, id: u32) -> Option<bool> {
        let e = self.entity_of(id)?;
        self.ecs()?.cart_flipped(e)
    }
    /// A copy of the slots of the contents of the cart `id`; None without contents or when inert.
    pub fn cart_items(&self, id: u32) -> Option<Vec<Option<ItemStack>>> {
        let e = self.entity_of(id)?;
        Some(self.ecs()?.cart_items(e)?.to_vec())
    }
    /// Sets slot `slot` of the contents of the cart `id`; false when there is no such slot or the level is inert.
    pub fn cart_set_slot(&mut self, id: u32, slot: usize, stack: Option<ItemStack>) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.cart_set_slot(e, slot, stack))
    }
    /// The loot table and seed of the contents of the cart `id`, once; None when nothing is pending or the level is inert.
    pub fn cart_take_loot(&mut self, id: u32) -> Option<(String, i64)> {
        let e = self.entity_of(id)?;
        self.ecs_mut()?.cart_take_loot(e)
    }
    /// The id of the cart the ray from `eye` along `dir` enters first within `reach`, with the distance. None when inert, when no cart is
    /// hit, or when that cart has no id yet.
    pub fn aimed_cart(&self, eye: Vec3, dir: Vec3, reach: f64) -> Option<(u32, f64)> {
        self.aimed_cart_except(eye, dir, reach, None)
    }
    /// `aimed_cart` with the cart `skip` (the one the aimer rides) left out; an id that is unknown skips nothing.
    pub fn aimed_cart_except(&self, eye: Vec3, dir: Vec3, reach: f64, skip: Option<u32>) -> Option<(u32, f64)> {
        let ecs = self.ecs()?;
        let skip = skip.and_then(|id| self.entity_of(id));
        let (e, t) = ecs.aimed_cart(eye, dir, reach, skip)?;
        Some((ecs.id_of(e)?, t))
    }
    /// The ids of the carts whose position lies in `min..=max`; a cart without an id is left out. Empty when inert.
    pub fn carts_in(&self, min: Vec3, max: Vec3) -> Vec<u32> {
        self.ecs().map_or_else(Vec::new, |ecs| ecs.carts_in(min, max).into_iter().filter_map(|e| ecs.id_of(e)).collect())
    }
}
