//! The player's advancements in the game: criteria triggered from game
//! events (`CriteriaTriggers`), awarding with rewards, the toast and the
//! chat announcement, and `players/advancements/<uuid>.json`.
use super::{Dim, Game};

use horizon_client::{
    advancements::{self as adv, EntityView, Frame, ItemView, Progress, Tree},
    items::ItemStack,
    text::{self, Style, Text},
};
use horizon_core::geometry::Vec3;
use horizon_core::random::RandomSource;
use horizon_protocol::packets::play::Clientbound as C;
use mod_api::ProgressType;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Instant,
};


/// Capture the owning progress without changing its dates or requirement semantics.
fn progress_snapshot(a: &adv::Advancement, progress: &Progress) -> mod_api::AdvancementProgress {
    let obtained = progress.criteria.get(&a.id);
    mod_api::AdvancementProgress::new(
        a.criteria
            .iter()
            .map(|criterion| {
                mod_api::CriterionProgress::new(
                    criterion.name.clone(),
                    obtained
                        .and_then(|criteria| criteria.get(&criterion.name))
                        .cloned(),
                )
            })
            .collect(),
        a.requirements.clone(),
        progress.counts(a).0 as u32,
        progress.done(a),
    )
}

/// Where the player's fall, lava ride and Nether trip began, the blast that
/// last hit the player, and the effects the last tick saw (`ServerPlayer`'s
/// fields of the same names). A guest keeps their own, as `levitation` does.
#[derive(Clone, Debug, Default)]
pub(super) struct TravelMarks {
    /// `startingToFallPosition`.
    pub(super) fall: Option<[f64; 3]>,
    /// `currentExplosionImpactPos` and `currentExplosionCause` (the entity
    /// that made the blast, if any).
    pub(super) blast: Option<([f64; 3], Option<EntityView>)>,
    /// `enteredLavaOnVehiclePosition`.
    pub(super) lava_ride: Option<[f64; 3]>,
    /// `enteredNetherPosition`.
    pub(super) nether: Option<[f64; 3]>,
    /// The (effect, amplifier) pairs `EffectsChangedTrigger` last saw.
    pub(super) effects: Vec<(&'static str, i32)>,
    /// `PlayerEffects::changes` as the last tick saw it.
    pub(super) effect_changes: u32,
}
#[derive(Default)]
pub(super) struct AdvState {
    pub(super) tree: Arc<Tree>,
    pub(super) progress: Progress,
    pub(super) loaded: bool,
    /// Awards skip the toast (`/advancement grant ... everything`).
    pub(super) quiet: bool,
    /// The inventory last seen by `inventory_changed`.
    pub(super) last_inventory: u64,
    /// `ServerPlayer.levitationStartPos` and `levitationStartTime`.
    pub(super) levitation: Option<([f64; 3], u64)>,
    /// The fall, lava ride, Nether trip, blast and effects `tick_marks` follows.
    pub(super) marks: TravelMarks,
    /// While the game acts for a network player (`as_guest`): the
    /// advancements whose progress changed (`progressChanged`), the recipes
    /// unlocked and the announcements, for their client and everyone's.
    pub(super) changed: BTreeSet<usize>,
    pub(super) unlocked: Vec<String>,
    pub(super) announced: Vec<Text>,
    /// The tree being read on another thread since the world opened
    /// (`prefetch_advancement_tree`), taken by the first `advancement_tree`.
    pub(super) pending_tree: Option<std::thread::JoinHandle<Arc<Tree>>>,
    /// `inventory_changed`'s criteria by item, built for `tree`.
    inventory_index: Option<(Arc<Tree>, Arc<adv::InventoryIndex>)>,
}
impl AdvState {
    /// Drop the prefetched tree and the index built for the old one (a
    /// `/reload` rebuilt the tree).
    pub(super) fn reset_tree(&mut self) {
        self.pending_tree = None;
        self.inventory_index = None;
    }
}
/// The game's world for predicates.
struct View<'a, 'g>(&'a Game<'g>);
impl adv::World for View<'_, '_> {
    fn predicate(&self, id: &str) -> Option<&Value> {
        self.0.fns.store.predicates.get(id)
    }
    fn dimension(&self) -> &str {
        self.0.dim.id()
    }
    /// `WeatherCheck.test`'s level queries, using dimension-aware effective levels.
    fn weather(&self) -> (bool, bool) {
        let (rain, thunder) = self.0.weather_levels(1.0);
        (f64::from(rain) > 0.2, f64::from(thunder) > 0.9)
    }
    fn biome(&self, p: [i32; 3]) -> String {
        self.0
            .world
            .chunks
            .get(&(p[0] >> 4, p[2] >> 4))
            .map(|c| {
                self.0
                    .res
                    .biomes()
                    .get(c.biome(p[0], p[1], p[2]))
                    .name
                    .clone()
            })
            .unwrap_or_default()
    }
    fn structures(&self, p: [i32; 3]) -> Vec<String> {
        self.0.structures_at(p)
    }
    fn block(&self, p: [i32; 3]) -> (String, BTreeMap<String, String>) {
        let state = self.0.world.block(p[0], p[1], p[2]);
        let name = self.0.res.facts().get(state).name.to_string();
        let props = self
            .0
            .pack
            .assets
            .registry
            .state(state)
            .map(|s| {
                s.properties
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        (name, props)
    }
    fn block_property_values(&self, block: &str, property: &str) -> Option<Vec<String>> {
        let values = self.0.pack.assets.registry.property_values(block, property);
        (!values.is_empty()).then(|| values.into_iter().map(str::to_owned).collect())
    }
    fn fluid(&self, p: [i32; 3]) -> Option<String> {
        match self
            .0
            .res
            .facts()
            .get(self.0.world.block(p[0], p[1], p[2]))
            .fluid
        {
            horizon_client::blocks::Fluid::Water => Some("minecraft:water".into()),
            horizon_client::blocks::Fluid::Lava => Some("minecraft:lava".into()),
            _ => None,
        }
    }
    /// `Level.canSeeSky`: sky light 15 at the position, whatever stands
    /// above it, as `MobWorld` reads it for the spawn rules.
    fn can_see_sky(&self, p: [i32; 3]) -> bool {
        self.0.mob_world_base(&[]).can_see_sky(p[0], p[1], p[2])
    }
    fn light(&self, p: [i32; 3]) -> i32 {
        let l = self.0.world.light(p[0], p[1], p[2]);
        i32::from((l >> 4).max(l & 15))
    }
    /// `CampfireBlock.isSmokeyPos`: share the beehive smoke test.
    fn smokey(&self, p: [i32; 3]) -> bool {
        horizon_client::beehive::smokey(&self.0.mob_world_base(&[]), p)
    }
    fn block_entity_nbt(&self, p: [i32; 3]) -> Option<horizon_core::nbt::Tag> {
        self.0.block_entity_nbt(p)
    }
    fn in_tag(&self, registry: &str, tag: &str, id: &str) -> bool {
        // The world's packs' tags over the game's own, as the server's
        // registries bind them (`TagLoader.loadTagsForExistingRegistries`).
        if let Some(tags) = self
            .0
            .world_tables
            .as_ref()
            .and_then(|t| t.pack_tags.get(registry))
        {
            return tags.get(tag).is_some_and(|t| t.contains(id));
        }
        let items = self.0.items();
        match registry {
            "item" => items.item_in_tag(tag, id),
            "block" => items.block_in_tag(tag, id),
            "entity_type" => self.0.res.mob_data().entity_has(tag, id),
            "worldgen/biome" => self.0.res.mob_data().biome_has(tag, id),
            "worldgen/structure" => vanilla_structure_tags(&self.0.options.resources)
                .get(tag)
                .is_some_and(|t| t.contains(id)),
            "damage_type" => self
                .0
                .health_resources
                .damage
                .id(id)
                .is_some_and(|source| self.0.health_resources.damage.in_tag(source, tag)),
            _ => false,
        }
    }
}
type Tags = std::collections::HashMap<String, std::collections::HashSet<String>>;
/// The game's own `worldgen/structure` tags (a world without data packs has
/// no `WorldTables` to hold them), loaded once for the resource root first
/// asked for.
fn vanilla_structure_tags(root: &std::path::Path) -> &'static Tags {
    static TAGS: std::sync::OnceLock<Tags> = std::sync::OnceLock::new();
    TAGS.get_or_init(|| {
        horizon_client::items::load_tags(
            root,
            "worldgen/structure",
            &horizon_world::loot::PackOverlay::NONE,
        )
        .unwrap_or_default()
    })
}
fn now_text() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs()) as i64;
    adv::date_text(secs)
}
impl Game<'_> {
    /// `ItemPredicate.test` (the `item_filter` of a filtered item modifier).
    pub(super) fn loot_item_filter(&self, predicate: &Value, s: &ItemStack) -> bool {
        adv::item(predicate, &item_view(s), &View(self))
    }
    /// `BaseContainerBlockEntity.canOpen`: whether the `lock` of the block
    /// entity at `key` keeps a player with this main hand out (`LockCode`).
    pub(super) fn container_locked_for(
        &self,
        key: (i32, i32, i32),
        spectator: bool,
        hand: Option<&ItemStack>,
    ) -> bool {
        let Some(lock) = self.world.block_lock(key) else {
            return false;
        };
        let lock = crate::game::stack_text::tag_json(lock);
        let hand = hand.map(item_view);
        !adv::can_unlock(&lock, spectator, hand.as_ref(), &View(self))
    }
}
/// `MinMaxBounds.Doubles.matchesSqr`: `sqr` against the bounds squared
/// (a missing one matches anything).
fn matches_sqr(v: &Value, sqr: f64) -> bool {
    let (min, max) = match v {
        Value::Null => (None, None),
        Value::Number(n) => (n.as_f64(), n.as_f64()),
        Value::Object(o) => (
            o.get("min").and_then(Value::as_f64),
            o.get("max").and_then(Value::as_f64),
        ),
        _ => return false,
    };
    !min.is_some_and(|m| m * m > sqr) && !max.is_some_and(|m| m * m < sqr)
}
pub(super) fn item_view(s: &ItemStack) -> ItemView {
    ItemView {
        id: s.item.to_string(),
        count: s.count,
        damage: s.damage,
        enchantments: s
            .enchantments()
            .iter()
            .map(|(id, l)| (id.to_string(), *l))
            .collect(),
        potion: s.potion().map(str::to_owned),
        custom_name: s.custom_name().is_some(),
        custom_data: None,
    }
}
impl Game<'_> {
    fn adv_path(&self) -> Option<std::path::PathBuf> {
        Some(super::stats::player_json(
            &self.world_root,
            "advancements",
            self.level.player_uuid?,
        ))
    }
    /// A server with no player of its own and nobody it acts for: there is
    /// no player for advancements and statistics.
    pub(super) fn no_player_here(&self) -> bool {
        self.lan_acting.is_none()
    }
    /// The advancement tree, loaded once.
    pub(super) fn advancement_tree(&mut self) -> Arc<Tree> {
        if self.adv.tree.list.is_empty() {
            let tree = match self.adv.pending_tree.take().map(|h| h.join()) {
                Some(Ok(tree)) => tree,
                _ => {
                    let overlay = self.pack_files();
                    Tree::shared(&self.options.resources.join("data"), &overlay)
                }
            };
            self.adv.tree = tree;
            // With the tree, while the world loads: built on the first
            // inventory change it cost that tick ~10 ms.
            self.inventory_index();
        }
        self.adv.tree.clone()
    }
    /// `inventory_changed`'s criteria by item, for the tree in place.
    fn inventory_index(&mut self) -> Arc<adv::InventoryIndex> {
        let tree = self.adv.tree.clone();
        if let Some((built, index)) = &self.adv.inventory_index
            && Arc::ptr_eq(built, &tree)
        {
            return index.clone();
        }
        let items = self.items();
        let index = Arc::new(adv::InventoryIndex::build(&tree, |tag| {
            items
                .item_tags
                .get(tag)
                .map(|t| t.iter().map(String::as_str).collect())
                .unwrap_or_default()
        }));
        self.adv.inventory_index = Some((tree, index.clone()));
        index
    }
    /// Start reading the advancement files on another thread while the world
    /// loads. The first tick's first criterion trigger used to read and parse
    /// all of them on the game thread (about 60 ms).
    pub(super) fn prefetch_advancement_tree(&mut self, stack: &super::functions::PackStack) {
        if !self.adv.tree.list.is_empty() || self.adv.pending_tree.is_some() {
            return;
        }
        let data = self.options.resources.join("data");
        // The world's packs' advancements go over the game's own, and a
        // native mod's contributed files are already among them: the tables
        // were built for this open (`stage_world_tables`), so the hook is
        // asked once, not again here.
        let files = match &self.world_tables {
            Some(tables) => tables.files.clone(),
            None => super::functions::content_overlay(stack),
        };
        let conditions = match &self.world_tables {
            Some(tables) => tables.conditions.clone(),
            None => horizon_world::conditions::LoadConditions::vanilla(),
        };
        self.adv.pending_tree = std::thread::Builder::new()
            .name("horizon-advancements".into())
            .spawn(move || {
                Tree::shared(
                    &data,
                    &horizon_world::loot::PackOverlay {
                        conditions: &conditions,
                        files: &files,
                    },
                )
            })
            .ok();
    }
    pub(super) fn load_advancements(&mut self) {
        if self.adv.loaded || self.no_player_here() {
            return;
        }
        self.adv.loaded = true;
        self.advancement_tree();
        let progress = self
            .adv_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .map(|v| Progress::from_json(&v))
            .unwrap_or_default();
        // The known recipes are the player's own (`recipeBook`), not the
        // rewards of done advancements: /recipe take sticks.
        let tree = self.adv.tree.clone();
        self.adv.progress = progress;
        self.session.registries.advancements = tree.list.iter().map(|a| a.id.clone()).collect();
        // `PlayerAdvancements.checkForAutomaticTriggers`: an advancement with no
        // criteria is never done (`AdvancementRequirements.test` is false), but
        // its rewards are granted as the progress loads. Vanilla's `award(holder, "")` finds no `""`
        // criterion to grant and so returns without a toast or an announcement;
        // the reward grant is the whole effect, and it happens again on the next
        // load because there is no criterion to record it.
        for index in tree.automatic() {
            self.grant_rewards(&tree.list[index]);
        }
    }
    pub(super) fn save_advancements(&self) {
        if !self.adv.loaded || self.no_player_here() {
            return;
        }
        let Some(path) = self.adv_path() else {
            return;
        };
        let json = self
            .adv
            .progress
            .to_json(&self.adv.tree, horizon_world::DATA_VERSION);
        if let Err(e) = super::stats::write_player_json(&path, &json) {
            log::warn!("{e}");
        }
    }
    /// The predicate view of the ECS entity `id`, from its components (the ECS twin of `mob_entity`): kind, place, motion, fire,
    /// age, gear, effects, name and passengers. Its NBT, team and variant component are not projected.
    pub(in crate::game) fn ecs_predicate_view(&self, id: u32) -> EntityView {
        let Some(v) = self.ecs_view(id) else { return EntityView::default() };
        let mut equipment: [Option<ItemView>; 7] = Default::default();
        for (slot, out) in equipment.iter_mut().enumerate() {
            let Some(stack) = self.ecs.equipment_stack(id, slot) else { continue };
            let mut view = item_view(&stack);
            if slot < 6 {
                view.custom_data = Some(horizon_core::nbt::Tag::Compound(stack.custom_data().cloned().unwrap_or_default()));
            }
            *out = Some(view);
        }
        let passengers = self.ecs_rider_of(id).map(|r| self.ecs_predicate_view(r)).into_iter().collect();
        let free = self.ecs_vehicle_of(id).is_none() && v.rider.is_none() && self.riding != Some(id);
        let movement = free.then(|| {
            let per_tick = self.ecs.velocity(id).map_or([0.0; 3], |m| [m.x, m.y, m.z]);
            adv::MovementView { per_tick, fall_distance: self.ecs.fall_distance(id) }
        });
        EntityView {
            movement,
            kind: v.kind.to_owned(),
            position: v.position,
            on_fire: self.ecs.flag(id, horizon_ecs_proto::bridge::host::Flag::OnFire).unwrap_or(false),
            baby: self.ecs_mob(id).is_some_and(|m| m.baby),
            on_ground: v.on_ground,
            equipment,
            effects: self
                .ecs
                .effects(id)
                .into_iter()
                .map(|e| (horizon_client::brewing::namespaced(e.id).to_owned(), e.amplifier, e.duration))
                .collect(),
            custom_name: self.ecs.custom_name(id).map(Value::String),
            passengers,
            ..Default::default()
        }
    }
    /// The player as predicates see it.
    /// The predicate view of a command target.
    pub(super) fn entity_view_of(&self, t: super::commands::Target) -> EntityView {
        use super::commands::Target;
        match t {
            Target::Player => self.player_entity(),
            Target::Mob(id) => self.ecs_predicate_view(id),
            // A server's player: what its last `ServerboundPlayerInputPacket`
            // held (shift 32) and its game mode.
            Target::Guest(conn) if self.command_guest(conn).is_some() => {
                let g = self.command_guest(conn).expect("checked");
                EntityView {
                    kind: self.target_type(t),
                    position: g.position,
                    sneaking: g.sneaking,
                    input_sneak: g.input & 32 != 0,
                    team: self
                        .score
                        .board
                        .players_team(&g.name)
                        .map(|t| t.name.clone()),
                    gamemode: Some(
                        match g.game_mode {
                            1 => "creative",
                            2 => "adventure",
                            3 => "spectator",
                            _ => "survival",
                        }
                        .to_owned(),
                    ),
                    ..EntityView::default()
                }
            }
            other => EntityView {
                kind: self.target_type(other),
                position: self.target_position(other).unwrap_or_default(),
                ..EntityView::default()
            },
        }
    }
    /// A `ContextAwarePredicate` for `this` at `origin`.
    pub(super) fn conditions_hold(
        &self,
        v: &Value,
        this: Option<&EntityView>,
        origin: [f64; 3],
    ) -> bool {
        adv::conditions(
            v,
            &adv::Entities {
                this,
                origin,
                tool: None,
                block_pos: None,
            },
            &View(self),
        )
    }
    /// `ExecuteCommand.checkCustomPredicate`: no seed/sequence override;
    /// reached literal chance leaves use this active level's random source.
    pub(super) fn command_conditions_hold(
        &mut self,
        v: &Value,
        this: Option<&EntityView>,
        origin: [f64; 3],
    ) -> bool {
        let ctx = adv::Entities {
            this,
            origin,
            tool: None,
            block_pos: None,
        };
        // An invalid reference anywhere fails the whole tree, even under an inversion.
        if !adv::references_valid(v, &View(self)) {
            return false;
        }
        adv::conditions_with(v, &ctx, &mut |leaf, ctx| {
            adv::literal_random_chance(leaf, &mut || self.world.random.next_float())
                .unwrap_or_else(|| adv::conditions(leaf, ctx, &View(self)))
        })
    }
    /// The player as a command's predicates see it: with what it looks at.
    pub(super) fn player_entity(&self) -> EntityView {
        self.player_entity_looking(true)
    }
    /// [`Self::entity_view_of`] for testing `predicate` (a predicate, a list
    /// of conditions or an item modifier): the player with the level,
    /// statistics, advancements and recipes it may ask for, and nothing
    /// copied when it asks for none.
    pub(super) fn entity_view_asked(
        &self,
        t: super::commands::Target,
        predicate: &Value,
    ) -> EntityView {
        self.entity_view_asking(t, adv::facts_asked_in(predicate))
    }
    /// [`Self::entity_view_of`] with the player's records `asked` names.
    pub(super) fn entity_view_asking(
        &self,
        t: super::commands::Target,
        asked: adv::FactsAsked,
    ) -> EntityView {
        use super::commands::Target;
        let mut view = self.entity_view_of(t);
        match t {
            Target::Player => view.player_facts = self.player_facts(asked),
            Target::Guest(conn) => view.player_facts = self.guest_facts(conn, asked),
            _ => {}
        }
        view
    }
    /// [`Self::player_facts`] of a server's player who is not in place
    /// (selectors leave out the one in place): its own records, kept on the
    /// hub while another acts.
    fn guest_facts(&self, conn: u32, asked: adv::FactsAsked) -> adv::PlayerFacts {
        let Some(host) = self.lan.as_ref() else {
            return adv::PlayerFacts::default();
        };
        let Some(g) = host
            .hub
            .guest(conn)
            .filter(|_| self.lan_acting != Some(conn))
        else {
            return adv::PlayerFacts::default();
        };
        let stats = (host.play.get(&conn).map(|p| &p.stats)).filter(|s| s.loaded);
        adv::PlayerFacts {
            level: asked.level.then_some(g.xp_level),
            // The guest's food as last written back from its body.
            food: Some((g.food, g.saturation)),
            stats: stats.filter(|_| asked.stats).map(|s| s.values.clone()),
            advancements: asked
                .advancements
                .then(|| (self.adv.tree.clone(), g.advancements.clone())),
            recipes: asked.recipes.then(|| g.recipe_book.recipes.clone()),
        }
    }
    /// [`Self::player_entity`] for `trigger`'s criteria: what the player
    /// looks at, and its level, statistics, advancements and recipes, only
    /// when one of them may ask (a ray against every entity and copies of
    /// the player's records otherwise), and its compound only when one of
    /// them may test `nbt`.
    pub(super) fn player_entity_for(&mut self, trigger: &str) -> EntityView {
        let mut me = self.player_entity_for_any(&[trigger]);
        if self.adv.tree.may_test_nbt(trigger) {
            me.nbt = self.player_nbt();
        }
        me
    }
    /// [`Self::player_entity_for`] for the criteria of several triggers,
    /// without the player's compound (building it needs `&mut self`).
    pub(super) fn player_entity_for_any(&self, triggers: &[&str]) -> EntityView {
        let tree = &self.adv.tree;
        let mut me = self.player_entity_looking(triggers.iter().any(|t| tree.may_look(t)));
        let asked = (triggers.iter()).fold(adv::FactsAsked::default(), |all, t| {
            all.or(tree.player_facts_asked(t))
        });
        me.player_facts = self.player_facts(asked);
        me
    }
    /// The player's level, statistics, advancements and recipes that
    /// `asked` names, and its food, for `PlayerPredicate`; records not
    /// loaded stay `None`.
    fn player_facts(&self, asked: adv::FactsAsked) -> adv::PlayerFacts {
        let Some(p) = self.player.as_ref() else {
            return adv::PlayerFacts::default();
        };
        adv::PlayerFacts {
            level: asked.level.then_some(self.experience.level),
            // `ServerPlayer.getFoodData`, read by every player predicate:
            // two numbers, so always filled.
            food: Some((p.health.food.food, p.health.food.saturation)),
            stats: (asked.stats && self.stats.loaded).then(|| self.stats.values.clone()),
            advancements: (asked.advancements && self.adv.loaded)
                .then(|| (self.adv.tree.clone(), self.adv.progress.clone())),
            recipes: asked
                .recipes
                .then(|| self.level.recipe_book.recipes.clone()),
        }
    }
    /// The player as predicates see it, with what it looks at when `looking`.
    pub(super) fn player_entity_looking(&self, looking: bool) -> EntityView {
        let Some(p) = self.player.as_ref() else {
            return EntityView::default();
        };
        let mut eq: [Option<ItemView>; 7] = Default::default();
        eq[0] = self.held().map(item_view);
        eq[1] = self.offhand.as_ref().map(item_view);
        for (i, a) in self.armor.iter().enumerate() {
            eq[2 + i] = a.as_ref().map(item_view);
        }
        let vehicle = self.riding.filter(|&id| self.ecs.exists(id)).map(|id| Box::new(self.ecs_predicate_view(id)));
        EntityView {
            kind: "minecraft:player".into(),
            position: [p.position.x, p.position.y, p.position.z],
            // Entity.isOnFire on a server: remainingFireTicks (players aren't fire immune).
            on_fire: p.fire_ticks > 0,
            sneaking: p.pose == horizon_client::pose::Pose::Crouching,
            // Input.shift: the held key, as `BlockItem.place`'s sneaking reads it.
            input_sneak: self.sneak_held,
            sprinting: p.sprinting,
            swimming: p.pose == horizon_client::pose::Pose::Swimming,
            flying: p.flying,
            on_ground: p.ground.on_ground,
            equipment: eq,
            vehicle,
            effects: self
                .potions
                .effects
                .list
                .iter()
                .map(|e| {
                    let id = horizon_client::brewing::namespaced(e.id);
                    (id.to_owned(), e.amplifier, e.duration)
                })
                .collect(),
            gamemode: Some(self.mode.name().to_owned()),
            team: self.player_team().map(|t| t.name.clone()),
            // Vanilla looks for it inside PlayerPredicate.matches whatever the
            // trigger; skipped for a trigger whose criteria can't ask.
            looking_at: looking
                .then(|| self.looking_at_entity().map(Box::new))
                .flatten(),
            ..Default::default()
        }
    }
    /// The player as a passenger of the vehicle `id` (`Entity.getPassengers`).
    fn rider_entity(&self, id: u32) -> Option<EntityView> {
        let p = self.player.as_ref().filter(|_| self.riding == Some(id))?;
        Some(EntityView {
            kind: "minecraft:player".into(),
            position: [p.position.x, p.position.y, p.position.z],
            gamemode: Some(self.mode.name().to_owned()),
            ..Default::default()
        })
    }
    /// A boat as the predicates see it: its type, and its passengers (the
    /// mobs seated on it, then the player riding it).
    pub(super) fn boat_entity(&self, b: &super::boats::Boat) -> EntityView {
        EntityView {
            kind: b.entity_type(),
            position: [b.position.x, b.position.y, b.position.z],
            on_ground: b.on_ground,
            passengers: self
                .ecs
                .passengers_of(b.id)
                .into_iter()
                .map(|id| self.ecs_predicate_view(id))
                .chain(self.rider_entity(b.id))
                .collect(),
            ..Default::default()
        }
    }
    /// `SimpleCriterionTrigger.trigger`: test each open criterion of the
    /// trigger and award what it completes.
    pub(super) fn trigger(&mut self, name: &str, test: impl Fn(&Value, &dyn adv::World) -> bool) {
        if self.lan_acting_fake || self.no_player_here() {
            return;
        }
        self.load_advancements();
        let tree = self.adv.tree.clone();
        let Some(list) = tree.by_trigger.get(name) else {
            return;
        };
        self.trigger_criteria(&tree, list, test);
    }
    /// `trigger` for some of a trigger's criteria: (advancement, criterion)
    /// indexes in `tree`, in tree order.
    fn trigger_criteria(
        &mut self,
        tree: &Tree,
        list: &[(usize, usize)],
        test: impl Fn(&Value, &dyn adv::World) -> bool,
    ) {
        if self.lan_acting_fake {
            return;
        }
        let mut hits = Vec::new();
        {
            let view = View(self);
            for &(ai, ci) in list {
                let a = &tree.list[ai];
                let c = &a.criteria[ci];
                if self.adv.progress.has(&a.id, &c.name) || self.adv.progress.done(a) {
                    continue;
                }
                if test(&c.conditions, &view) {
                    hits.push((ai, ci));
                }
            }
        }
        if hits.is_empty() {
            return;
        }
        let date = now_text();
        for (ai, ci) in hits {
            let a = &tree.list[ai];
            let was = self.adv.progress.done(a);
            let criterion = &a.criteria[ci].name;
            if self.adv.progress.grant(&a.id, criterion, &date) {
                self.advancement_progress(a, criterion, ProgressType::Grant);
            }
            self.progress_changed(ai);
            if !was && self.adv.progress.done(a) {
                self.award(ai);
            }
        }
    }
    /// `PlayerAdvancements.award` / `revoke` after a criterion's progress changed.
    fn advancement_progress(
        &mut self,
        advancement: &adv::Advancement,
        criterion: &str,
        progress_type: ProgressType,
    ) {
        if !false {
            return;
        }
        if let (Some(uuid), Some(entity_id), Some(name)) = (
            self.lan_acting_uuid,
            self.lan_acting_entity,
            self.lan_acting_name.clone(),
        ) {
            let progress = progress_snapshot(advancement, &self.adv.progress);
            self.native_player_advancement_progress(
                mod_api::__private::PlayerAdvancementProgressCall {
                    player: uuid,
                    player_entity_id: entity_id as i32,
                    name: &name,
                    advancement: &advancement.id,
                    criterion,
                    progress_type,
                    progress: &progress,
                },
            );
        }
    }
    /// Lend the hub each guest turn's actual advancement storage, without
    /// moving it back into a record while that turn is still active.
    pub(super) fn with_advancement_hub<R>(
        &mut self,
        run: impl FnOnce(
            &mut horizon_client::lan::hub::Hub,
            &mut horizon_client::lan::hub::advancements::AdvancementOwners<'_>,
        ) -> R,
    ) -> Option<R> {
        use horizon_client::lan::hub::advancements::{AdvancementOwners, AdvancementState};
        let host = self.lan.as_mut()?;
        let mut owners = AdvancementOwners::active(
            &self.lan_guest_swaps,
            AdvancementState {
                progress: &mut self.adv.progress,
                changed: &mut self.adv.changed,
                unlocked: &mut self.adv.unlocked,
                book: &mut self.level.recipe_book,
            },
        );
        Some(run(&mut host.hub, &mut owners))
    }

    /// Grant the hub's tick criteria one at a time: each hook sees the new
    /// progress before the completion's rewards and packets are applied.
    pub(super) fn run_native_advancement_grants(&mut self) {
        let Some(host) = self.lan.as_mut() else {
            return;
        };
        let tasks = std::mem::take(&mut host.hub.native_advancement_grants);
        for task in tasks {
            let mut fresh = Vec::new();
            for grant in &task.grants {
                let Some(was_done) = self
                    .with_advancement_hub(|hub, owners| hub.grant_native_tick(&task, grant, owners))
                    .flatten()
                else {
                    continue;
                };
                fresh.push(grant.clone());
                let definition = task
                    .sync
                    .advancements
                    .definition(&grant.advancement)
                    .expect("granted advancement");
                let done = self
                    .with_advancement_hub(|hub, owners| {
                        hub.advancement_task_progress(&task, owners, |progress| {
                            progress.done(definition)
                        })
                    })
                    .flatten()
                    .unwrap_or(false);
                if self.lan_acting == Some(task.conn) {
                    if !was_done && done {
                        self.award_definition(definition);
                    }
                } else {
                    self.with_advancement_hub(|hub, owners| {
                        hub.reward_native_tick(&task, grant, was_done, owners);
                    });
                }
            }
            self.with_advancement_hub(|hub, owners| {
                hub.finish_native_first_tick(&task, &fresh, owners);
            });
        }
        self.with_advancement_hub(|hub, owners| {
            if hub.native_reload_pending {
                hub.finish_reload_resources(owners);
            }
        });
    }
    /// `PlayerAdvancements.progressChanged`, kept for a network player's client.
    fn progress_changed(&mut self, index: usize) {
        if self.lan_acting.is_some() {
            self.adv.changed.insert(index);
        }
    }
    /// After `ServerRecipeBook.addRecipes`: the toast and the
    /// `recipe_unlocked` trigger of each recipe now known. Returns the
    /// number of recipe displays taken back out because a mod refused their
    /// discovery.
    pub(super) fn recipe_unlocked(&mut self, unlocked: &[String]) -> usize {
        let mut allowed = Vec::with_capacity(unlocked.len());
        let mut refused = Vec::new();
        let mut taken_back = 0;
        if let (Some(player), Some(name)) = (self.lan_acting_uuid, self.lan_acting_name.clone()) {
            for recipe in unlocked {
                let mut cancel = false;
                self.native_player_recipe_discovered(player, &name, recipe, &mut cancel);
                if cancel {
                    refused.push(recipe.clone());
                } else {
                    allowed.push(recipe.clone());
                }
            }
            if !refused.is_empty() {
                let removed = self
                    .level
                    .recipe_book
                    .remove_recipes(&world_stations!(self).recipe_index, &refused);
                taken_back = removed.len();
                if self.lan_acting.is_some() && !removed.is_empty() {
                    self.guest_packets.push(C::RecipeBookRemove(
                        horizon_protocol::packets::recipes::RecipeBookRemove { ids: removed },
                    ));
                }
            }
        } else {
            allowed.extend_from_slice(unlocked);
        }
        if self.lan_acting.is_some() {
            // A network player's client gets them in ClientboundRecipeBookAddPacket.
            self.adv.unlocked.extend_from_slice(&allowed);
        }
        if !allowed.is_empty() {
            self.invalidate_mod_book_placement_knowledge();
        }
        if allowed.is_empty() || self.no_player_here() {
            return taken_back;
        }
        self.load_advancements();
        let tree = self.adv.tree.clone();
        for r in &allowed {
            let list = tree.recipe_criteria(r);
            self.trigger_criteria(&tree, &list, |c, _| {
                c["recipes"]
                    .as_str()
                    .is_some_and(|id| adv::full(id) == r.as_str())
            });
        }
        taken_back
    }
    /// `ServerPlayer.awardRecipes`: fake players learn no recipes.
    pub(super) fn award_recipes(&mut self, recipes: &[String]) -> usize {
        if self.lan_acting_fake {
            return 0;
        }
        let index = &world_stations!(self).recipe_index;
        let (count, unlocked) = self.level.recipe_book.add_recipes(index, recipes);
        count.saturating_sub(self.recipe_unlocked(&unlocked))
    }

    /// `KnowledgeBookItem.use`: the book is used up and its recipes pass through
    /// `award_recipes`; a fake player learns nothing. A book naming a recipe this server does not have is
    /// vanilla's `Invalid recipe: {}` and knows nothing. Whether the click
    /// ends here: `InteractionResult.SUCCESS` does and swings, `FAIL` leaves
    /// the other hand its turn (`Minecraft.startUseItem`).
    pub(super) fn use_knowledge_book(&mut self, stack: &ItemStack) -> bool {
        if &*stack.item != "minecraft:knowledge_book" {
            return false;
        }
        let slot = self.held_slot();
        let mut hand = self.stack_in(slot).cloned();
        let infinite = self.creative;
        let index = &world_stations!(self).recipe_index;
        let result = horizon_client::recipe_book::Settings::knowledge_book_recipes(
            index, &mut hand, infinite,
        );
        *self.stack_in_mut(slot) = hand;
        match result {
            Ok(recipes) => {
                self.award_recipes(&recipes);
                self.award_stat("used", &stack.item, 1);
                self.swing_arm();
                true
            }
            Err(invalid) => {
                if let Some(id) = invalid {
                    log::error!("Invalid recipe: {id}");
                }
                false
            }
        }
    }
    /// `AdvancementRewards.grant`'s pickup: volume 0.2 at the player's feet,
    /// pitch `(nextFloat - nextFloat) * 0.7 + 1`, times 2. `playSound(null, …)`
    /// so the player hears it. The draws are the player's own random
    /// (`player.getRandom()`), which no level draw shares.
    fn advancement_pickup_sound(&mut self) {
        let Some(p) = self.player.as_ref() else {
            return;
        };
        let at = [p.position.x, p.position.y, p.position.z];
        let mut random = horizon_client::random_source::entity();
        let pitch = ((random.next_float() - random.next_float()) * 0.7 + 1.0) * 2.0;
        self.audio
            .play("minecraft:entity.item.pickup", Some(at), 0.2, pitch);
    }
    /// `PlayerAdvancements.award` once done: `AdvancementRewards.grant`,
    /// then toast and announcement.
    fn award(&mut self, index: usize) {
        if self.lan_acting_fake {
            return;
        }
        let tree = self.adv.tree.clone();
        self.award_definition(&tree.list[index]);
    }

    fn award_definition(&mut self, a: &adv::Advancement) {
        if self.lan_acting_fake {
            return;
        }
        if let (Some(player), Some(name)) = (self.lan_acting_uuid, self.lan_acting_name.clone()) {
            self.native_player_advancement_done(player, &name, &a.id);
        }
        let rewards = self.grant_rewards(a);
        if let Some(d) = &a.display {
            if d.announce
                && self
                    .session
                    .rules
                    .bool_at(horizon_client::gamerules::vanilla::SHOW_ADVANCEMENT_MESSAGES)
            {
                let key = match d.frame {
                    Frame::Task => "chat.type.advancement.task",
                    Frame::Goal => "chat.type.advancement.goal",
                    Frame::Challenge => "chat.type.advancement.challenge",
                };
                let name = adv_name(a);
                // PlayerList.broadcastSystemMessage: everyone hears of a
                // network player's advancement.
                match self.lan_acting_name.clone() {
                    // Player.getDisplayName.
                    Some(_) => {
                        let player = self.target_name(super::commands::Target::Player);
                        self.adv
                            .announced
                            .push(Text::translate(key, &[player, name]));
                    }
                    None => {
                        let text = Text::translate(key, &[self.player_display_text(), name]);
                        // The guests hear of the host's advancement too.
                        if let Some(host) = &self.lan {
                            host.hub
                                .system_message(horizon_client::lan::data::component_nbt(
                                    &text.component(),
                                ));
                        }
                        self.system_message(text);
                    }
                }
            }
        }
        self.recipe_unlocked(&rewards);
    }
    /// `AdvancementRewards.grant`: the experience, recipes, loot and function of
    /// a finished advancement, and the recipes it unlocked for the
    /// `recipe_unlocked` trigger, which the caller runs after its toast and
    /// announcement. Returns those recipes.
    fn grant_rewards(&mut self, a: &adv::Advancement) -> Vec<String> {
        // AdvancementRewards.grant: giveExperiencePoints runs even for 0.
        self.give_experience_points(a.rewards.experience);
        // ServerPlayer.awardRecipesByKey: the rewards that name a recipe.
        let rewards: Vec<String> = a
            .rewards
            .recipes
            .iter()
            .filter(|r| world_stations!(self).recipe_index.contains(r))
            .cloned()
            .collect();
        let (_, unlocked) = self
            .level
            .recipe_book
            .add_recipes(&world_stations!(self).recipe_index, &rewards);
        for table in &a.rewards.loot {
            let items = world_items!(self).loot_table(table, false, None, &mut self.world.random);
            for s in items {
                // Inventory.add returns false when anything is left, and
                // AdvancementRewards then drops that remainder with no
                // pickup sound. A stack the inventory takes whole plays
                // entity.item.pickup at the player's feet.
                if let Some(rest) = self.add_stack_to_inventory(s) {
                    if let Some(eye) = self.eye_position() {
                        self.spawn_item(rest, eye, [0.0; 3], 0);
                    }
                } else {
                    self.advancement_pickup_sound();
                }
            }
        }
        if let Some(function) = &a.rewards.function {
            self.run_advancement_reward_function(function);
        }
        unlocked
    }
    /// `InventoryChangeTrigger`, checked when the inventory changes.
    pub(super) fn check_inventory_trigger(&mut self) {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for s in self
            .inventory
            .iter()
            .chain(self.armor.iter())
            .chain(std::iter::once(&self.offhand))
        {
            s.as_ref().map(|s| (&s.item, s.count)).hash(&mut h);
        }
        let key = h.finish();
        if key == self.adv.last_inventory {
            return;
        }
        self.adv.last_inventory = key;
        // Native mods hear the change before any criterion is looked at, so
        // they do with no `inventory_changed` advancement in the tree too
        // (`native_player_inventory_changed`, API 579).
        if true
            && !self.no_player_here()
            && false
        {
            let stacks = inventory_slots(&self.inventory, &self.armor, &self.offhand);
            let player = self.native_acting_id();
            self.native_player_inventory_changed(player, &stacks);
        }
        let stacks: Vec<ItemView> = self
            .inventory
            .iter()
            .chain(self.armor.iter())
            .chain(std::iter::once(&self.offhand))
            .flatten()
            .map(item_view)
            .collect();
        // InventoryChangeTrigger slots: occupied, full and empty counts.
        let all: Vec<&Option<ItemStack>> = self
            .inventory
            .iter()
            .chain(self.armor.iter())
            .chain(std::iter::once(&self.offhand))
            .collect();
        let occupied = all.iter().filter(|s| s.is_some()).count() as f64;
        let empty = all.len() as f64 - occupied;
        let full = all
            .iter()
            .filter_map(|s| s.as_ref())
            .filter(|s| {
                self.res
                    .items()
                    .get(&s.item)
                    .is_some_and(|i| s.count >= i.max_stack)
            })
            .count() as f64;
        // Only the criteria the items held could pass (`InventoryIndex`).
        if self.no_player_here() {
            return;
        }
        self.load_advancements();
        let tree = self.adv.tree.clone();
        let index = self.inventory_index();
        let list = index.candidates(stacks.iter().map(|s| s.id.as_str()));
        if list.is_empty() {
            return;
        }
        let me = self.player_entity_for("minecraft:inventory_changed");
        self.trigger_criteria(&tree, &list, move |c, w| {
            let slots = &c["slots"];
            let range = |k: &str, v: f64| {
                let r = &slots[k];
                r.is_null()
                    || r.as_f64().is_some_and(|n| (n - v).abs() < 0.5)
                    || r.is_object()
                        && r["min"].as_f64().is_none_or(|m| v >= m)
                        && r["max"].as_f64().is_none_or(|m| v <= m)
            };
            if !(range("occupied", occupied) && range("full", full) && range("empty", empty)) {
                return false;
            }
            let mut wanted: Vec<&Value> = c["items"]
                .as_array()
                .map(|a| a.iter().collect())
                .unwrap_or_default();
            for s in &stacks {
                wanted.retain(|p| !adv::item(p, s, w));
            }
            // `InventoryChangeTrigger.TriggerInstance`'s `player`
            // (`ContextAwarePredicate`), tested by `SimpleCriterionTrigger.trigger`.
            wanted.is_empty()
                && adv::conditions(
                    &c["player"],
                    &adv::Entities {
                        this: Some(&me),
                        origin: me.position,
                        tool: None,
                        block_pos: None,
                    },
                    w,
                )
        });
    }
    /// `PlayerTrigger.LOCATION` every 20 of the player's own ticks (`tickCount % 20`), and the others.
    pub(super) fn tick_advancements(&mut self) {
        if self.player.is_none() {
            return;
        }
        self.check_inventory_trigger();
        // ServerPlayer.tick: PlayerTrigger.TICK every tick.
        let me = self.player_entity_for("minecraft:tick");
        self.trigger("minecraft:tick", |c, w| {
            let ctx = adv::Entities {
                this: Some(&me),
                origin: me.position,
                tool: None,
                block_pos: None,
            };
            adv::conditions(&c["player"], &ctx, w)
        });
        // ServerPlayer.onEffectAdded and onEffectsRemoved keep the start of a
        // levitation; tick fires LEVITATION while there is one.
        let levitating = self
            .potions
            .effects
            .list
            .iter()
            .any(|e| e.id == "levitation");
        match (levitating, self.adv.levitation) {
            (true, None) => {
                let at = self
                    .player
                    .as_ref()
                    .map(|p| [p.position.x, p.position.y, p.position.z]);
                self.adv.levitation = at.map(|at| (at, self.ticks));
            }
            (false, Some(_)) => self.adv.levitation = None,
            _ => {}
        }
        if let Some((start, since)) = self.adv.levitation {
            self.trigger_levitation(start, (self.ticks - since) as i64);
        }
        self.tick_marks();
        if self.own_tick_count().is_some_and(|n| n % 20 == 0) {
            let me = self.player_entity_for("minecraft:location");
            self.trigger("minecraft:location", |c, w| {
                let ctx = adv::Entities {
                    this: Some(&me),
                    origin: me.position,
                    tool: None,
                    block_pos: None,
                };
                adv::conditions(&c["player"], &ctx, w)
            });
        }
    }
    /// A player condition plus one entity (`player`, `entity` keys).
    pub(super) fn trigger_entity(&mut self, name: &str, key: &str, target: &EntityView) {
        let me = self.player_entity_for(name);
        let target = target.clone();
        let key = key.to_owned();
        self.trigger(name, move |c, w| {
            let pctx = adv::Entities {
                this: Some(&me),
                origin: me.position,
                tool: None,
                block_pos: None,
            };
            let ectx = adv::Entities {
                this: Some(&target),
                origin: me.position,
                tool: None,
                block_pos: None,
            };
            adv::conditions(&c["player"], &pctx, w) && adv::conditions(&c[&key], &ectx, w)
        });
    }
    /// `KilledTrigger` (`player_killed_entity`): the entity killed and the
    /// killing blow's damage source (none: any `killing_blow` fails).
    pub(super) fn trigger_killed(&mut self, target: &EntityView, blow: Option<&adv::DamageView>) {
        self.trigger_killed_as("minecraft:player_killed_entity", target, blow);
    }
    /// `KilledTrigger.trigger` for any of its three instances
    /// (`player_killed_entity`, `entity_killed_player`,
    /// `kill_mob_near_sculk_catalyst`).
    pub(super) fn trigger_killed_as(
        &mut self,
        name: &str,
        target: &EntityView,
        blow: Option<&adv::DamageView>,
    ) {
        let me = self.player_entity_for(name);
        let target = target.clone();
        let blow = blow.cloned();
        self.trigger(name, move |c, w| {
            adv::killed(c, &me, &target, blow.as_ref(), w)
        });
    }
    /// `KilledByArrowTrigger`: what the arrow killed and the weapon it came from.
    pub(in crate::game) fn trigger_killed_by_arrow(
        &mut self,
        victims: &[EntityView],
        weapon: Option<&ItemStack>,
    ) {
        let me = self.player_entity_for("minecraft:killed_by_arrow");
        let victims = victims.to_vec();
        let weapon = weapon.map(item_view);
        self.trigger("minecraft:killed_by_arrow", move |c, w| {
            adv::killed_by_arrow(c, &me, &victims, weapon.as_ref(), w)
        });
    }
    /// `LevitationTrigger`: how far the player rose from where it started
    /// levitating, and for how long.
    fn trigger_levitation(&mut self, start: [f64; 3], duration: i64) {
        let me = self.player_entity_for("minecraft:levitation");
        self.trigger("minecraft:levitation", move |c, w| {
            let pctx = adv::Entities {
                this: Some(&me),
                origin: me.position,
                tool: None,
                block_pos: None,
            };
            adv::conditions(&c["player"], &pctx, w)
                && (c["distance"].is_null() || adv::distance(&c["distance"], start, me.position))
                && adv::range_i(&c["duration"], duration)
        });
    }
    /// `ServerPlayer.tick`: `trackStartFallingPosition` and the end of a fall
    /// (`resetFallDistance`), `trackEnteredOrExitedLavaOnVehicle`, the way back
    /// from the Nether, and `onEffectAdded`, `onEffectUpdated` and
    /// `onEffectsRemoved`, which are seen as each tick ends.
    fn tick_marks(&mut self) {
        let Some(p) = self.player.as_ref() else {
            return;
        };
        let at = [p.position.x, p.position.y, p.position.z];
        let falling = p.fall_distance > 0.0;
        let dead = p.health.dead_or_dying();
        // The riding mount's lava flag has no ECS reader yet: not in lava.
        let lava = false;
        match (falling, self.adv.marks.fall) {
            (true, None) => {
                self.adv.marks.fall = Some(at);
                // `trackStartFallingPosition`: the last blast is spent by the
                // fall (whatever made it), and counts only if the player was
                // above where it hit.
                if let Some((impact, cause)) = self.adv.marks.blast.take()
                    && impact[1] <= at[1]
                {
                    self.trigger_fall_after_explosion(impact, cause);
                }
            }
            // `resetFallDistance`: a fall ends by landing, climbing or water.
            (false, Some(start)) => {
                self.adv.marks.fall = None;
                if !dead {
                    self.trigger_distance("minecraft:fall_from_height", start);
                }
            }
            _ => {}
        }
        match (lava, self.adv.marks.lava_ride) {
            (true, None) => self.adv.marks.lava_ride = Some(at),
            (true, Some(start)) => self.trigger_distance("minecraft:ride_entity_in_lava", start),
            (false, Some(_)) => self.adv.marks.lava_ride = None,
            (false, None) => {}
        }
        // Back in the Overworld from the Nether: measured from where it was entered.
        if let Some(start) = self.adv.marks.nether
            && self.dim == Dim::Overworld
        {
            self.adv.marks.nether = None;
            self.trigger_distance("minecraft:nether_travel", start);
        }
        let effects = self.effects_seen();
        // `onEffectAdded`, `onEffectUpdated` (an `update` that changed the
        // duration, amplifier or flags too) and `onEffectsRemoved`.
        let changes = self.potions.effects.changes;
        if effects != self.adv.marks.effects || changes != self.adv.marks.effect_changes {
            self.adv.marks.effects = effects;
            self.adv.marks.effect_changes = changes;
            self.trigger_effects_changed(None);
        }
    }
    /// The (effect, amplifier) pairs `marks.effects` compares.
    fn effects_seen(&self) -> Vec<(&'static str, i32)> {
        self.potions
            .effects
            .list
            .iter()
            .map(|e| (e.id, e.amplifier))
            .collect()
    }
    /// `EffectsChangedTrigger` for a change the caller knows the source of;
    /// the end-of-tick check then does not fire it a second time.
    pub(super) fn trigger_effects_changed_by(&mut self, source: Option<EntityView>) {
        self.adv.marks.effects = self.effects_seen();
        self.adv.marks.effect_changes = self.potions.effects.changes;
        self.trigger_effects_changed(source);
    }
    /// `DistanceTrigger`: the player's `player` condition, `start_position`
    /// at `start`, and the `distance` from `start` to the player.
    pub(super) fn trigger_distance(&mut self, name: &str, start: [f64; 3]) {
        let me = self.player_entity_for(name);
        self.trigger(name, move |c, w| {
            let pctx = adv::Entities {
                this: Some(&me),
                origin: me.position,
                tool: None,
                block_pos: None,
            };
            adv::conditions(&c["player"], &pctx, w)
                && adv::location(&c["start_position"], start, w)
                && (c["distance"].is_null() || adv::distance(&c["distance"], start, me.position))
        });
    }
    /// `FallAfterExplosionTrigger.trigger`: a blast that hit the player at
    /// `impact` (made by `cause`, or by nothing), measured to where the
    /// player is now. `matches` tests `cause` only when the instance has one,
    /// and a missing cause fails it.
    fn trigger_fall_after_explosion(&mut self, impact: [f64; 3], cause: Option<EntityView>) {
        let me = self.player_entity_for("minecraft:fall_after_explosion");
        self.trigger("minecraft:fall_after_explosion", move |c, w| {
            let pctx = adv::Entities {
                this: Some(&me),
                origin: me.position,
                tool: None,
                block_pos: None,
            };
            adv::conditions(&c["player"], &pctx, w)
                && adv::location(&c["start_position"], impact, w)
                && (c["distance"].is_null() || adv::distance(&c["distance"], impact, me.position))
                && (c["cause"].is_null()
                    || cause.as_ref().is_some_and(|cause| {
                        let cctx = adv::Entities {
                            this: Some(cause),
                            origin: me.position,
                            tool: None,
                            block_pos: None,
                        };
                        adv::conditions(&c["cause"], &cctx, w)
                    }))
        });
    }
    /// `EffectsChangedTrigger`: the player's `effects`, and for an effect that
    /// `source` gave, that entity (`source`).
    fn trigger_effects_changed(&mut self, source: Option<EntityView>) {
        let me = self.player_entity_for("minecraft:effects_changed");
        self.trigger("minecraft:effects_changed", move |c, w| {
            let pctx = adv::Entities {
                this: Some(&me),
                origin: me.position,
                tool: None,
                block_pos: None,
            };
            let effects = c["effects"].is_null()
                || adv::entity(
                    &serde_json::json!({ "minecraft:effects": c["effects"] }),
                    &me,
                    w,
                    me.position,
                );
            let source = c["source"].is_null()
                || source.as_ref().is_some_and(|s| {
                    let sctx = adv::Entities {
                        this: Some(s),
                        origin: me.position,
                        tool: None,
                        block_pos: None,
                    };
                    adv::conditions(&c["source"], &sctx, w)
                });
            adv::conditions(&c["player"], &pctx, w) && effects && source
        });
    }
    /// `ItemDurabilityTrigger.trigger` and `TriggerInstance.matches`: a player's
    /// item takes damage (`ItemStack.applyDamage`), `stack` as it is before the
    /// damage and `new_damage` the unclamped damage value it is about to get.
    /// `durability` tests `maxDamage - newDamage`, `delta` tests
    /// `damageValue - newDamage`.
    pub(super) fn trigger_item_durability(&mut self, stack: &ItemStack, new_damage: i32) {
        if self.no_player_here() {
            return;
        }
        let me = self.player_entity_for("minecraft:item_durability_changed");
        let item = item_view(stack);
        let max = match stack
            .data()
            .and_then(|data| data.other("minecraft:max_damage"))
        {
            Some(horizon_core::nbt::Tag::Int(value)) => i64::from(*value),
            _ => i64::from(
                self.res
                    .items()
                    .get(&stack.item)
                    .map_or(0, |i| i.max_damage),
            ),
        };
        // ItemStack.getDamageValue clamps to 0..=getMaxDamage.
        let damage = i64::from(stack.damage).clamp(0, max);
        let new_damage = i64::from(new_damage);
        self.trigger("minecraft:item_durability_changed", move |c, w| {
            let pctx = adv::Entities {
                this: Some(&me),
                origin: me.position,
                tool: None,
                block_pos: None,
            };
            adv::conditions(&c["player"], &pctx, w)
                && (c["item"].is_null() || adv::item(&c["item"], &item, w))
                && adv::range_i(&c["durability"], max - new_damage)
                && adv::range_i(&c["delta"], damage - new_damage)
        });
    }
    /// `ServerPlayer.updateUsingItem`: USING_ITEM for the item in use.
    pub(super) fn trigger_using_item(&mut self, stack: &ItemStack) {
        if self.no_player_here() {
            return;
        }
        let me = self.player_entity_for("minecraft:using_item");
        let it = item_view(stack);
        self.trigger("minecraft:using_item", move |c, w| {
            let pctx = adv::Entities {
                this: Some(&me),
                origin: me.position,
                tool: None,
                block_pos: None,
            };
            adv::conditions(&c["player"], &pctx, w) && adv::item(&c["item"], &it, w)
        });
    }
    /// `UsedEnderEyeTrigger.trigger`: the player threw an eye of ender toward
    /// a structure `sqr_distance` (squared, horizontal) away.
    pub(super) fn trigger_used_ender_eye(&mut self, sqr_distance: f64) {
        if self.no_player_here() {
            return;
        }
        let me = self.player_entity_for("minecraft:used_ender_eye");
        self.trigger("minecraft:used_ender_eye", move |c, w| {
            let pctx = adv::Entities {
                this: Some(&me),
                origin: me.position,
                tool: None,
                block_pos: None,
            };
            adv::conditions(&c["player"], &pctx, w) && matches_sqr(&c["distance"], sqr_distance)
        });
    }
    /// The entity a 100 block ray from the eyes meets first, if no block is
    /// in the way (`ProjectileUtil.getEntityHitResult` over every entity but
    /// the player that is not a spectator, then `hasLineOfSight`).
    fn looking_at_entity(&self) -> Option<EntityView> {
        use super::commands::Target;
        use super::entity::{Entity as _, entity_lists_of, for_each_entity};
        use horizon_core::geometry::Aabb;
        let eye = self.eye_position()?;
        let dir = super::view_vector(self.yaw, self.pitch);
        let mut best: Option<(Target, f64)> = None;
        let mut offer = |t: Option<Target>, b: Aabb| {
            if let Some(t) = t
                && let Some(d) = super::ray_box(eye, dir, 100.0, b)
                && best.is_none_or(|(_, bd)| d < bd)
            {
                best = Some((t, d));
            }
        };
        for_each_entity!(entity_lists_of!(self), e => {
            if !e.is_removed() {
                offer(e.selector_target(), e.bounds());
            }
        });
        let aabb = |(lo, hi): ([f64; 3], [f64; 3])| {
            Aabb::new(
                Vec3::new(lo[0], lo[1], lo[2]),
                Vec3::new(hi[0], hi[1], hi[2]),
            )
        };
        for f in self.ecs_frames().into_iter() {
            offer(Some(Target::Frame(f.id)), aabb(f.bounds()));
        }
        for p in self.ecs_paintings().into_iter() {
            offer(Some(Target::Painting(p.id)), aabb(p.bounds()));
        }
        for (id, _) in self.ecs.crystal_views() {
            if let Some(b) = self.ecs.bounds(id) {
                offer(Some(Target::Crystal(id)), b);
            }
        }
        let (t, _) = best?;
        // `LivingEntity.hasLineOfSight`: eye to the target's eye height,
        // `ClipContext.Block.COLLIDER`, no fluids, within 128 blocks.
        let at = self.target_position(t)?;
        let from = Vec3::new(eye[0], eye[1], eye[2]);
        let to = Vec3::new(at[0], at[1] + self.target_eye_height(t), at[2]);
        let world = self.mob_world_base(&[]);
        let clear = to.distance(from) <= 128.0 && world.clear_between(from, to, false);
        if !clear {
            return None;
        }
        Some(match t {
            Target::Mob(id) => self.ecs.exists(id).then(|| self.ecs_predicate_view(id))?,
            Target::Boat(id) => self.is_boat_id(id).then(|| self.ecs_predicate_view(id))?,
            other => self.entity_view_of(other),
        })
    }
    /// `PlayerHurtEntityTrigger`: the player, the damage and the victim.
    pub(super) fn trigger_player_hurt_entity(
        &mut self,
        victim: &EntityView,
        damage: &adv::DamageView,
    ) {
        let me = self.player_entity_for("minecraft:player_hurt_entity");
        let (victim, damage) = (victim.clone(), damage.clone());
        self.trigger("minecraft:player_hurt_entity", move |c, w| {
            let pctx = adv::Entities {
                this: Some(&me),
                origin: me.position,
                tool: None,
                block_pos: None,
            };
            let vctx = adv::Entities {
                this: Some(&victim),
                ..pctx
            };
            adv::conditions(&c["player"], &pctx, w)
                && adv::damage(&c["damage"], &damage, w, me.position)
                && adv::conditions(&c["entity"], &vctx, w)
        });
    }
    /// The player's hit on `victim` as `LivingEntity.hurtServer` reports it:
    /// `kind` damage of `amount` from `direct` (the player's own hand when
    /// `None`), nothing blocked.
    pub(super) fn player_hit(
        &mut self,
        victim: &EntityView,
        kind: &str,
        direct: Option<EntityView>,
        amount: f32,
    ) {
        let me = self.player_entity_for("minecraft:player_hurt_entity");
        self.trigger_player_hurt_entity(
            victim,
            &adv::DamageView {
                kind: kind.to_owned(),
                is_direct: direct.is_none(),
                direct: direct.or_else(|| Some(me.clone())),
                source: Some(me),
                original: amount,
                actual: amount,
                blocked: false,
            },
        );
    }
    /// `EntityHurtPlayerTrigger`: the player and the damage it took.
    pub(super) fn trigger_entity_hurt_player(&mut self, damage: &adv::DamageView) {
        let me = self.player_entity_for("minecraft:entity_hurt_player");
        let damage = damage.clone();
        self.trigger("minecraft:entity_hurt_player", move |c, w| {
            let pctx = adv::Entities {
                this: Some(&me),
                origin: me.position,
                tool: None,
                block_pos: None,
            };
            adv::conditions(&c["player"], &pctx, w)
                && adv::damage(&c["damage"], &damage, w, me.position)
        });
    }
    /// A player condition plus one item (`item` key).
    pub(super) fn trigger_item(&mut self, name: &str, stack: &ItemStack) {
        let me = self.player_entity_for(name);
        let it = item_view(stack);
        self.trigger(name, move |c, w| {
            let pctx = adv::Entities {
                this: Some(&me),
                origin: me.position,
                tool: None,
                block_pos: None,
            };
            adv::conditions(&c["player"], &pctx, w) && adv::item(&c["item"], &it, w)
        });
    }
    pub(super) fn trigger_item_entity(
        &mut self,
        name: &str,
        stack: &ItemStack,
        target: &EntityView,
    ) {
        let me = self.player_entity_for(name);
        let it = item_view(stack);
        self.trigger(name, |c, w| adv::item_entity(c, &me, target, &it, w));
    }
    /// `ServerPlayer.onItemPickup`: the player in place (the host, or the
    /// guest `as_guest` runs) picked up an item entity whose owner
    /// (`ItemEntity.getOwner`, the thrower) is `thrower`: the whole stack, or
    /// what is left after a partial take. An owner that is that player also
    /// fires `LivingEntity.onItemPickup` (the by-entity trigger); one that is
    /// gone, or another player, fires nothing here.
    pub(super) fn trigger_thrown_picked_up_by_player(
        &mut self,
        stack: &ItemStack,
        thrower: [i32; 4],
    ) {
        let target = if thrower == self.acting_uuid() {
            // The player is the entity of both triggers.
            let me = self.player_entity_for_any(&[
                "minecraft:thrown_item_picked_up_by_entity",
                "minecraft:thrown_item_picked_up_by_player",
            ]);
            self.trigger_item_entity("minecraft:thrown_item_picked_up_by_entity", stack, &me);
            me
        } else if let Some(id) = self.mob_by_uuid(thrower) {
            self.ecs_predicate_view(id)
        } else {
            return;
        };
        self.trigger_item_entity("minecraft:thrown_item_picked_up_by_player", stack, &target);
    }
    /// `ConstructBeaconTrigger`: the pyramid levels of the new beacon.
    pub(super) fn trigger_construct_beacon(&mut self, levels: i32) {
        let me = self.player_entity_for("minecraft:construct_beacon");
        self.trigger("minecraft:construct_beacon", move |c, w| {
            let pctx = adv::Entities {
                this: Some(&me),
                origin: me.position,
                tool: None,
                block_pos: None,
            };
            adv::conditions(&c["player"], &pctx, w) && adv::range_i(&c["level"], i64::from(levels))
        });
    }
    /// `EnchantedItemTrigger`: the enchanted item and the levels it cost.
    pub(super) fn trigger_enchanted_item(&mut self, stack: &ItemStack, levels: i32) {
        let me = self.player_entity_for("minecraft:enchanted_item");
        let it = item_view(stack);
        self.trigger("minecraft:enchanted_item", move |c, w| {
            let pctx = adv::Entities {
                this: Some(&me),
                origin: me.position,
                tool: None,
                block_pos: None,
            };
            adv::conditions(&c["player"], &pctx, w)
                && adv::item(&c["item"], &it, w)
                && adv::range_i(&c["levels"], i64::from(levels))
        });
    }
    /// `BrewedPotionTrigger`: the potion contents of a stack taken from a
    /// brewing stand.
    pub(super) fn trigger_brewed_potion(&mut self, stack: &ItemStack) {
        let me = self.player_entity_for("minecraft:brewed_potion");
        let potion = stack.potion().map(str::to_owned);
        self.trigger("minecraft:brewed_potion", move |c, w| {
            let pctx = adv::Entities {
                this: Some(&me),
                origin: me.position,
                tool: None,
                block_pos: None,
            };
            adv::conditions(&c["player"], &pctx, w)
                && adv::potions(&c["potion"], potion.as_deref(), w)
        });
    }
    /// `TradeTrigger`: the villager and the item the trade gave.
    pub(super) fn trigger_trade(&mut self, villager: &EntityView, stack: &ItemStack) {
        let me = self.player_entity_for("minecraft:villager_trade");
        let villager = villager.clone();
        let it = item_view(stack);
        self.trigger("minecraft:villager_trade", move |c, w| {
            let pctx = adv::Entities {
                this: Some(&me),
                origin: me.position,
                tool: None,
                block_pos: None,
            };
            let vctx = adv::Entities {
                this: Some(&villager),
                ..pctx
            };
            adv::conditions(&c["player"], &pctx, w)
                && adv::conditions(&c["villager"], &vctx, w)
                && adv::item(&c["item"], &it, w)
        });
    }
    /// `FishingRodHookedTrigger`: the rod, what the hook is in (or the hook)
    /// and the items it caught.
    pub(super) fn trigger_fishing_rod_hooked(
        &mut self,
        rod: &ItemStack,
        hooked: &EntityView,
        items: &[ItemStack],
    ) {
        let me = self.player_entity_for("minecraft:fishing_rod_hooked");
        let rod = item_view(rod);
        let hooked = hooked.clone();
        let items: Vec<ItemView> = items.iter().map(item_view).collect();
        self.trigger("minecraft:fishing_rod_hooked", move |c, w| {
            let pctx = adv::Entities {
                this: Some(&me),
                origin: me.position,
                tool: None,
                block_pos: None,
            };
            let ectx = adv::Entities {
                this: Some(&hooked),
                ..pctx
            };
            adv::conditions(&c["player"], &pctx, w)
                && adv::item(&c["rod"], &rod, w)
                && adv::conditions(&c["entity"], &ectx, w)
                && (c["item"].is_null() || items.iter().any(|i| adv::item(&c["item"], i, w)))
        });
    }
    /// `EnterBlockTrigger`: the block, its state properties and the player.
    pub(super) fn trigger_enter_block(&mut self, state: horizon_world::registry::BlockStateId) {
        let Some(definition) = self.pack.assets.registry.state(state).cloned() else {
            return;
        };
        let me = self.player_entity_for("minecraft:enter_block");
        self.trigger("minecraft:enter_block", move |c, w| {
            let block_ok = adv::id_matches(&c["blocks"], "block", &definition.name, w);
            let state_ok = c["state"]
                .as_object()
                .into_iter()
                .flatten()
                .all(|(k, v)| definition.properties.get(k).map(String::as_str) == v.as_str());
            let pctx = adv::Entities {
                this: Some(&me),
                origin: me.position,
                tool: None,
                block_pos: None,
            };
            block_ok && state_ok && adv::conditions(&c["player"], &pctx, w)
        });
    }
    /// `SlideDownBlockTrigger`: match the actual block and its state.
    pub(super) fn trigger_honey_slide(&mut self, pos: [i32; 3]) {
        let me = self.player_entity_for("minecraft:slide_down_block");
        self.trigger("minecraft:slide_down_block", move |c, w| {
            let ctx = adv::Entities {
                this: Some(&me),
                origin: me.position,
                tool: None,
                block_pos: None,
            };
            adv::conditions(&c["player"], &ctx, w) && adv::block(c, pos, w)
        });
    }
    /// `TargetBlockTrigger`: signal and projectile conditions are evaluated in
    /// the shooter's context, including distance from the shooter.
    pub(super) fn trigger_target_hit(&mut self, projectile: &EntityView, strength: i32) {
        let me = self.player_entity_for("minecraft:target_hit");
        let projectile = projectile.clone();
        self.trigger("minecraft:target_hit", move |c, w| {
            let player = adv::Entities {
                this: Some(&me),
                origin: me.position,
                tool: None,
                block_pos: None,
            };
            let target = adv::Entities {
                this: Some(&projectile),
                ..player
            };
            adv::conditions(&c["player"], &player, w)
                && adv::range_i(&c["signal_strength"], i64::from(strength))
                && adv::conditions(&c["projectile"], &target, w)
        });
    }
    /// `ItemUsedOnLocationTrigger` (placed_block, item_used_on_block).
    pub(super) fn trigger_block(&mut self, name: &str, pos: [i32; 3], stack: &ItemStack) {
        #[cfg(test)]
        crate::game::block_use_completion_tests::record(name, Some(stack));
        let me = self.player_entity_for(name);
        let it = item_view(stack);
        self.trigger(name, move |c, w| {
            let ctx = adv::Entities {
                this: Some(&me),
                origin: me.position,
                tool: Some(&it),
                block_pos: Some(pos),
            };
            adv::conditions(&c["location"], &ctx, w)
        });
    }
    /// `DefaultBlockInteractionTrigger` (default_block_use): the player, and
    /// the block's `BLOCK_USE` context at its centre, with no tool.
    pub(super) fn trigger_default_block_use(&mut self, pos: [i32; 3]) {
        self.trigger_block_use("minecraft:default_block_use", pos, None);
    }
    /// `AnyBlockInteractionTrigger` (any_block_use): the player, and the
    /// block's `ADVANCEMENT_LOCATION` context at its centre with the original
    /// stack and its in-place edits, as documented on `Game::finish_block_use`.
    pub(super) fn trigger_any_block_use(&mut self, pos: [i32; 3], stack: &ItemStack) {
        self.trigger_block_use("minecraft:any_block_use", pos, Some(stack));
    }
    fn trigger_block_use(&mut self, name: &str, pos: [i32; 3], stack: Option<&ItemStack>) {
        #[cfg(test)]
        crate::game::block_use_completion_tests::record(name, stack);
        let me = self.player_entity_for(name);
        let it = stack.map(item_view);
        let centre = pos.map(|c| f64::from(c) + 0.5);
        self.trigger(name, move |c, w| {
            let player = adv::Entities {
                this: Some(&me),
                origin: me.position,
                tool: None,
                block_pos: None,
            };
            let location = adv::Entities {
                origin: centre,
                tool: it.as_ref(),
                block_pos: Some(pos),
                ..player
            };
            adv::conditions(&c["player"], &player, w)
                && adv::conditions(&c["location"], &location, w)
        });
    }
    /// `ChanneledLightningTrigger`: each victim condition matched by some
    /// mob the bolt hit, from the player who channelled it.
    pub(in crate::game) fn trigger_channeled_lightning(&mut self, victims: &[EntityView]) {
        let me = self.player_entity_for("minecraft:channeled_lightning");
        let victims = victims.to_vec();
        self.trigger("minecraft:channeled_lightning", move |c, w| {
            let pctx = adv::Entities {
                this: Some(&me),
                origin: me.position,
                tool: None,
                block_pos: None,
            };
            adv::conditions(&c["player"], &pctx, w)
                && c["victims"].as_array().into_iter().flatten().all(|want| {
                    victims.iter().any(|v| {
                        adv::conditions(
                            want,
                            &adv::Entities {
                                this: Some(v),
                                ..pctx
                            },
                            w,
                        )
                    })
                })
        });
    }
    /// `SpearMobsTrigger`: the living entities the spear stabbed in this use.
    pub(super) fn trigger_spear_mobs(&mut self, stabbed: usize) {
        let me = self.player_entity_for("minecraft:spear_mobs");
        self.trigger("minecraft:spear_mobs", move |c, w| {
            let pctx = adv::Entities {
                this: Some(&me),
                origin: me.position,
                tool: None,
                block_pos: None,
            };
            adv::conditions(&c["player"], &pctx, w)
                && c["count"].as_u64().is_none_or(|n| stabbed as u64 >= n)
        });
    }
    /// `BeeNestDestroyedTrigger`: the nest's block state, the tool that broke
    /// it and the bees left inside.
    pub(super) fn trigger_bee_nest_destroyed(
        &mut self,
        state: horizon_world::registry::BlockStateId,
        tool: Option<&ItemStack>,
        bees: i64,
    ) {
        let Some(definition) = self.pack.assets.registry.state(state).cloned() else {
            return;
        };
        let me = self.player_entity_for("minecraft:bee_nest_destroyed");
        let tool = tool.map(item_view).unwrap_or_default();
        self.trigger("minecraft:bee_nest_destroyed", move |c, w| {
            let pctx = adv::Entities {
                this: Some(&me),
                origin: me.position,
                tool: None,
                block_pos: None,
            };
            let state_ok = c["state"]
                .as_object()
                .into_iter()
                .flatten()
                .all(|(k, v)| definition.properties.get(k).map(String::as_str) == v.as_str());
            adv::id_matches(&c["blocks"], "block", &definition.name, w)
                && state_ok
                && adv::item(&c["item"], &tool, w)
                && adv::range_i(&c["num_bees_inside"], bees)
                && adv::conditions(&c["player"], &pctx, w)
        });
    }
    /// `Entity.startRiding`'s `sendEventAndTriggers`: `START_RIDING_TRIGGER`
    /// for every `ServerPlayer` in `entityToRide.getIndirectPassengersStream()`.
    pub(super) fn trigger_started_riding(&mut self, vehicle: u32) {
        let mounts: Vec<(u32, Option<u32>)> =
            self.ecs.followed_ids().into_iter().map(|id| (id, self.ecs.vehicle_of(id))).collect();
        let aboard = super::boarding::passenger_vehicles(vehicle, &mounts);
        self.for_each_player(|g| {
            if g.riding.is_some_and(|v| aboard.contains(&v)) {
                g.trigger_player("minecraft:started_riding");
            }
        });
    }
    /// A trigger with only the player condition (slept_in_bed, started_riding).
    pub(super) fn trigger_player(&mut self, name: &str) {
        let me = self.player_entity_for(name);
        self.trigger(name, move |c, w| {
            let pctx = adv::Entities {
                this: Some(&me),
                origin: me.position,
                tool: None,
                block_pos: None,
            };
            adv::conditions(&c["player"], &pctx, w)
        });
    }
    /// `RecipeCraftedTrigger`: the recipe and the stacks that went into it
    /// (`ServerPlayer.triggerRecipeCrafted`); an empty slot is air.
    pub(super) fn trigger_recipe_crafted(
        &mut self,
        recipe: &str,
        ingredients: &[Option<ItemStack>],
    ) {
        let me = self.player_entity_for("minecraft:recipe_crafted");
        let used: Vec<ItemView> = ingredients
            .iter()
            .map(|s| {
                s.as_ref().map_or_else(
                    || ItemView {
                        id: "minecraft:air".to_owned(),
                        ..Default::default()
                    },
                    item_view,
                )
            })
            .collect();
        let recipe = recipe.to_owned();
        self.trigger("minecraft:recipe_crafted", move |c, w| {
            let pctx = adv::Entities {
                this: Some(&me),
                origin: me.position,
                tool: None,
                block_pos: None,
            };
            adv::conditions(&c["player"], &pctx, w)
                && adv::id_matches(&c["recipes"], "recipe", &recipe, w)
                && adv::recipe_ingredients(&c["ingredients"], &used, w)
        });
    }
    /// `LootTableTrigger`: a container's loot table unpacked for this player
    /// (`RandomizableContainer.unpackLootTable`).
    pub(super) fn trigger_loot_table(&mut self, table: &str) {
        let me = self.player_entity_for("minecraft:player_generates_container_loot");
        let table = table.to_owned();
        self.trigger("minecraft:player_generates_container_loot", move |c, w| {
            let pctx = adv::Entities {
                this: Some(&me),
                origin: me.position,
                tool: None,
                block_pos: None,
            };
            adv::conditions(&c["player"], &pctx, w)
                && adv::id_matches(&c["loot_tables"], "loot_table", &table, w)
        });
    }
    /// `ChangeDimensionTrigger`, and `ServerPlayer.enteredNetherPosition`: the
    /// Overworld position the player went into the Nether from (`at`) is kept
    /// for the way back, and any other trip drops it.
    pub(super) fn trigger_dimension(&mut self, from: Dim, to: Dim, at: [f64; 3]) {
        self.trigger_changed_dimension(from, to);
        if from == Dim::Overworld && to == Dim::Nether {
            self.adv.marks.nether = Some(at);
        } else if !(from == Dim::Nether && to == Dim::Overworld) {
            self.adv.marks.nether = None;
        }
    }
    /// `CriteriaTriggers.CHANGED_DIMENSION` alone, without the
    /// `enteredNetherPosition` bookkeeping of
    /// `ServerPlayer.triggerDimensionChangeTriggers`: a respawn after the
    /// credits triggers the criterion (`END` → `OVERWORLD`, whatever level
    /// the point is in) and leaves that position as it is.
    pub(super) fn trigger_changed_dimension(&mut self, from: Dim, to: Dim) {
        let (from_id, to_id) = (from.id().to_owned(), to.id().to_owned());
        self.trigger("minecraft:changed_dimension", move |c, _| {
            c["from"].as_str().is_none_or(|f| adv::full(f) == from_id)
                && c["to"].as_str().is_none_or(|t| adv::full(t) == to_id)
        });
    }
}
/// `Advancement.name`: the bracketed title in the frame's colour, with the
/// description on hover; the id when there is no display.
pub(super) fn adv_name(a: &adv::Advancement) -> Text {
    let Some(d) = &a.display else {
        return Text::literal(a.id.clone());
    };
    let color = frame_color(d.frame);
    let title = text_of(&d.title).with_style(&Style {
        hover: Some(std::sync::Arc::new(decorate_name(d))),
        ..Style::default()
    });
    Text::translate("chat.square_brackets", &[title]).with_color(color)
}
fn frame_color(frame: Frame) -> u32 {
    match frame {
        Frame::Challenge => text::DARK_PURPLE,
        _ => text::GREEN,
    }
}
/// `Advancement.decorateName`: a copy of the title with the frame's colour
/// under its own style (mergeStyles), then a new line and the description
/// after the title's own siblings, so both inherit the title's root style.
/// Built on the title's JSON (a list as its first element with the rest as
/// siblings): a parsed Text moves a styled root's style down a level once it
/// has siblings.
fn decorate_name(d: &adv::Display) -> Text {
    let mut component = match horizon_client::scoreboard::canonical_component(&d.title) {
        Value::String(s) => serde_json::json!({ "text": s }),
        c => c,
    };
    if let Some(o) = component.as_object_mut() {
        let frame = Text::empty().with_color(frame_color(d.frame)).component();
        o.entry("color").or_insert_with(|| frame["color"].clone());
        let extra = o.entry("extra").or_insert_with(|| serde_json::json!([]));
        if let Some(extra) = extra.as_array_mut() {
            extra.extend(["\n".into(), d.description.clone()]);
        }
    }
    text_of(&component).with_component(component)
}
impl Game<'_> {
    /// `AdvancementCommands.Action.GRANT` on one advancement (or one of
    /// its criteria): false when nothing changed.
    pub(super) fn grant_advancement(&mut self, index: usize, criterion: Option<&str>) -> bool {
        if self.lan_acting_fake {
            return false;
        }
        let tree = self.adv.tree.clone();
        let a = &tree.list[index];
        if criterion.is_none() && self.adv.progress.done(a) {
            return false;
        }
        let names: Vec<&str> = match criterion {
            Some(c) => vec![c],
            None => a.criteria.iter().map(|c| c.name.as_str()).collect(),
        };
        let date = now_text();
        let mut changed = false;
        for c in names {
            let was = self.adv.progress.done(a);
            if self.adv.progress.grant(&a.id, c, &date) {
                changed = true;
                self.advancement_progress(a, c, ProgressType::Grant);
                self.progress_changed(index);
                if !was && self.adv.progress.done(a) {
                    self.award(index);
                }
            }
        }
        changed || criterion.is_none()
    }
    /// `AdvancementCommands.Action.REVOKE`.
    pub(super) fn revoke_advancement(&mut self, index: usize, criterion: Option<&str>) -> bool {
        if self.lan_acting_fake {
            return false;
        }
        let tree = self.adv.tree.clone();
        let a = &tree.list[index];
        self.progress_changed(index);
        match criterion {
            Some(c) => self.revoke_criterion(a, c),
            None => {
                let mut any = false;
                for c in &a.criteria {
                    any |= self.revoke_criterion(a, &c.name);
                }
                any
            }
        }
    }
    /// `PlayerAdvancements.revoke(holder, criterion)`: the progress change and
    /// the hook for it.
    fn revoke_criterion(&mut self, advancement: &adv::Advancement, criterion: &str) -> bool {
        let revoked = self.adv.progress.revoke(&advancement.id, criterion);
        if revoked {
            self.advancement_progress(advancement, criterion, ProgressType::Revoke);
        }
        revoked
    }
}
/// An advancement's title or description: a whole component (ComponentSerialization.CODEC),
/// with its siblings and styles, as a plugin's `{"text":"","extra":[...]}` needs.
pub(super) fn text_of(v: &Value) -> Text {
    horizon_client::scoreboard::comp_text(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "daily"]
    fn helmet_wear_reports_the_old_stack_and_the_unclamped_damage() {
        const ID: &str = "testmod:wear";
        let scratch = std::env::temp_dir().join(format!("helmet-wear-{}", std::process::id()));
        let dir = scratch.join("data/testmod/advancement");
        std::fs::create_dir_all(&dir).unwrap();
        // An iron helmet has 165 points. At 164, a hit of 8 takes 2: damage 166, one past the
        // maximum, so durability -1, and a delta of -2 from the damage the stack had.
        std::fs::write(
            dir.join("wear.json"),
            r#"{
            "criteria": {
                "worn": {
                    "trigger": "minecraft:item_durability_changed",
                    "conditions": {
                        "durability": {"min": -1, "max": -1},
                        "delta": {"min": -2, "max": -2}
                    }
                }
            },
            "requirements": [["worn"]]
        }"#,
        )
        .unwrap();
        super::super::native_entity_refs::tests::with_game(|game| {
            game.adv.tree = Arc::new(Tree::load(&scratch.join("data")));
            let origin = horizon_core::geometry::Vec3::ZERO;
            game.player =
                Some(horizon_client::motion::PlayerMotion::new(game.motion, origin).unwrap());
            let mut helmet = ItemStack::new("minecraft:iron_helmet", 1);
            helmet.damage = 164;
            game.armor[0] = Some(helmet);
            let source =
                horizon_client::health::NO_ATTACK.player_source("minecraft:falling_anvil", 0);
            game.hurt_helmet(&source, 8.0);
            assert_eq!(game.armor[0], None);
            assert!(game.adv.progress.has(ID, "worn"));
        });
        std::fs::remove_dir_all(scratch).unwrap();
    }

    #[test]
    fn looking_at_triggers_are_the_ones_that_ask() {
        let criterion = |trigger: &str, conditions: serde_json::Value| adv::Criterion {
            name: "c".into(),
            trigger: trigger.into(),
            conditions: horizon_world::lazy_json::LazyJson::new(&conditions),
        };
        let looking = serde_json::json!({"player": {"type": "minecraft:entity_properties",
            "entity": "this", "predicate": {"minecraft:type_specific/player":
            {"looking_at": {"minecraft:entity_type": "minecraft:oak_boat"}}}}});
        let reference = serde_json::json!({"player": [{"condition": "minecraft:reference",
            "name": "pack:looks"}]});
        let plain = serde_json::json!({"player": {"type": "minecraft:entity_properties",
            "entity": "this", "predicate": {"minecraft:flags": {"is_sneaking": true}}}});
        let mut tree = Tree::default();
        tree.list.push(adv::Advancement {
            id: "pack:a".into(),
            parent: None,
            display: None,
            criteria: vec![
                criterion("minecraft:using_item", looking),
                criterion("minecraft:location", reference),
                criterion("minecraft:tick", plain),
            ],
            requirements: Vec::new(),
            rewards: Default::default(),
        });
        tree.index_looking_at();
        assert!(tree.may_look("minecraft:using_item"));
        assert!(tree.may_look("minecraft:location"));
        assert!(!tree.may_look("minecraft:tick"));
    }

    #[test]
    fn nbt_triggers_are_the_ones_that_ask() {
        let criterion = |trigger: &str, conditions: serde_json::Value| adv::Criterion {
            name: "c".into(),
            trigger: trigger.into(),
            conditions: horizon_world::lazy_json::LazyJson::new(&conditions),
        };
        let nbt = serde_json::json!({"entity": [{"condition": "minecraft:entity_properties",
            "entity": "this", "predicate": {"minecraft:nbt": "{Variant:1}"}}]});
        let reference = serde_json::json!({"player": [{"condition": "minecraft:reference",
            "name": "pack:saved"}]});
        let plain = serde_json::json!({"entity": [{"condition": "minecraft:entity_properties",
            "entity": "this", "predicate": {"minecraft:flags": {"is_baby": true}}}]});
        let mut tree = Tree::default();
        tree.index_nbt();
        assert!(!tree.tests_nbt());
        tree.list.push(adv::Advancement {
            id: "pack:a".into(),
            parent: None,
            display: None,
            criteria: vec![
                criterion("minecraft:player_interacted_with_entity", nbt),
                criterion("minecraft:location", reference),
                criterion("minecraft:tame_animal", plain),
            ],
            requirements: Vec::new(),
            rewards: Default::default(),
        });
        tree.index_nbt();
        assert!(tree.tests_nbt());
        assert!(tree.may_test_nbt("minecraft:player_interacted_with_entity"));
        assert!(tree.may_test_nbt("minecraft:location"));
        assert!(!tree.may_test_nbt("minecraft:tame_animal"));
    }

    #[test]
    fn titles_keep_their_siblings_and_styles() {
        let title = text_of(&serde_json::json!({
            "text": "",
            "extra": [{"text": "Quest ", "color": "gold"}, "Board"],
        }));
        assert_eq!(title.plain(), "Quest Board");
        assert_eq!(title.spans[0].style.color, Some(text::GOLD));
        assert_eq!(title.spans[1].style.color, None);
        let vanilla = text_of(&serde_json::json!({"translate": "advancements.story.root.title"}));
        assert_eq!(
            vanilla,
            Text::translate("advancements.story.root.title", &[])
        );
        assert_eq!(
            vanilla.translation_key(),
            Some("advancements.story.root.title")
        );
    }

    fn announced(title: Value, description: Value) -> adv::Advancement {
        adv::Advancement {
            id: "quests:board".into(),
            parent: None,
            display: Some(adv::Display {
                icon: horizon_client::items::ItemStack::new("minecraft:book", 1),
                title,
                description,
                frame: Frame::Task,
                background: None,
                show_toast: true,
                announce: true,
                hidden: false,
            }),
            criteria: Vec::new(),
            requirements: Vec::new(),
            rewards: adv::Rewards::default(),
        }
    }
    /// The chat hover: the JSON a client receives, and the spans drawn here.
    fn hover(a: &adv::Advancement) -> (Value, Text) {
        let local = decorate_name(a.display.as_ref().unwrap());
        let sent = adv_name(a).component()["with"][0]["hover_event"]["value"].clone();
        (sent, local)
    }

    #[test]
    fn chat_hover_keeps_the_titles_siblings_before_the_description() {
        let a = announced(
            serde_json::json!({"text": "", "extra": [{"text": "Quest ", "color": "gold"}, "Board"]}),
            serde_json::json!("Open the board"),
        );
        let (sent, local) = hover(&a);
        let sent = horizon_client::scoreboard::comp_text(&sent);
        assert_eq!(sent.plain(), "Quest Board\nOpen the board");
        let colors = |t: &Text| t.spans.iter().map(|s| s.style.color).collect::<Vec<_>>();
        let (gold, green) = (Some(text::GOLD), Some(text::GREEN));
        assert_eq!(colors(&sent), [gold, green, green, green]);
        // The spans drawn here come from the same JSON a client receives.
        assert_eq!(colors(&local), colors(&sent));
    }

    #[test]
    fn chat_hover_description_takes_a_styled_titles_own_style() {
        // mergeStyles keeps the title's gold; the description inherits it.
        let object = announced(
            serde_json::json!({"text": "Quest", "color": "gold", "bold": true, "extra": [" Board"]}),
            serde_json::json!("Open the board"),
        );
        let list = announced(
            serde_json::json!([{"text": "Quest", "color": "gold", "bold": true}, " Board"]),
            serde_json::json!("Open the board"),
        );
        for a in [object, list] {
            let (sent, local) = hover(&a);
            let sent = horizon_client::scoreboard::comp_text(&sent);
            assert_eq!(sent.plain(), "Quest Board\nOpen the board");
            let last = &sent.spans.last().unwrap().style;
            assert_eq!((last.color, last.bold), (Some(text::GOLD), Some(true)));
            assert_eq!(local.spans.last().unwrap().style, *last);
        }
    }

    #[test]
    fn chat_hover_of_a_vanilla_advancement() {
        let a = announced(
            serde_json::json!({"translate": "advancements.story.root.title"}),
            serde_json::json!({"translate": "advancements.story.root.description"}),
        );
        assert_eq!(
            hover(&a).0,
            serde_json::json!({
                "translate": "advancements.story.root.title",
                "color": "green",
                "extra": ["\n", {"translate": "advancements.story.root.description"}],
            })
        );
    }

}
#[cfg(test)]
mod structure_tag_tests {
    use super::View;
    use crate::game::{Audio, Game, Options, Progress, Settings, load_resources};
    use horizon_client::advancements::{World, id_matches};
    use serde_json::json;

    /// `LocationPredicate.structures` names a `#tag`: the game's own
    /// `worldgen/structure` tags, and with a world's data packs theirs over
    /// the game's.
    #[test]
    #[ignore = "daily"]
    fn location_structure_tags_hold_their_structures() {
        let root = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").unwrap());
        let settings = Settings::default();
        let loaded = load_resources(
            horizon_assets::resource_source::ResourceRoot::directory(root.clone()),
            Vec::new(),
            None,
            &root,
            &settings,
            std::time::Instant::now(),
            crate::game::execution_role::ExecutionRole::DedicatedServer,
            &Progress::new(),
        )
        .unwrap();
        let mut options = Options::parse(std::iter::empty()).unwrap().unwrap();
        options.resources = root.clone();
        options.execution_role = crate::game::execution_role::ExecutionRole::DedicatedServer;
        let mut game = Game::new(
            &loaded.resources,
            loaded.pack,
            options,
            settings,
        )
        .unwrap();
        let set = |s: &str| json!(s);
        let holds = |game: &Game, tag: &str, id: &str| {
            id_matches(&set(tag), "worldgen/structure", id, &View(game))
        };
        // A world without packs: the game's own tags.
        assert!(game.world_tables.is_none());
        assert!(holds(
            &game,
            "#minecraft:village",
            "minecraft:village_taiga"
        ));
        assert!(holds(&game, "#shipwreck", "minecraft:shipwreck_beached"));
        assert!(!holds(&game, "#minecraft:village", "minecraft:shipwreck"));
        assert!(!View(&game).in_tag("worldgen/structure", "mypack:village", "minecraft:igloo"));

        // A pack's own tag (nesting the game's) and an addition to the game's.
        let dir =
            std::env::temp_dir().join(format!("horizon-structure-tags-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let pack = dir.join("datapacks").join("mypack");
        let tags = pack.join("data/mypack/tags/worldgen/structure");
        let game_tags = pack.join("data/minecraft/tags/worldgen/structure");
        std::fs::create_dir_all(&tags).unwrap();
        std::fs::create_dir_all(&game_tags).unwrap();
        std::fs::write(
            pack.join("pack.mcmeta"),
            r#"{"pack":{"description":"fixture"}}"#,
        )
        .unwrap();
        std::fs::write(
            tags.join("village.json"),
            r##"{"values": ["#minecraft:village", "minecraft:igloo"]}"##,
        )
        .unwrap();
        std::fs::write(
            game_tags.join("shipwreck.json"),
            r#"{"values": ["minecraft:ocean_ruin_cold"]}"#,
        )
        .unwrap();
        let settings = json!({"enabled": ["file/mypack"], "disabled": []});
        std::fs::write(dir.join("datapacks.json"), settings.to_string()).unwrap();
        game.world_root = dir.clone();
        let stack = crate::game::functions::pack_stack(
            &dir,
            &root,
            &horizon_assets::resource_source::ResourceSource::directory(root.clone()),
            &Default::default(),
            false,
            &[],
            &[],
        );
        let (tables, _, _) = game.stage_world_tables(&stack, false, &root).unwrap();
        game.world_tables = tables;
        assert!(game.world_tables.is_some());
        assert!(holds(&game, "#mypack:village", "minecraft:igloo"));
        assert!(holds(&game, "#mypack:village", "minecraft:village_plains"));
        assert!(!holds(&game, "#mypack:village", "minecraft:shipwreck"));
        assert!(holds(
            &game,
            "#minecraft:shipwreck",
            "minecraft:ocean_ruin_cold"
        ));
        assert!(holds(&game, "#minecraft:shipwreck", "minecraft:shipwreck"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod random_chance_command_tests {
    use super::*;
    use crate::game::commands::Target;
    use horizon_core::nbt::Tag;
    #[test]
    #[ignore = "daily"]
    fn registered_named_random_chance_uses_real_source_and_world_draws() {
        use crate::game::{Audio, Options, Settings, load_resources};
        use std::time::Instant;
        let root = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").unwrap());
        let settings = Settings::default();
        let loaded = load_resources(
            horizon_assets::resource_source::ResourceRoot::directory(root.clone()),
            Vec::new(),
            None,
            &root,
            &settings,
            Instant::now(),
            crate::game::execution_role::ExecutionRole::DedicatedServer,
            &crate::game::Progress::new(),
        )
        .unwrap();
        let mut options = Options::parse(std::iter::empty()).unwrap().unwrap();
        options.resources = root.clone();
        options.execution_role = crate::game::execution_role::ExecutionRole::DedicatedServer;
        let mut game = Game::new(
            &loaded.resources,
            loaded.pack,
            options,
            settings,
        )
        .unwrap();
        game.build_registries();
        game.set_allow_commands(true);

        let dir = std::env::temp_dir().join(format!("horizon-h122-unit-{}", std::process::id()));
        assert!(!dir.exists());
        let pack = dir.join("datapacks/chance");
        let folder = pack.join("data/h122/predicate");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(pack.join("pack.mcmeta"),r#"{"pack":{"description":"Chance","pack_format":121,"min_format":[121,0],"max_format":[121,0]}}"#).unwrap();
        let function = pack.join("data/h122/function");
        std::fs::create_dir_all(&function).unwrap();
        std::fs::write(
            function.join("admit.mcfunction"),
            "scoreboard players set pack h122 17\n",
        )
        .unwrap();
        let rows: Vec<Value> =
            include_str!("../../../../../../docs/parity/random-chance-seeded.jsonl")
                .lines()
                .map(|s| serde_json::from_str(s).unwrap())
                .collect();
        for row in &rows {
            std::fs::write(
                folder.join(format!("case_{}.json", row["case"])),
                row["input"].to_string(),
            )
            .unwrap();
        }
        std::fs::write(folder.join("pig.json"),r#"{"type":"minecraft:entity_properties","entity":"this","predicate":{"minecraft:entity_type":"minecraft:pig"}}"#).unwrap();
        game.world_root = dir.clone();
        game.reload_datapacks(false, &[]).unwrap();
        assert!(game.run_command("scoreboard objectives add h122 dummy"));
        assert!(game.run_command("function h122:admit"));
        assert_eq!(game.score.board.score("pack", "h122").unwrap().value, 17);
        let tag = Tag::compound([
            ("id", Tag::String("minecraft:pig".into())),
            ("UUID", Tag::IntArray(vec![122, 0, 0, 1])),
            (
                "Pos",
                Tag::List(vec![
                    Tag::Double(4.5_f64.to_bits()),
                    Tag::Double(200.0_f64.to_bits()),
                    Tag::Double(4.5_f64.to_bits()),
                ]),
            ),
            ("Health", Tag::Float(10_f32.to_bits())),
            ("NoAI", Tag::Byte(1)),
            ("NoGravity", Tag::Byte(1)),
        ]);
        let id = game.load_mob(tag, None).unwrap();
        assert_eq!(game.persist.uuids.get(&id), Some(&[122, 0, 0, 1]));
        assert_eq!(game.entity_view_of(Target::Mob(id)).kind, "minecraft:pig");
        let clock = game.world.game_time;
        assert!(game.run_command("execute as 0000007a-0000-0000-0000-000000000001 if predicate h122:pig run scoreboard players set identity h122 1"));
        assert_eq!(game.score.board.score("identity", "h122").unwrap().value, 1);
        for row in &rows {
            let seed = row["seed"].as_i64().unwrap();
            let name = format!("h122:case_{}", row["case"]);
            assert!(game.fns.store.predicates.contains_key(&name));
            game.world.random.set_seed(seed);
            game.run_command("scoreboard players set branch h122 -99");
            game.exec = Default::default();
            let source = game.player_source();
            game.execute_line(&format!("execute as 0000007a-0000-0000-0000-000000000001 if predicate {name} run scoreboard players set branch h122 1"),source).unwrap();
            assert_eq!(
                game.score.board.score("branch", "h122").unwrap().value,
                if row["result"].as_bool().unwrap() {
                    1
                } else {
                    -99
                },
                "{row}"
            );
            let continuation: Vec<i64> = (0..4).map(|_| game.world.random.next_long()).collect();
            let expected: Vec<i64> = row["continuation"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_i64().unwrap())
                .collect();
            assert_eq!(continuation, expected, "actual world RNG {row}");
        }
        assert_eq!(game.world.game_time, clock);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg(test)]
mod custom_data_equipment_tests {
    use super::*;
    use horizon_client::{mobs::finalize, save};
    use horizon_core::nbt::{Tag, parse_snbt};
    use horizon_core::position::BlockPos;

}

/// A player's 41 slots in `Inventory` numbering, for
/// `native_player_inventory_changed`: the main inventory (0..36), the armour
/// feet to head (36..40; `armor` holds it head first), the off hand (40).
pub(super) fn inventory_slots(
    inventory: &[Option<ItemStack>],
    armor: &[Option<ItemStack>; 4],
    offhand: &Option<ItemStack>,
) -> Vec<Option<ItemStack>> {
    let mut slots: Vec<Option<ItemStack>> = inventory.iter().take(36).cloned().collect();
    slots.resize(36, None);
    slots.extend(armor.iter().rev().cloned());
    slots.push(offhand.clone());
    slots
}
