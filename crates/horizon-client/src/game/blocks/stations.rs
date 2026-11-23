//! Workstation menus in the game: enchanting table, anvil, grindstone,
//! smithing table, stonecutter and the brewing stand screen. Layouts, sprites,
//! colours and sounds follow the vanilla screens (`EnchantmentScreen`,
//! `AnvilScreen`, `GrindstoneScreen`, `SmithingScreen`, `StonecutterScreen`,
//! `BrewingStandScreen`); item rules live in `horizon_client::stations`.

#[path = "stations/anvil.rs"]
mod anvil;
use super::Game;
use horizon_client::{
    brewing,
    containers::BlockEntity,
    items::ItemStack,
    random_source,
    stations::{self, AnvilResult, EnchantOffers},
};
use horizon_core::random::{LegacyRandom, RandomSource};

/// The book above the enchanting table's offers: `EnchantmentScreen`'s
/// `time`, `flip`, `oFlip`, `flipT`, `flipA`, `open`, `oOpen` and `last`.
pub(super) struct EnchantBook {
    time: i32,
    flip: f32,
    o_flip: f32,
    flip_t: f32,
    flip_a: f32,
    open: f32,
    o_open: f32,
    last: Option<ItemStack>,
    random: LegacyRandom,
}
impl EnchantBook {
    fn new(random: LegacyRandom) -> Self {
        Self {
            time: 0,
            flip: 0.0,
            o_flip: 0.0,
            flip_t: 0.0,
            flip_a: 0.0,
            open: 0.0,
            o_open: 0.0,
            last: None,
            random,
        }
    }
    /// `EnchantmentScreen.tickBook`: a new item in the enchanting slot picks a
    /// new page to turn to; the book opens while any offer has a cost.
    pub(super) fn tick(&mut self, input: Option<&ItemStack>, costs: [i32; 3]) {
        if input != self.last.as_ref() {
            self.last = input.cloned();
            loop {
                let a = self.random.next_int_bound(4).unwrap_or(0);
                let b = self.random.next_int_bound(4).unwrap_or(0);
                self.flip_t += (a - b) as f32;
                if !(self.flip <= self.flip_t + 1.0 && self.flip >= self.flip_t - 1.0) {
                    break;
                }
            }
        }
        self.time += 1;
        self.o_flip = self.flip;
        self.o_open = self.open;
        let open = costs.iter().any(|&c| c != 0);
        self.open = (self.open + if open { 0.2 } else { -0.2 }).clamp(0.0, 1.0);
        let diff = ((self.flip_t - self.flip) * 0.4).clamp(-0.2, 0.2);
        self.flip_a += (diff - self.flip_a) * 0.9;
        self.flip += self.flip_a;
    }
}

/// Slot ids of the open station: inputs from 600, the result at 650.
pub(super) const INPUT: usize = 600;
pub(super) const RESULT: usize = 650;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Kind {
    Enchanting,
    Anvil,
    Grindstone,
    Smithing,
    Stonecutter,
    /// Villager trading (game/items/trading.rs).
    Merchant,
    /// The beacon menu (game/blocks/beacon.rs).
    Beacon,
    /// The cartography table (game/blocks/cartography.rs).
    Cartography,
    /// The loom (game/blocks/loom.rs).
    Loom,
}
impl Kind {
    pub(super) fn from_block(name: &str) -> Option<Self> {
        Some(match name {
            "minecraft:enchanting_table" => Self::Enchanting,
            "minecraft:anvil" | "minecraft:chipped_anvil" | "minecraft:damaged_anvil" => {
                Self::Anvil
            }
            "minecraft:grindstone" => Self::Grindstone,
            "minecraft:smithing_table" => Self::Smithing,
            "minecraft:stonecutter" => Self::Stonecutter,
            "minecraft:beacon" => Self::Beacon,
            "minecraft:cartography_table" => Self::Cartography,
            "minecraft:loom" => Self::Loom,
            _ => return None,
        })
    }
    pub(super) fn inputs(self) -> usize {
        match self {
            Self::Smithing | Self::Loom => 3,
            Self::Stonecutter | Self::Beacon => 1,
            _ => 2,
        }
    }
    pub(super) fn title_key(self) -> &'static str {
        match self {
            Self::Enchanting => "container.enchant",
            Self::Anvil => "container.repair",
            Self::Grindstone => "container.grindstone_title",
            Self::Smithing => "container.upgrade",
            Self::Stonecutter => "container.stonecutter",
            Self::Merchant => "merchant.trades",
            Self::Beacon => "container.beacon",
            Self::Cartography => "container.cartography_table",
            Self::Loom => "container.loom",
        }
    }
    fn input_pos(self, i: usize) -> (f32, f32) {
        let table: &[(f32, f32)] = match self {
            Self::Enchanting => &[(15.0, 47.0), (35.0, 47.0)],
            Self::Anvil => &[(27.0, 47.0), (76.0, 47.0)],
            Self::Grindstone => &[(49.0, 19.0), (49.0, 40.0)],
            Self::Smithing => &[(8.0, 48.0), (26.0, 48.0), (44.0, 48.0)],
            Self::Stonecutter => &[(20.0, 33.0)],
            Self::Merchant => &[(86.0, 37.0), (112.0, 37.0)],
            Self::Beacon => &[(109.0, 110.0)],
            Self::Cartography => &[(15.0, 15.0), (15.0, 52.0)],
            Self::Loom => &[(13.0, 26.0), (33.0, 26.0), (23.0, 45.0)],
        };
        table[i.min(table.len() - 1)]
    }
    fn result_pos(self) -> Option<(f32, f32)> {
        match self {
            Self::Enchanting => None,
            Self::Anvil => Some((134.0, 47.0)),
            Self::Grindstone => Some((129.0, 34.0)),
            Self::Smithing => Some((98.0, 48.0)),
            Self::Stonecutter => Some((143.0, 33.0)),
            Self::Merchant => Some((170.0, 37.0)),
            Self::Beacon => None,
            Self::Cartography => Some((145.0, 39.0)),
            Self::Loom => Some((143.0, 57.0)),
        }
    }
}

pub(super) struct Station {
    pub(super) kind: Kind,
    pub(super) pos: [i32; 3],
    pub(super) inputs: Vec<Option<ItemStack>>,
    pub(super) result: Option<ItemStack>,
    pub(super) offers: EnchantOffers,
    bookcases: i32,
    pub(super) anvil: AnvilResult,
    /// Anvil rename box text (`AnvilMenu.itemName`).
    pub(super) name: String,
    /// Stonecutter: the menu's recipe list (horizon_client::menu), its
    /// `selectedRecipeIndex` and the screen's first shown index.
    pub(super) stone: horizon_client::menu::Stonecutter,
    pub(super) selected: Option<usize>,
    start: usize,
    /// Stonecutter: the screen's `scrollOffs` (0 to 1).
    scroll: f32,
    /// Anvil: the input the rename box was last filled from.
    anvil_last: Option<ItemStack>,
    /// Anvil on a server: the inputs and name the client's
    /// `AnvilUpdateEvent` was last asked about, and the result it left.
    anvil_answer: Option<(Vec<Option<ItemStack>>, String, AnvilResult)>,
    pub(super) last_sound: u64,
    /// Villager trading state.
    pub(super) merchant: Option<Box<super::trading::MerchantState>>,
    /// Beacon menu: the selected primary and secondary power (EFFECTS index).
    pub(super) beacon: [Option<usize>; 2],
    /// The loom's patterns, selection and scroller (game/blocks/loom.rs).
    pub(super) loom: super::loom::LoomState,
    /// The enchanting table's book (`EnchantmentScreen`'s flip and open state).
    pub(super) book: EnchantBook,
}
impl Station {
    fn new(kind: Kind, pos: [i32; 3]) -> Self {
        Self {
            kind,
            pos,
            inputs: vec![None; kind.inputs()],
            result: None,
            offers: EnchantOffers::default(),
            bookcases: 0,
            anvil: AnvilResult::default(),
            name: String::new(),
            stone: Default::default(),
            selected: None,
            start: 0,
            scroll: 0.0,
            anvil_last: None,
            anvil_answer: None,
            last_sound: 0,
            merchant: None,
            beacon: [None; 2],
            loom: Default::default(),
            book: EnchantBook::new(random_source::create()),
        }
    }
    pub(super) fn new_kind(kind: Kind, pos: [i32; 3]) -> Self {
        Self::new(kind, pos)
    }
    /// `StonecutterScreen.containerChanged`: a changed input puts the list back at the top.
    pub(super) fn stonecutter_changed(&mut self) {
        self.start = 0;
        self.scroll = 0.0;
    }
}

impl Game<'_> {
    /// `EnchantmentScreen.containerTick`'s `tickBook` for an open enchanting table.
    pub(super) fn tick_enchanting_book(&mut self) {
        if let Some(s) = self.station.as_mut().filter(|s| s.kind == Kind::Enchanting) {
            let costs = s.offers.costs;
            s.book.tick(s.inputs[0].as_ref(), costs);
        }
    }
    /// `Component.translatable`: the selected language (which `lang` layers
    /// over `en_us`), the caller's English text last.
    pub(super) fn translate_with_fallback(&self, key: &str, fallback: &str) -> String {
        horizon_client::lang::get_or(key, fallback)
    }
    fn brewing_open(&self) -> bool {
        matches!(self.container_kind(), Some(BlockEntity::Brewing(_)))
    }
    /// Opens a workstation menu for the block; false when it is not one.
    pub(super) fn open_station(&mut self, pos: [i32; 3], name: &str) -> bool {
        let Some(kind) = Kind::from_block(name) else {
            return false;
        };
        if self.level.xp_seed == 0 {
            self.level.xp_seed = self.world.random.next_int();
        }
        self.station = Some(Station::new(kind, pos));
        self.station_changed();
        self.beacon_opened();
        // The block's interaction statistic.
        let stat = match name {
            "minecraft:crafting_table" => "interact_with_crafting_table",
            "minecraft:furnace" => "interact_with_furnace",
            "minecraft:blast_furnace" => "interact_with_blast_furnace",
            "minecraft:smoker" => "interact_with_smoker",
            "minecraft:brewing_stand" => "interact_with_brewingstand",
            "minecraft:beacon" => "interact_with_beacon",
            "minecraft:grindstone" => "interact_with_grindstone",
            "minecraft:smithing_table" => "interact_with_smithing_table",
            "minecraft:stonecutter" => "interact_with_stonecutter",
            "minecraft:loom" => "interact_with_loom",
            "minecraft:cartography_table" => "interact_with_cartography_table",
            n if n.ends_with("anvil") => "interact_with_anvil",
            _ => "",
        };
        if !stat.is_empty() {
            self.award_custom_stat(stat, 1);
        }
        true
    }
    /// `AbstractContainerMenu.clearContainer`: inputs go back to the player.
    pub(super) fn close_station(&mut self) {
        if self
            .station
            .as_ref()
            .is_some_and(|s| s.kind == Kind::Merchant)
        {
            // MerchantMenu.removed: the carried stack and the payment go back.
            self.menu_session(|menu, env, _| {
                menu.merchant_close(env);
                Some(true)
            });
        }
        let Some(mut station) = self.station.take() else {
            return;
        };
        if false && station.kind != Kind::Merchant {
            // A server's workstation: its removed() gives the inputs back.
            return;
        }
        if let Some(m) = station.merchant.as_mut() {
            self.merchant_closed(m);
        }
        for stack in station.inputs.into_iter().flatten() {
            self.give_or_drop(stack);
        }
    }
    pub(super) fn station_slots(&self) -> Option<Vec<usize>> {
        let mut v: Vec<usize> = (0..36).collect();
        if self.brewing_open() {
            v.extend(300..305);
            return Some(v);
        }
        let s = self.station.as_ref()?;
        v.extend((0..s.inputs.len()).map(|i| INPUT + i));
        if s.kind.result_pos().is_some() {
            v.push(RESULT);
        }
        Some(v)
    }
    pub(super) fn station_slot_position(&self, i: usize) -> Option<(f32, f32)> {
        if self.brewing_open() && (300..305).contains(&i) {
            return Some(
                [
                    (56.0, 51.0),
                    (79.0, 58.0),
                    (102.0, 51.0),
                    (79.0, 17.0),
                    (17.0, 17.0),
                ][i - 300],
            );
        }
        let s = self.station.as_ref()?;
        if s.kind == Kind::Merchant && i < 36 {
            return Some(super::trading::inventory_slot(i));
        }
        if s.kind == Kind::Beacon {
            return super::beacon::slot_position(i);
        }
        match i {
            RESULT => s.kind.result_pos(),
            _ if i >= INPUT && i < INPUT + s.inputs.len() => Some(s.kind.input_pos(i - INPUT)),
            _ => None,
        }
    }
    pub(super) fn station_slot_mut(&mut self, i: usize) -> &mut Option<ItemStack> {
        match self.station.as_mut() {
            Some(s) if i == RESULT => &mut s.result,
            Some(s) if i >= INPUT && i < INPUT + s.inputs.len() => &mut s.inputs[i - INPUT],
            _ => &mut self.scratch_slot,
        }
    }
    /// The anvil name the menu works with (`AnvilScreen.onNameChanged`).
    pub(super) fn anvil_item_name(&self, s: &Station) -> String {
        match &s.inputs[0] {
            Some(input)
                if input.custom_name().is_none()
                    && s.name == self.pack.names.display_item(&input.item) =>
            {
                String::new()
            }
            Some(_) => s.name.clone(),
            None => String::new(),
        }
    }
    /// Apply the client anvil hook once for each input and name combination.
    fn update_anvil_answer(&mut self, s: &mut Station, name: String) {
        if !false {
            return;
        }
        let Some(left) = s.inputs[0].clone() else {
            return;
        };
        match &s.anvil_answer {
            Some((inputs, asked, answer)) if *inputs == s.inputs && *asked == name => {
                s.anvil = answer.clone();
            }
            _ => {
                let mut update = crate::menu::AnvilUpdate::new(
                    left,
                    s.inputs[1].clone(),
                    Some(name.clone()),
                    &s.anvil,
                );
                let mut cancel = false;
                let (player, entity_id) = (self.own_uuid(), self.own_entity_id());
                self.api_anvil_update(player, entity_id, s.pos, &mut update, &mut cancel);
                update.answer(cancel, &mut s.anvil);
                s.anvil_answer = Some((s.inputs.clone(), name, s.anvil.clone()));
            }
        }
    }
    /// `slotsChanged` / `createResult` for the open station.
    pub(super) fn station_changed(&mut self) {
        let Some(mut s) = self.station.take() else {
            return;
        };
        let creative = self.creative;
        let (enchantments, data) = (
            std::sync::Arc::clone(self.item_enchantments()),
            self.stations(),
        );
        let items = self.items();
        match s.kind {
            // A server's table: its data slots carry the offers, and its
            // client menu only clears them without an enchantable item.
            Kind::Enchanting if false => {
                let enchantable = s.inputs[0]
                    .as_ref()
                    .is_some_and(|i| stations::is_enchantable(&enchantments, i));
                if !enchantable {
                    s.offers = Default::default();
                }
            }
            Kind::Enchanting => {
                s.bookcases = self.count_bookshelves(s.pos);
                s.offers = stations::enchant_offers(
                    &enchantments,
                    items,
                    self.level.xp_seed,
                    s.bookcases,
                    s.inputs[0].as_ref(),
                );
            }
            Kind::Anvil => {
                let name = self.anvil_item_name(&s);
                let hover = s.inputs[0]
                    .as_ref()
                    .map(|i| stations::hover_name(&self.pack.names, i))
                    .unwrap_or_default();
                s.anvil = stations::anvil_result(
                    &enchantments,
                    items,
                    s.inputs[0].as_ref(),
                    s.inputs[1].as_ref(),
                    Some(&name),
                    &hover,
                    creative,
                );
                // AnvilUpdateEvent on a server's client menu, as on the
                // server: asked when an input or the name changed (this
                // recomputes on every click), its answer kept until then.
                // An empty left slot has no result and forgets the answer.
                if s.inputs[0].is_none() {
                    s.anvil_answer = None;
                }
                s.result = s.anvil.result.clone();
            }
            Kind::Grindstone => {
                s.result = stations::grindstone_result(
                    &self.loop_context,
                    &enchantments,
                    items,
                    s.inputs[0].as_ref(),
                    s.inputs[1].as_ref(),
                );
            }
            Kind::Smithing => {
                s.result = data.recipes.smithing_result(
                    &self.loop_context,
                    &data.enchantments,
                    s.inputs[0].as_ref(),
                    s.inputs[1].as_ref(),
                    s.inputs[2].as_ref(),
                );
            }
            // The menu engine keeps the merchant's result (Game::menu_session).
            Kind::Merchant => {}
            Kind::Beacon => {}
            // The menu engine keeps the cartography table's, the loom's and
            // the stonecutter's state (Game::menu_session).
            Kind::Cartography | Kind::Loom | Kind::Stonecutter => {}
        }
        self.station = Some(s);
    }
    /// `ItemStack.getMaxStackSize` for a station slot: the stack's own
    /// `max_stack_size` (a removed one being vanilla's 1), else the item's.
    pub(super) fn max_stack(&self, stack: &ItemStack) -> u32 {
        stack.max_stack_size(self.res.items())
    }
    /// `Slot.mayPlace` and `getMaxStackSize` for station slots.
    fn slot_rules(&self, slot: usize, stack: &ItemStack) -> (bool, u32) {
        if let Some(BlockEntity::Crafter(c)) = self.container_kind()
            && (300..309).contains(&slot)
        {
            // CrafterSlot.mayPlace: not a disabled slot.
            return (!c.disabled[slot - 300], 64);
        }
        if (300..400).contains(&slot) && !self.brewing_open() && self.refuses(slot, stack) {
            // ShulkerBoxSlot.mayPlace.
            return (false, 64);
        }
        let Some(s) = self.station.as_ref() else {
            return (true, 64);
        };
        if slot < 36 {
            return (true, 64);
        }
        if slot == RESULT {
            return (false, 64);
        }
        (s.kind != Kind::Loom, 64)
    }
    /// `moveItemStackTo` over `targets` (menu order), merging first, then empty slots.
    pub(super) fn move_to(
        &mut self,
        stack: &mut ItemStack,
        targets: &[usize],
        reverse: bool,
    ) -> bool {
        let order: Vec<usize> = if reverse {
            targets.iter().rev().copied().collect()
        } else {
            targets.to_vec()
        };
        let start = stack.count;
        if self.max_stack(stack) > 1 {
            for &t in &order {
                if stack.count == 0 {
                    break;
                }
                let (_, cap) = self.slot_rules(t, stack);
                let max = cap.min(self.max_stack(stack));
                if let Some(s) = self.slot_mut(t).as_mut()
                    && s.same_item_same_components(stack)
                {
                    let n = max.saturating_sub(s.count).min(stack.count);
                    s.count += n;
                    stack.count -= n;
                }
            }
        }
        for &t in &order {
            if stack.count == 0 {
                break;
            }
            let (ok, cap) = self.slot_rules(t, stack);
            if self.slot_mut(t).is_none() && (t < 36 || ok) {
                let n = cap.min(self.max_stack(stack)).min(stack.count);
                *self.slot_mut(t) = Some(ItemStack {
                    count: n,
                    ..stack.clone()
                });
                stack.count -= n;
            }
        }
        stack.count != start
    }
    /// After an anvil or smithing click on the menu engine: the anvil's cost
    /// and state back into the station and the rename box following the
    /// input (`AnvilScreen.slotChanged`), then the rest of a taken result's
    /// `onTake` (the levels, the anvil's wear, the use sound) unless a
    /// server does it.
    pub(super) fn combiner_session_end(
        &mut self,
        menu: &horizon_client::menu::Menu,
        predicting: bool,
    ) {
        let Some(s) = self.station.as_mut() else {
            return;
        };
        let (pos, kind) = (s.pos, s.kind);
        if kind == Kind::Anvil {
            s.anvil.cost = menu.data[0];
            s.anvil.repair_item_count_cost = menu.anvil.repair_count;
            s.anvil.only_renaming = menu.anvil.only_renaming;
            s.anvil.result = s.result.clone();
        }
        self.station_after_input();
        if !menu.combined || predicting {
            return;
        }
        if kind == Kind::Smithing {
            self.level_event_at(1044, pos, 0);
            return;
        }
        self.give_levels(menu.level_change);
        if self.anvil_used(pos, self.creative) {
            self.close_station();
        }
    }
    /// `AnvilMenu.onTake`'s level part: out of creative a 12% chance
    /// (`AnvilBlock.damage`) to wear the anvil a stage, or break a damaged one
    /// (level event 1029); otherwise level event 1030. True when it broke.
    pub(super) fn anvil_used(&mut self, pos: [i32; 3], creative: bool) -> bool {
        let state = self.world.block(pos[0], pos[1], pos[2]);
        let name = self.res.facts().get(state).name.clone();
        let anvil = matches!(
            &*name,
            "minecraft:anvil" | "minecraft:chipped_anvil" | "minecraft:damaged_anvil"
        );
        if !creative && anvil && self.world.random.next_float() < 0.12 {
            match &*name {
                "minecraft:anvil" => {
                    let _ = self.set_state_named(pos, "minecraft:chipped_anvil", &["facing"]);
                    self.native_anvil_damaged(self.dim.id(), pos, &name, "minecraft:chipped_anvil");
                }
                "minecraft:chipped_anvil" => {
                    let _ = self.set_state_named(pos, "minecraft:damaged_anvil", &["facing"]);
                    self.native_anvil_damaged(self.dim.id(), pos, &name, "minecraft:damaged_anvil");
                }
                _ => {
                    let _ = self.world.set(pos[0], pos[1], pos[2], self.world.air);
                    self.world.modified.insert((pos[0] >> 4, pos[2] >> 4));
                    self.native_anvil_damaged(self.dim.id(), pos, &name, "minecraft:air");
                    self.level_event_at(1029, pos, 0);
                    return true;
                }
            }
        }
        self.level_event_at(1030, pos, 0);
        false
    }
    /// The grindstone result's `onTake` with a level (`amount`: the menu's
    /// `getExperienceAmount` sum): the orbs at the block's centre
    /// (`ExperienceOrb.award`), then level event 1042, the grindstone's sound
    /// for everyone around.
    pub(super) fn grindstone_taken(&mut self, pos: [i32; 3], amount: i32) {
        let xp = stations::grindstone_xp_roll(amount, &mut self.world.random);
        self.grindstone_award(pos, xp);
    }
    /// The rest of `onTake` with the experience `xp` decided: the orbs and
    /// level event 1042.
    pub(super) fn grindstone_award(&mut self, pos: [i32; 3], xp: u32) {
        if xp > 0 {
            self.spawn_orbs(pos.map(|v| f64::from(v) + 0.5), xp);
        }
        self.level_event_at(1042, pos, 0);
    }
    fn play_at(&mut self, sound: &str, pos: [i32; 3], volume: f32, pitch: f32) {
        let at = [
            f64::from(pos[0]) + 0.5,
            f64::from(pos[1]) + 0.5,
            f64::from(pos[2]) + 0.5,
        ];
        self.audio.play(sound, Some(at), volume, pitch);
    }
    pub(super) fn give_or_drop(&mut self, stack: ItemStack) {
        if let Some(rest) = self.add_stack_to_inventory(stack)
            && let Some(eye) = self.eye_position()
        {
            // Inventory.placeItemBackInInventory: `player.drop(stack, false)`.
            self.throw_from_eyes(rest, eye, false, false);
        }
    }
    /// `Player.giveExperienceLevels`, and the level change reported to the mods.
    pub(super) fn give_levels(&mut self, levels: i32) {
        let old_level = self.experience.level;
        self.add_experience_levels(levels);
        self.native_player_level_changed(old_level);
    }
    /// `ServerPlayer.giveExperienceLevels`: the mods of a server player may change
    /// or cancel the levels, and every fifth level gained plays the level-up sound
    /// (at most every 100 ticks).
    pub(super) fn add_experience_levels(&mut self, mut levels: i32) {
        // ServerPlayer.giveExperienceLevels
        if levels == 0 {
            return;
        }
        if false
            && let (Some(player), Some(name)) = (self.lan_acting_uuid, self.lan_acting_name.clone())
        {
            let mut cancel = false;
            self.native_player_xp_level_change(player, &name, &mut levels, &mut cancel);
            if cancel {
                return;
            }
        }
        self.experience.add_levels(levels);
        let level = self.experience.level;
        self.play_level_up(levels, level);
    }
    /// `Player.onEnchantmentPerformed`: the cost leaves the level directly, not
    /// through `giveExperienceLevels`, and the mods hear of the level change.
    pub(super) fn enchantment_performed(&mut self, cost: i32) {
        let old_level = self.experience.level;
        self.experience.add_levels(-cost);
        self.native_player_level_changed(old_level);
    }
    /// `Player.giveExperienceLevels`: all players near its feet hear the sound.
    pub(super) fn play_level_up(&mut self, amount: i32, level: i32) {
        if let Some(volume) = self.level_up_sound.give_levels(amount, level) {
            let at = self
                .player
                .as_ref()
                .map(|p| [p.position.x, p.position.y, p.position.z]);
            self.audio
                .play("minecraft:entity.player.levelup", at, volume, 1.0);
        }
    }
    /// Recompute after slot changes; the anvil box follows its input item.
    fn station_after_input(&mut self) {
        let Some(s) = self.station.as_mut() else {
            return;
        };
        if s.kind == Kind::Anvil && s.inputs[0] != s.anvil_last {
            // AnvilScreen.slotChanged: the box follows its input item.
            s.anvil_last = s.inputs[0].clone();
            let name = s.inputs[0]
                .as_ref()
                .map(|i| stations::hover_name(&self.pack.names, i))
                .unwrap_or_default();
            anvil::input_changed(s, name);
        }
        self.station_changed();
    }
    // ---------------------------------------------------------------- brewing stand

    /// `BrewingStandBlockEntity.serverTick` of the brewing stand at `pos`, in
    /// its place among the level's block entity tickers.
    pub(super) fn tick_brewing_stand(
        &mut self,
        pos: (i32, i32, i32),
    ) -> horizon_client::Result<()> {
        let fuel = self.brewing_fuel_remainder(pos);
        let Some(BlockEntity::Brewing(b)) = self.world.block_entities.get_mut(&pos) else {
            return Ok(());
        };
        let was_idle = b.brew_time <= 0;
        // `serverTick` takes the fuel item only while the stand has no fuel
        // uses left, which `out.fuel_taken` reports.
        let fuel_item = b.items[4].as_ref().map(|s| s.item.clone());
        let (mut out, due) = b.tick_to_brew(
            &world_stations!(self).brewing,
            world_items!(self),
            self.res.providers(),
            &|s| fuel.resolve(world_items!(self), s),
        );
        let started = was_idle && b.brew_time > 0;
        let ingredient = started.then(|| b.ingredient.clone()).flatten();
        let hooked = !mod_api::__private::mods().is_empty();
        let stacks = (due && hooked).then(|| b.items.clone());
        let at = [pos.0, pos.1, pos.2];
        let mut post = false;
        if due {
            // `doBrew`, with PotionBrewEvent.Pre before it and .Post after.
            let asked = stacks.map(|mut stacks| {
                let mut cancel = false;
                self.native_potion_brew_pre(at, &mut stacks, &mut cancel);
                (stacks, cancel)
            });
            // A cancelled Pre fires Post itself, with the slots the mods left.
            let cancelled = matches!(asked, Some((_, true)));
            let rest = self.brewing_ingredient_remainder(pos, cancelled);
            let Some(BlockEntity::Brewing(b)) = self.world.block_entities.get_mut(&pos) else {
                return Ok(());
            };
            let brewing = &world_stations!(self).brewing;
            b.brew_after_pre(asked, brewing, &|_| rest.clone(), &mut out);
            self.world.modified.insert((pos.0 >> 4, pos.2 >> 4));
            post = hooked && (out.brewed || cancelled);
        }
        if out.fuel_taken
            && let Some(item) = fuel_item
        {
            self.native_brewing_fuel(at, &item);
        }
        if let Some(ingredient) = ingredient {
            self.native_brewing_start(at, &ingredient);
        }
        if !was_idle || started || out != brewing::BrewTick::default() {
            self.container_changed(pos);
        }
        let (x, y, z) = pos;
        // `doBrew` drops the ingredient's remainder through
        // `Containers.dropItemStack`, before the brew's level event: no
        // `block_drops` test and no pickup delay.
        for d in std::mem::take(&mut out.drops) {
            self.drop_item_stack_containers(at, d);
        }
        if out.brewed {
            self.native_brewing_complete(at);
            self.play_at("minecraft:block.brewing_stand.brew", at, 1.0, 1.0);
        }
        // PotionBrewEvent.Post ends `doBrew`, after its sound and the
        // ingredient's remainder; the bottles' block state follows it.
        if post && let Some(BlockEntity::Brewing(b)) = self.world.block_entities.get(&pos) {
            let mut stacks = b.items.clone();
            self.native_potion_brew_post(at, &mut stacks);
            if let Some(BlockEntity::Brewing(b)) = self.world.block_entities.get_mut(&pos) {
                b.items = stacks;
            }
        }
        let Some(BlockEntity::Brewing(b)) = self.world.block_entities.get_mut(&pos) else {
            return Ok(());
        };
        b.sync_bottles(&mut out);
        if out != brewing::BrewTick::default() {
            self.world.modified.insert((x >> 4, z >> 4));
            if let Some(bottles) = out.bottles {
                let state = self.world.block(x, y, z);
                if let Some(mut s) = self.pack.assets.registry.state(state).cloned()
                    && AsRef::<str>::as_ref(&s.name) == "minecraft:brewing_stand"
                {
                    for (i, has) in bottles.iter().enumerate() {
                        s.properties
                            .insert(format!("has_bottle_{i}"), has.to_string());
                    }
                    if let Ok(id) = self.pack.assets.registry.resolve(&s) {
                        self.level_set_with_flags(
                            [x, y, z],
                            id,
                            horizon_world::level::flags::UPDATE_CLIENTS,
                        )?;
                    }
                }
            }
        }
        if matches!(self.world.block_entities.get(&pos), Some(BlockEntity::Brewing(b)) if b.idle())
        {
            self.world.block_entities.sleep(pos);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "daily"]
    fn anvil_subscriber_probe_and_answer_cache() {
        use mod_api::{self, AnvilUpdateEvent, Flow, HookId, Hooks, HostSide, ModEntry};
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };

        struct DefaultMod;
        #[mod_api::__subscriptions]
        impl Hooks for DefaultMod {}
        struct Listener(Arc<AtomicUsize>);
        #[mod_api::__subscriptions]
        impl Hooks for Listener {
            fn anvil_update(&self, _: HostSide<'_>, event: &mut AnvilUpdateEvent<'_>) -> Flow {
                self.0.fetch_add(1, Ordering::Relaxed);
                event.update.xp_cost = -7;
                Flow::Continue
            }
        }
        let defaults = vec![ModEntry::new("default", Box::new(DefaultMod))].into();
        mod_platform::testing::with_mods(defaults, || {
            super::super::native_entity_refs::tests::with_game(|game| {
                let mut station = super::Station::new(super::Kind::Anvil, [0; 3]);
                station.inputs[0] = Some(super::ItemStack::new("minecraft:iron_sword", 1));
                assert!(!false);
                game.update_anvil_answer(&mut station, "first".into());
                assert!(station.anvil_answer.is_none());
                game.update_anvil_answer(&mut station, "second".into());
                assert!(station.anvil_answer.is_none());
            });
        });
        let calls = Arc::new(AtomicUsize::new(0));
        let mods = vec![ModEntry::new("listener", Box::new(Listener(calls.clone())))].into();
        mod_platform::testing::with_mods(mods, || {
            super::super::native_entity_refs::tests::with_game(|game| {
                let mut station = super::Station::new(super::Kind::Anvil, [0; 3]);
                station.inputs[0] = Some(super::ItemStack::new("minecraft:iron_sword", 1));
                game.update_anvil_answer(&mut station, "first".into());
                assert_eq!(station.anvil.cost, -7);
                station.anvil.cost = 1;
                game.update_anvil_answer(&mut station, "first".into());
                assert_eq!(station.anvil.cost, -7, "cached listener answer is signed");
                game.update_anvil_answer(&mut station, "second".into());
                assert_eq!(station.anvil.cost, -7);
                assert_eq!(calls.load(Ordering::Relaxed), 2);
                assert!(false);
            });
        });
    }

}

#[cfg(test)]
#[path = "tests/brewing_changed_tests.rs"]
mod brewing_changed_tests;

#[cfg(test)]
#[path = "stations/brewing_tick_tests.rs"]
mod brewing_tick_tests;

