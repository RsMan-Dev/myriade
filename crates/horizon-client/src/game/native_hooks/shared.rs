#[path = "shared/book_screen.rs"]
mod book_screen;
#[path = "shared/item_fallthrough.rs"]
mod item_fallthrough;
#[path = "shared/loot.rs"]
mod loot;

use crate::game::api_values::{TryIntoApi, TryIntoEngine};
use horizon_client::containers::ModBlockEntity as EngineBlockEntity;
use screen_chars::chars_to_type;

struct ContentsChangedGuard<'a, 'game> {
    game: &'a mut Game<'game>,
}

impl Drop for ContentsChangedGuard<'_, '_> {
    fn drop(&mut self) {
        self.game.contents_changing.pop();
    }
}

/// `Level.setBlock`'s flags, for the `native_set_block_property` helpers:
/// `flags::UPDATE_CLIENTS` as a vanilla crop grows, `flags::UPDATE_ALL` as a
/// furnace sets its `lit`.
pub(super) use horizon_world::level::flags;

/// A mod's camera (`native_camera_pose`): each value that isn't finite keeps vanilla's from `view`.
fn posed_camera(
    view: (glam::DVec3, f32, f32),
    position: [f64; 3],
    yaw: f32,
    pitch: f32,
) -> (glam::DVec3, f32, f32) {
    let position = std::array::from_fn(|i| {
        if position[i].is_finite() {
            position[i]
        } else {
            view.0[i]
        }
    });
    (
        glam::DVec3::from_array(position),
        if yaw.is_finite() { yaw } else { view.1 },
        if pitch.is_finite() { pitch } else { view.2 },
    )
}

/// A mod's camera roll (`native_camera_roll`), in degrees, within a turn (-360 to 360): one that
/// isn't finite is none. Every reader (the view, the billboards, the name tags) then turns by
/// the same small angle, where a huge one would round differently in each. Neither bounded nor
/// scaled by Distortion Effects, unlike a shake: it is the camera's pose.
fn posed_roll(roll: f32) -> f32 {
    let turn = if roll.is_finite() { roll % 360.0 } else { 0.0 };
    // -0 (a whole number of turns the other way) is no roll, as 0.
    if turn == 0.0 { 0.0 } else { turn }
}

/// The largest turn a shake gives, in degrees, on each axis.
const SHAKE_MAX_DEGREES: f32 = 10.0;
/// The largest offset a shake gives, in blocks, along each axis.
const SHAKE_MAX_OFFSET: f32 = 0.5;

use camera_pose::bounded_shake;
pub(super) use camera_pose::hand_pose_matrix;

/// The pose a bounded shake multiplies into the projection, after the view
/// bobbing: the offset, then roll, pitch and yaw.
fn shake_pose(shake: Shake) -> glam::Mat4 {
    let [x, y, z] = shake.offset;
    glam::Mat4::from_translation(glam::Vec3::new(-x, -y, -z))
        * glam::Mat4::from_rotation_z(shake.roll.to_radians())
        * glam::Mat4::from_rotation_x(shake.pitch.to_radians())
        * glam::Mat4::from_rotation_y(shake.yaw.to_radians())
}

/// The mod screen open on this client: the block it is for, the key its
/// slots are under, the window id of its menu, the menu's data slots as
/// the server sent them, and what the server sent before the menu opened
/// (`horizon:menu_extra`), when it did.
pub(super) struct OpenNativeScreen {
    pub(super) layout_revision: u64,
    key: Pos,
    window: i32,
    block: String,
    screen: mod_api::Screen,
    data: Vec<i32>,
    extra: Option<horizon_protocol::menu_extra::Message>,
}

impl OpenNativeScreen {
    /// The block the menu is of: where the server said, else the clicked
    /// block its slots are under (a server without `horizon:menu_extra`).
    fn pos(&self) -> Option<[i32; 3]> {
        match &self.extra {
            Some(extra) => extra.pos,
            None if self.key.0 != i32::MIN => Some([self.key.0, self.key.1, self.key.2]),
            None => None,
        }
    }
    /// Data slot `id` of window `window`'s menu: false for another window's
    /// (an old or replaced one), which this screen leaves alone.
    fn set_data(&mut self, window: i32, id: i16, value: i16) -> bool {
        if window != self.window {
            return false;
        }
        // A menu's data slot ids are small; a bad one is dropped.
        if let Ok(i @ 0..256) = usize::try_from(id) {
            if self.data.len() <= i {
                self.data.resize(i + 1, 0);
            }
            self.data[i] = i32::from(value);
        }
        true
    }
}

/// The player as a native mod reads it.
#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub struct NativePlayer {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    /// Degrees; 0 faces south, 90 west (`Direction.fromYRot`).
    pub yaw: f32,
    pub pitch: f32,
    /// `minecraft:overworld`, `minecraft:the_nether`, `minecraft:the_end`.
    pub dimension: &'static str,
}

/// How the panel lays out a recipe of `native_panel_recipes`.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativeRecipeLayout {
    /// A crafting grid of `width` x `height` slots, an arrow and the result.
    Grid,
    /// One input over a flame, an arrow and the result (furnaces, smokers,
    /// campfires).
    Cooking,
    /// One input, an arrow and the result (the stonecutter).
    Single,
    /// Template, base and addition, an arrow and the result.
    Smithing,
    /// A potion and an ingredient, an arrow and the result.
    Brewing,
}

/// One result of a recipe the panel shows (`native_panel_recipes`): a slot
/// cycling through `stacks`, one a second, and the chance in percent the recipe
/// gives it (`Some(25.0)` draws "25%"); `None` gives it every time.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NativeRecipeResult {
    pub(crate) stacks: Vec<mod_api::Stack>,
    pub(crate) chance: Option<f32>,
}

/// A recipe the panel shows (`native_panel_recipes`). Each slot, and each
/// result, cycles through its stacks, one a second; an empty slot shows
/// nothing. A crafting recipe has one result and no chance.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NativeRecipeView {
    pub(crate) layout: NativeRecipeLayout,
    /// The grid's size (`Grid` only), at most 3 x 3; slots past it continue
    /// in rows under it, 27 in all.
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) slots: Vec<Vec<mod_api::Stack>>,
    /// Up to four, left to right after the arrow.
    pub(crate) results: Vec<NativeRecipeResult>,
    /// Fluids the recipe takes and gives, `amount` in millibuckets, each in
    /// a well with its amount under it; at most eight each. A well shows the
    /// fluid's client look (`data/<mod>/horizon/fluid/`: sprite and tint), a
    /// flat square of vanilla's colour for water and lava and else grey when
    /// the fluid has none.
    pub(crate) fluid_inputs: Vec<FluidStack>,
    pub(crate) fluid_outputs: Vec<FluidStack>,
    /// A short line under the recipe (a cooking time), or "".
    pub(crate) note: String,
    /// The action button beside it: none, enabled, or shown greyed.
    pub(crate) action: Option<bool>,
}

/// An item of [`NativeRecipes::items`], by its index there, with a count
/// and the components it has beyond the item's own, in vanilla's item
/// syntax (`[minecraft:potion_contents={potion:"minecraft:swiftness"}]`),
/// or "".
#[allow(dead_code)]
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct NativeRecipeStack {
    pub(crate) item: u32,
    pub(crate) count: u32,
    pub(crate) components: String,
}

/// The station a recipe is made at.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativeRecipeKind {
    CraftingShaped,
    CraftingShapeless,
    Smelting,
    Blasting,
    Smoking,
    Campfire,
    Stonecutting,
    Smithing,
    Brewing,
}

/// A recipe of [`NativeRecipes`].
#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NativeRecipe {
    /// `minecraft:oak_planks`.
    pub(crate) id: String,
    pub(crate) kind: NativeRecipeKind,
    /// The recipe book's tab (`crafting_building_blocks`, `furnace_food`),
    /// or "".
    pub(crate) category: String,
    /// Recipes the book shows on one button share a group, or "".
    pub(crate) group: String,
    /// A shaped recipe's grid: `slots` holds its cells row by row. 0 for
    /// the other kinds.
    pub(crate) width: u32,
    pub(crate) height: u32,
    /// Each slot's stacks, any one of which fits; an empty list is an empty
    /// cell. Cooking and stonecutting: the input. Smithing: the template,
    /// the base, the addition. Brewing: the bottle, the reagent.
    pub(crate) slots: Vec<Vec<NativeRecipeStack>>,
    /// What it makes; several when that varies (a trim on any armour).
    pub(crate) results: Vec<NativeRecipeStack>,
    /// Cooking: game ticks and experience; 0 for the other kinds.
    pub(crate) cooking_time: u32,
    pub(crate) experience: f32,
    /// The recipe book's id for it (one per way the book shows it), as
    /// `known_recipes` lists them; -1 for the kinds the book doesn't
    /// show (campfire, stonecutting, smithing, brewing).
    pub(crate) book_id: i32,
}

/// An item of [`NativeRecipes::items`].
#[allow(dead_code)]
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct NativeItem {
    /// `minecraft:diamond`, `<mod>:<path>`.
    pub(crate) id: String,
    /// In the player's language.
    pub(crate) name: String,
    /// Its item tags, without the `#`.
    pub(crate) tags: Vec<String>,
}

/// Every item and recipe the game knows (`native_recipes`): `items` in the
/// registry's order, which each [`NativeRecipeStack::item`] indexes; the
/// recipes are the book's in its order, then campfire, stonecutting,
/// smithing and brewing.
#[allow(dead_code)]
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct NativeRecipes {
    pub(crate) items: Vec<NativeItem>,
    pub(crate) recipes: Vec<NativeRecipe>,
}

/// The mappings `mapping` answers for, with the key their default
/// binding is (none: a mouse button, attack or use).
const MAPPINGS: [(&str, Option<KeyCode>); 9] = [
    ("key.forward", Some(KeyCode::KeyW)),
    ("key.left", Some(KeyCode::KeyA)),
    ("key.back", Some(KeyCode::KeyS)),
    ("key.right", Some(KeyCode::KeyD)),
    ("key.jump", Some(KeyCode::Space)),
    ("key.sneak", Some(KeyCode::ShiftLeft)),
    ("key.sprint", Some(KeyCode::ControlLeft)),
    ("key.attack", None),
    ("key.use", None),
];

/// The Post a mouse input owes once the screen has handled it, built when
/// its Pre let the input through (`Game::screen_mouse_post`).
pub(super) enum ScreenMousePost {
    Button { button: i32, pressed: bool },
    Scroll([f64; 2]),
    Drag { button: i32, delta: [f32; 2] },
}

/// What a drag's Pre decided (`Game::screen_mouse_dragged`).
pub(super) enum ScreenMousePre {
    /// No screen hook ran: no drag, a synthetic move or no screen open.
    NotFired,
    Canceled,
    Passed(ScreenMousePost),
}

/// What a `ScreenEvent.CharacterTyped.Pre` leaves of `text`, given `canceled`
/// (whether a hook cancelled character `c`): the cancelled printable
/// characters are gone and every other character stays, control characters
/// included — a text box reads Backspace, Delete or Escape as keys and vanilla
/// fires no `charTyped` for them, so they are never offered to the hook.
/// `None` when nothing is left to type.
fn screen_text_hooked(text: &str, mut canceled: impl FnMut(char) -> bool) -> Option<String> {
    let mut left = String::with_capacity(text.len());
    for c in text.chars() {
        if !c.is_control() && canceled(c) {
            continue;
        }
        left.push(c);
    }
    if left.is_empty() && !text.is_empty() {
        return None;
    }
    Some(left)
}

/// The hook points. Each does nothing in Horizon.
impl Game<'_> {
    /// The single admission point for all toast families. Producers decide the
    /// candidate first; only an accepted candidate is applied to its queue.
    pub(super) fn add_toast(
        &self,
        kind: mod_api::ToastKind,
        title: &mut Text,
        description: &mut Option<Text>,
    ) -> bool {
        // Shared advancement/recipe producers also run on logical servers;
        // only their receiving client owns toast presentation and its hook.
        if self.is_logical_server() {
            return false;
        }
        let mut cancel = false;
        self.native_toast_add(kind, title, description, &mut cancel);
        !cancel
    }

    pub(super) fn add_system_toast(
        &self,
        mut toast: horizon_client::toasts::SystemToast,
    ) -> Option<horizon_client::toasts::SystemToast> {
        self.add_toast(
            mod_api::ToastKind::System,
            &mut toast.title,
            &mut toast.message,
        )
        .then_some(toast)
    }

    /// The milliseconds a client tick lasts at this frame's clock advance
    /// (`Minecraft.getTickTargetMillis`), as a mod's `tick_target_millis` leaves it. An answer
    /// that is not a finite number above zero keeps the clock's own.
    pub(super) fn tick_target_millis(&self) -> f32 {
        let vanilla = self.clock.tick_target_millis();
        let mut millis = vanilla;
        self.native_tick_target_millis(&mut millis);
        if millis.is_finite() && millis > 0.0 {
            millis
        } else {
            vanilla
        }
    }
    /// The player-health HUD rows, once a frame the HUD draws them
    /// (`HealthHud::layout`, not under F1): `inputs` carries the health,
    /// food, air and vehicle state the layout reads, `wi`/`hi` the screen's
    /// GUI size. A mod that sets `draws` replaces every row — armour,
    /// hearts, food and air are the answer's to give; `bubble_pop_sound`
    /// too.
    /// Nothing set draws Horizon's own.
    /// The hearts the local player's health bar draws (`Gui.HeartType.forPlayer`, then
    /// NeoForge's `PlayerHeartTypeEvent`): poisoned, else withered, else frozen when fully
    /// frozen, else normal, as a mod's `player_heart_type` leaves it. None without a player.
    pub(super) fn player_heart_kind(&mut self) -> Option<mod_api::client::hud_layout::HeartKind> {
        use mod_api::client::hud_layout::HeartKind;
        let p = self.player.as_ref()?;
        let effects = &self.potions.effects;
        let mut kind = if effects.get("poison").is_some() {
            HeartKind::Poisoned
        } else if effects.get("wither").is_some() {
            HeartKind::Withered
        } else if p.is_fully_frozen() {
            HeartKind::Frozen
        } else {
            HeartKind::Normal
        };
        self.native_player_heart_type(&mut kind);
        Some(kind)
    }
    /// Whether a mod cancelled `packet` on its way to the server (`client_packet_send`).
    /// Builds the packet's view only when a mod listens.
    pub(super) fn client_packet_send_canceled(
        &self,
        packet: &horizon_protocol::packets::play::Serverbound,
    ) -> bool {
        if !mod_api::__private::listened(mod_api::HookId::ClientPacketSend) {
            return false;
        }
        let mut canceled = false;
        self.native_client_packet_send(&serverbound_packet_view(packet), &mut canceled);
        canceled
    }
    /// The living entity an effect decision names: `Some(id)` a mob's
    /// `Entity.getId`, `None` the acting player. `None` when no native
    /// world runs, and the decision falls back to vanilla's.
    pub(super) fn effect_living_ref(&self, entity: Option<i32>) -> Option<LivingRef> {
        let epoch = self.native_lifecycle.epoch()?;
        let id = match entity {
            Some(id) => id as u32,
            None => self.own_entity_id() as u32,
        };
        Some(LivingRef::new(EntityRef::new(epoch, id)))
    }
    /// Deliver what the batch just ticked recorded, in order: `minecart_pass`
    /// to the parts, and the writes a landing or a mid-walk `entity_inside`
    /// contact asked for to the level. A write is skipped when the block at
    /// its position is no longer the state it touched, as vanilla never sees
    /// a stale one. The parts' own `entity_inside` contacts fire mid-walk now,
    /// on the shared inside sweep, not here.
    pub(super) fn drain_entity_inside(&mut self) {
        for contact in horizon_client::block_contact::take_pending() {
            let (state, pos) = match &contact {
                Pending::Landed(landed) => (landed.state, landed.position),
                Pending::MinecartPass(pass) => (pass.state, pass.position),
                Pending::InsideWritten(written) => (written.state, written.position),
            };
            let Some(level) = self.level(self.dim.id()) else {
                continue;
            };
            if self.level_block_state(level, [pos.x, pos.y, pos.z]) != Some(state) {
                continue;
            }
            match contact {
                Pending::MinecartPass(pass) => {
                    if let Some(entity) = u32::try_from(pass.entity)
                        .ok()
                        .and_then(|id| self.entity(id))
                    {
                        self.native_minecart_pass(state, pos, entity);
                    }
                }
                Pending::Landed(landed) => {
                    for write in landed.writes {
                        self.apply_landing_write(level, pos, write);
                    }
                }
                // The mid-walk contacts' own level writes, guarded on the touched
                // state above, as a landing's are.
                Pending::InsideWritten(written) => {
                    for write in written.writes {
                        self.apply_inside_write(level, pos, write);
                    }
                }
            }
        }
    }

    /// Apply one already-validated engine write, at the block the entity landed on.
    /// The original-state guard runs once before the ordered batch, not between writes.
    fn apply_landing_write(
        &mut self,
        level: mod_api::LevelRef,
        pos: BlockPos,
        write: horizon_world::level::LandingWrite,
    ) {
        let at = [pos.x, pos.y, pos.z];
        match write {
            // `Level.setBlockAndUpdate`: flag 3, the neighbours and the clients.
            horizon_world::level::LandingWrite::SetBlock(state) => {
                let flags = BlockUpdateFlags::NEIGHBORS | BlockUpdateFlags::CLIENTS;
                let _ = self.level_set_block(level, at, state, flags);
            }
            horizon_world::level::LandingWrite::DestroyBlock { drop } => {
                self.level_destroy_block(level, at, drop, None, false);
            }
            // A write a later API adds that this game does not know is not applied.
            _ => {}
        }
    }
    /// A write an `entity_inside` part asked for, applied as a landing write is: at the
    /// touched block, once the entity's batch has ticked.
    fn apply_inside_write(
        &mut self,
        level: mod_api::LevelRef,
        pos: BlockPos,
        write: horizon_world::level::InsideWrite,
    ) {
        let at = [pos.x, pos.y, pos.z];
        match write {
            horizon_world::level::InsideWrite::SetBlock(state) => {
                let flags = BlockUpdateFlags::NEIGHBORS | BlockUpdateFlags::CLIENTS;
                let _ = self.level_set_block(level, at, state, flags);
            }
            horizon_world::level::InsideWrite::DestroyBlock { drop } => {
                self.level_destroy_block(level, at, drop, None, false);
            }
            // A write a later API adds that this game does not know is not applied.
            _ => {}
        }
    }
}

/// `native_living_use_totem` for the damage code: whether it
/// canceled the totem. The entity and the source are named as the damage hook names them.
pub(super) fn use_totem(level: DamageLevel<'_>, hurting: Hurting<'_>, off_hand: bool) -> bool {
    if !mod_api::__private::listened(mod_api::HookId::LivingUseTotem) {
        return false;
    }
    let (entity, kind, source) = hurt_server_args(hurting, level.attack());
    let hand = if off_hand { Hand::Off } else { Hand::Main };
    let mut cancel = false;
    HitLevel(level).native_living_use_totem(
        entity,
        kind,
        &source,
        hand,
        "minecraft:totem_of_undying",
        &mut cancel,
    );
    cancel
}

/// The level a hit or a landing happens in, as its hooks' level host: its blocks, its random and its
/// own attachments. Not `GameLevel`, which a hit can't build: a mob's hit lands in the mob
/// tick, where the world is read by every mob and its random is lent out, and building one
/// per hit would allocate its entity boxes.
pub(super) struct HitLevel<'a>(pub(super) DamageLevel<'a>);

/// Let listeners override the vanilla invulnerability decision for this hit.
pub(super) fn invulnerability_check(
    level: DamageLevel<'_>,
    hurting: Hurting<'_>,
    vanilla: bool,
) -> bool {
    if !mod_api::__private::listened(mod_api::HookId::EntityInvulnerabilityCheck) {
        return vanilla;
    }
    let (entity, kind, source) = hurt_server_args(hurting, level.attack());
    let mut invulnerable = vanilla;
    HitLevel(level).native_entity_invulnerability_check(
        entity,
        kind,
        &source,
        vanilla,
        &mut invulnerable,
    );
    invulnerable
}

/// `native_living_incoming_damage` for the damage code: whether it
/// canceled the hit.
pub(super) fn incoming_damage(
    level: DamageLevel<'_>,
    hurting: Hurting<'_>,
    victim: Victim,
    container: &mut DamageContainer,
) -> bool {
    if !mod_api::__private::listened(mod_api::HookId::LivingIncomingDamage) {
        return false;
    }
    let (entity, kind, source) = hurt_server_args(hurting, level.attack());
    let mut canceled = false;
    let attack = level.attack();
    let Ok(weapon) = attack.weapon_of(source.direct).try_into_api() else {
        return false;
    };
    let mut snapshot = (*container).try_into_api().expect("scalar damage snapshot");
    HitLevel(level).native_living_incoming_damage(mod_api::__private::LivingIncomingDamageCall {
        entity,
        kind,
        source: &source,
        victim: &victim,
        weapon: weapon.as_ref(),
        container: &mut snapshot,
        canceled: &mut canceled,
    });
    *container = snapshot
        .try_into_engine(&())
        .expect("scalar damage snapshot");
    canceled
}

/// `native_living_damage_pre` for the damage code.
pub(super) fn damage_pre(
    level: DamageLevel<'_>,
    hurting: Hurting<'_>,
    container: &mut DamageContainer,
) {
    if !mod_api::__private::listened(mod_api::HookId::LivingDamagePre) {
        return;
    }
    let (entity, kind, source) = hurt_server_args(hurting, level.attack());
    let mut snapshot = (*container).try_into_api().expect("scalar damage snapshot");
    HitLevel(level).native_living_damage_pre(entity, kind, &source, &mut snapshot);
    *container = snapshot
        .try_into_engine(&())
        .expect("scalar damage snapshot");
}

/// `native_living_damage_post` for the damage code.
pub(super) fn damage_post(
    level: DamageLevel<'_>,
    hurting: Hurting<'_>,
    container: &DamageContainer,
) {
    if !mod_api::__private::listened(mod_api::HookId::LivingDamagePost) {
        return;
    }
    let (entity, kind, source) = hurt_server_args(hurting, level.attack());
    let snapshot = (*container).try_into_api().expect("scalar damage snapshot");
    HitLevel(level).native_living_damage_post(entity, kind, &source, &snapshot);
}

/// `native_living_shield_block` for the damage code. Its one
/// caller. The hurt entity and the source are named as the damage hook names them.
pub(super) fn shield_block(
    level: DamageLevel<'_>,
    hurting: Hurting<'_>,
    damage: f32,
    original_blocked: bool,
    block: &mut ShieldBlock,
) {
    let (entity, kind, source) = hurt_server_args(hurting, level.attack());
    HitLevel(level).native_living_shield_block(
        entity,
        kind,
        &source,
        damage,
        original_blocked,
        block,
    );
}

impl Game<'_> {
    /// The [`Attack`] of `attacker` (an entity id, 0 for none) for the level a hit lands in
    /// ([`DamageLevel::attacked_by`]). While a native mod listens it also records what the
    /// attacker holds (the player's selected and off-hand stacks, a mob's main hand) for
    /// `living_knockback` and the weapon of `living_incoming_damage`; without one it is the
    /// id alone, two `None`s and no allocation. Taken before the level's borrows of the game.
    pub(super) fn attack_of(&self, attacker: i32) -> Attack {
        let held = if mod_api::__private::mods().is_empty() {
            [None, None]
        } else if self
            .player
            .as_ref()
            .is_some_and(|p| p.entity_id == attacker)
        {
            [self.stack_in(self.selected).cloned(), self.offhand.clone()]
        } else if let Some(hand) = self.mob_main_hand(attacker) {
            [Some(hand), None]
        } else if let Some(guest) = self
            .lan
            .as_ref()
            .and_then(|host| host.hub.guests.iter().find(|g| g.entity_id == attacker))
        {
            [
                guest.inventory.get(guest.selected).cloned().flatten(),
                guest.offhand.clone(),
            ]
        } else {
            [None, None]
        };
        let player_uuid = if self
            .player
            .as_ref()
            .is_some_and(|p| p.entity_id == attacker)
        {
            self.owner_damage_uuid()
        } else {
            self.lan
                .as_ref()
                .and_then(|host| host.hub.guests.iter().find(|g| g.entity_id == attacker))
                .map(|g| horizon_client::lan::playerdata::uuid_ints(g.uuid))
        };
        Attack {
            attacker,
            held,
            projectile: None,
            projectile_is_arrow: false,
            direct: None,
            position: None,
            projectile_weapon: None,
            player_uuid,
        }
    }
    /// [`Self::attack_of`] for the attacker a damage source names: the player, a mob, or (none)
    /// the attack the enclosing [`Self::with_attack`] lends.
    pub(super) fn attack_of_who(&self, attacker: Option<Who>) -> Attack {
        match attacker {
            Some(Who::Player) => {
                let mut attack = self.attack_of(self.player.as_ref().map_or(0, |p| p.entity_id));
                attack.player_uuid = self.owner_damage_uuid();
                attack
            }
            Some(Who::Mob(id)) => self.attack_of(id as i32),
            None => self.current_attack(),
        }
    }
}

/// `native_living_knockback` for the knockback code: whether it
/// canceled the push. Its one caller. The pushed entity and the source are named as the
/// damage hook names them.
pub(super) fn knockback(level: DamageLevel<'_>, pushed: Hurting<'_>, knock: &mut Knock) -> bool {
    if !mod_api::__private::listened(mod_api::HookId::LivingKnockback) {
        return false;
    }
    let attack = level.attack();
    let attacker = match &pushed {
        Hurting::Mob(mob, damage) => match damage.attacker.or(mob.ai.last_hurt_by) {
            Some(Who::Mob(id)) => id as i32,
            _ => attack.attacker,
        },
        Hurting::Player(_, source)
        | Hurting::Dragon(_, source)
        | Hurting::ArmorStand(_, source)
        | Hurting::Entity(_, _, source) => match source.causing {
            0 => attack.attacker,
            causing => causing,
        },
    };
    let (entity, kind, source) = hurt_server_args(pushed, level.attack());
    let mut canceled = false;
    let [main, off] = &attack.held;
    let (Ok(main), Ok(off)) = (main.try_into_api(), off.try_into_api()) else {
        return false;
    };
    let entry = level.knockback_entry();
    HitLevel(level).native_living_knockback(mod_api::__private::LivingKnockbackCall {
        entity,
        kind,
        source: &source,
        attacker,
        attacker_main: main.as_ref(),
        attacker_off: off.as_ref(),
        entry,
        strength: &mut knock.strength,
        x: &mut knock.x,
        z: &mut knock.z,
        canceled: &mut canceled,
    });
    canceled
}

/// The damage hooks every level of the game lends to its hits (`Game.damage_hooks`).
pub(super) const DAMAGE_HOOKS: DamageHooks = DamageHooks {
    invulnerable: Some(invulnerability_check),
    incoming: Some(incoming_damage),
    pre: Some(damage_pre),
    post: Some(damage_post),
    knockback: Some(knockback),
    totem: Some(use_totem),
    shield: Some(shield_block),
    accepted: Some(living_death::capture_hit),
};
/// The contacts of a simulating entity with a block, as the simulation's own code (client-lib)
/// reports them.
pub(super) const BLOCK_CONTACT_HOOKS: horizon_client::block_contact::Hooks =
    horizon_client::block_contact::Hooks {
        step_on: |dimension, c| {
            Game::native_block_step_on(
                dimension,
                c.entity,
                c.kind,
                c.state,
                [c.position.x, c.position.y, c.position.z],
            );
        },
        any_inside: || mod_api::__private::has_scoped_hook(mod_api::HookId::EntityInside),
        inside_listens: |state| {
            crate::game::api_values::block_state::block_listens(
                mod_api::HookId::EntityInside,
                state,
            )
        },
        pass_listens: |state| {
            crate::game::api_values::block_state::block_listens(
                mod_api::HookId::MinecartPass,
                state,
            )
        },
        fall_listens: |state| {
            crate::game::api_values::block_state::block_listens(mod_api::HookId::FallOn, state)
        },
        bounce_listens: |state| {
            crate::game::api_values::block_state::block_listens(mod_api::HookId::BounceOn, state)
        },
        inside_fire: |level, dimension, entity, kind, state, pos, precise, staged, writes| {
            // The whole callback is staged: a panic is contained and the walk
            // keeps the caller's collector and write list as they were (arismoko/horizon#2247's
            // rule: a mod's panic never breaks the model).
            let fired = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                HitLevel(level.reborrow()).native_entity_inside(
                    crate::game::native_hooks::fire::EntityInsideCall {
                        dimension,
                        entity,
                        kind,
                        state,
                        pos,
                        precise,
                        effects: staged,
                        writes,
                    },
                );
            }));
            if let Err(payload) = fired {
                if let Some(text) = payload.downcast_ref::<&str>() {
                    eprintln!("an entity_inside part panicked at {text:?}");
                } else if let Some(text) = payload.downcast_ref::<String>() {
                    eprintln!("an entity_inside part panicked at {text:?}");
                }
            }
        },
        fall_on: |dimension, level, c, distance, damage, writes| {
            HitLevel(level.reborrow()).native_fall_on(
                crate::game::native_hooks::fire::FallOnCall {
                    dimension,
                    entity: c.entity,
                    kind: c.kind,
                    state: c.state,
                    pos: c.position,
                    distance,
                    damage,
                    writes,
                },
            );
        },
        bounce_on: |dimension, level, c, fall_distance, movement| {
            let mut answer = [movement.x, movement.y, movement.z];
            HitLevel(level.reborrow()).native_bounce_on(
                crate::game::native_hooks::fire::BounceOnCall {
                    dimension,
                    entity: c.entity,
                    kind: c.kind,
                    state: c.state,
                    pos: c.position,
                    fall_distance,
                    movement: &mut answer,
                },
            );
            *movement = horizon_core::geometry::Vec3::new(answer[0], answer[1], answer[2]);
        },
    };
pub(super) use horizon_client::health::mob_source;
pub(super) use living_death::hurt_server_args;

/// Whether `id` is a sound event's id, `namespace:path` as `Identifier`
/// allows: the namespace of `[a-z0-9_.-]`, the path also with `/`.
/// Whether a native mod's sound can be sent: a namespaced event, a sound
/// source, a position inside the world's range and a finite, non-negative
/// volume and pitch.
pub(super) fn mod_sound_ok(
    sound: &str,
    at: [f64; 3],
    source: usize,
    volume: f32,
    pitch: f32,
) -> bool {
    sound_id(sound)
        && source < horizon_client::settings::SOUND_SOURCES.len()
        && level_sound_playable(at, volume, pitch)
}

/// Whether a sound can be sent: a position inside the world's range and a
/// finite, non-negative volume and pitch.
pub(super) fn level_sound_playable(at: [f64; 3], volume: f32, pitch: f32) -> bool {
    let position_ok = at
        .into_iter()
        .all(|v| v.is_finite() && v.abs() <= f64::from(i32::MAX) / 8.0);
    let gain_ok = volume.is_finite() && volume >= 0.0 && pitch.is_finite() && pitch >= 0.0;
    position_ok && gain_ok
}

/// Whether a native mod's particles can be sent. Block particle types need
/// a known state (`state_ok`), item particles a validated stack; trail and vibration
/// aren't supported by the network dispatch. A mod's own type must be namespaced.
pub(super) fn mod_particles_ok(
    options: &ParticleOptions,
    state: Option<BlockStateId>,
    state_ok: impl Fn(BlockStateId) -> bool,
    at: [f64; 3],
    spread: [f64; 3],
    speed: f64,
) -> bool {
    use ParticleType as T;
    // A native mod's own type: the name has to be namespaced, and a client
    // draws it only when its pack has the sprite set.
    if let Some(name) = &options.mod_particle {
        let namespaced = name
            .split_once(':')
            .is_some_and(|(ns, path)| !ns.is_empty() && !path.is_empty());
        if !namespaced {
            return false;
        }
    }
    let supported = match options.ty {
        T::Block | T::BlockMarker | T::FallingDust | T::DustPillar | T::BlockCrumble => {
            state.is_some_and(state_ok)
        }
        T::Item | T::Trail | T::Vibration => crate::game::item_particle_send::supported(options),
        T::Geyser | T::GeyserPlume | T::GeyserBase | T::GeyserPoof => options
            .geyser
            .is_some_and(|(_, impulse)| impulse.is_finite()),
        _ => true,
    };
    supported
        && at.into_iter().all(f64::is_finite)
        && spread
            .into_iter()
            .all(|v| v.is_finite() && v.abs() <= f32::MAX as f64)
        && speed.is_finite()
        && speed.abs() <= f32::MAX as f64
        && options.color.into_iter().all(f32::is_finite)
        && options.to_color.into_iter().all(f32::is_finite)
        && options.scale.is_finite()
}

pub(super) fn sound_id(id: &str) -> bool {
    let Some((namespace, path)) = id.split_once(':') else {
        return false;
    };
    let allowed = |c: char, slash: bool| {
        c.is_ascii_lowercase() || c.is_ascii_digit() || "_.-".contains(c) || (slash && c == '/')
    };
    !namespace.is_empty()
        && !path.is_empty()
        && namespace.chars().all(|c| allowed(c, false))
        && path.chars().all(|c| allowed(c, true))
}

/// Sets the viewer counts `known` holds for `dim` to `now` and returns the
/// changes as `(pos, previous, viewers)`, in position order. A container that
/// is no longer counted goes to 0 and leaves `known`.
pub(super) fn viewer_changes(
    known: &mut HashMap<(&'static str, (i32, i32, i32)), u32>,
    dim: &'static str,
    mut now: HashMap<(i32, i32, i32), u32>,
) -> Vec<((i32, i32, i32), u32, u32)> {
    let mut changes = Vec::new();
    known.retain(|&(d, pos), previous| {
        if d != dim {
            return true;
        }
        let viewers = now.remove(&pos).unwrap_or(0);
        if viewers != *previous {
            changes.push((pos, *previous, viewers));
            *previous = viewers;
        }
        viewers > 0
    });
    for (pos, viewers) in now {
        known.insert((dim, pos), viewers);
        changes.push((pos, 0, viewers));
    }
    changes.sort_unstable();
    changes
}

fn dimension_id(dimension: LevelDimension) -> &'static str {
    match dimension {
        LevelDimension::Overworld => "minecraft:overworld",
        LevelDimension::Nether => "minecraft:the_nether",
        LevelDimension::End => "minecraft:the_end",
        LevelDimension::Custom(key) => key.id(),
    }
}

/// Server-level helpers for native mods.
#[allow(dead_code)]
impl GameLevel<'_> {
    /// Play a sound event for nearby players in this server level. The return
    /// value means queued, not heard. A `minecraft:` event must be registered:
    /// an unknown one is dropped at dispatch. Another namespace's event (a
    /// mod's, `mymod:machine_hum`) is sent by name, as vanilla sends a sound
    /// that isn't in its registry (`Holder.direct`, a variable range): players
    /// hear it if their resources define it (`assets/mymod/sounds.json`, a
    /// content mod's).
    pub(super) fn native_play_sound(
        &mut self,
        sound: &str,
        at: [f64; 3],
        source: usize,
        volume: f32,
        pitch: f32,
    ) -> bool {
        if !self.native_authoritative || !mod_sound_ok(sound, at, source, volume, pitch) {
            return false;
        }
        // `Level.playSound` draws `soundSeedGenerator.nextLong` at the call;
        // without a host nothing can be sent.
        let Some(seed) = self.sound_seeds.map(|hub| hub.next_sound_seed()) else {
            return false;
        };
        self.native_sounds.push(super::creatures::NativeSound {
            dimension: dimension_id(self.dimension),
            sound: sound.to_owned(),
            at,
            source: source as i32,
            volume,
            pitch,
            seed,
            except: None,
            entity: None,
        });
        true
    }

    /// Queue particles in this authoritative level. A zero count sends one
    /// directional particle. The return value means accepted, not delivered.
    /// Block particle types require their block state; item, trail and
    /// vibration particles are not supported by the current network dispatch.
    /// `ParticleOptions::named("mymod:spark")` sends a mod's own type
    ///: Horizon clients draw it from the sprite
    /// set in the mod's `assets/<namespace>/particles/<path>.json`, a client
    /// without that file draws nothing, and vanilla clients aren't sent it.
    pub(super) fn native_send_particles(
        &mut self,
        options: &ParticleOptions,
        state: Option<BlockStateId>,
        at: [f64; 3],
        spread: [f64; 3],
        speed: f64,
        count: i32,
    ) -> bool {
        let state_ok = |id| self.registry.state(id).is_some();
        if !self.native_authoritative
            || !mod_particles_ok(options, state, state_ok, at, spread, speed)
        {
            return false;
        }
        self.native_particles
            .push(super::lan_particles::QueuedParticle {
                dimension: dimension_id(self.dimension),
                options: options.clone(),
                state,
                at,
                spread,
                speed: [speed; 3],
                count,
                randomization: 0,
            });
        true
    }
    /// `EntityBlock.newBlockEntity` of block `name`: a declared mod type
    /// starts with the data `native_block_entity` gives, a mod's sign, else
    /// Horizon's own. The one place both whole-block write paths make it.
    pub(super) fn new_block_entity(&self, name: &str) -> Option<BlockEntity> {
        let mut nbt = None;
        if horizon_client::block_entity_types::type_for(name).is_some() {
            self.native_block_entity(name, self.registry.default_state(name)?, &mut nbt);
        }
        block_entity_for_block(self.registry, self.facts, name, nbt.unwrap_or_default())
    }
    /// Creates a declared state-aware block entity from its actual state. No answer
    /// means no entity; the registered declaration owns its type identity.
    pub(super) fn new_block_entity_for_state(&self, state: BlockStateId) -> Option<BlockEntity> {
        let name = &self.registry.state(state)?.name;
        if self.world.block_entities.stateful_entity(state.0) {
            let mut nbt = None;
            self.native_block_entity(name, state, &mut nbt);
            return block_entity_for_block(self.registry, self.facts, name, nbt?);
        }
        self.new_block_entity(name)
    }
    /// Replaces the block at `pos` with `state` through `Level.setBlock`, as
    /// `Game::native_set_block` does for the Host, from a hook that gets the
    /// level's `ticks`: the old block's block entity is removed and the new
    /// block's is created in the write (`LevelChunk.setBlockState`), and the
    /// write's level events stay queued for the caller, in order; a mod block
    /// entity starts with the data `native_block_entity` gives. True when the block changed; false off an
    /// authoritative server, for a position that is not loaded or outside the
    /// build height, an invalid state, or an unchanged state. Never loads a
    /// chunk.
    pub(super) fn native_set_block(
        &mut self,
        ticks: &mut Ticks,
        pos: [i32; 3],
        state: BlockStateId,
        flags: i32,
    ) -> bool {
        if !self.native_authoritative
            || !self.world.is_loaded_at(pos[0], pos[1], pos[2])
            || self.registry.state(state).is_none()
        {
            return false;
        }
        let at = BlockPos::new(pos[0], pos[1], pos[2]);
        let previous = self.world.block(pos[0], pos[1], pos[2]);
        let b = self.behaviours;
        let changed = b.set_block(self, ticks, at, state, flags);
        let landed = self.world.block(pos[0], pos[1], pos[2]);
        if !self.world.block_entities.stateful_entity(landed.0)
            && block_entity_to_create(self.facts, changed, previous, state, landed).is_some()
            && let Some(entity) = self.new_block_entity_for_state(landed)
        {
            put_block_entity(self.world, pos, entity);
        }
        changed
    }
    /// `Level.removeBlock`: replace a block with its fluid's legacy block.
    pub(super) fn remove_block_write(
        &mut self,
        ticks: &mut Ticks,
        pos: [i32; 3],
        moved: bool,
    ) -> bool {
        if !self.native_authoritative || !self.world.is_loaded_at(pos[0], pos[1], pos[2]) {
            return false;
        }
        self.behaviours
            .remove_block(self, ticks, BlockPos::new(pos[0], pos[1], pos[2]), moved)
    }
    /// Schedules a tick of `block` (a block name) at `pos` in `delay` game
    /// ticks, as a falling block's shape update schedules its fall
    /// (`ScheduledTickAccess.scheduleTick(pos, block, delay)`, normal
    /// priority); false for an unknown block or a position that isn't
    /// loaded. A tick of that block already queued there is kept and counts
    /// as scheduled. `ticks` is the hook's.
    pub(super) fn native_schedule_block_tick(
        &self,
        ticks: &mut Ticks,
        pos: [i32; 3],
        block: BlockKey,
        delay: i32,
    ) -> bool {
        let Some(block) = crate::game::api_values::behavior_block(block, self.behaviours) else {
            return false;
        };
        let at = BlockPos::new(pos[0], pos[1], pos[2]);
        let time = self.game_time();
        ticks.schedule_block(time, at, block, delay, TickPriority::Normal)
            != ScheduleResult::MissingChunk
    }
    /// Schedules a tick of `fluid` (`minecraft:water`, `minecraft:lava`,
    /// `minecraft:flowing_water`...) at `pos` in `delay` game ticks, as a
    /// waterlogged block's shape update schedules its water in 5
    /// (`ScheduledTickAccess.scheduleTick(pos, fluid, delay)`); false for
    /// an unknown fluid or a position that isn't loaded. A tick of that
    /// fluid already queued there is kept and counts as scheduled. `ticks` is
    /// the hook's.
    pub(super) fn native_schedule_fluid_tick(
        &self,
        ticks: &mut Ticks,
        pos: [i32; 3],
        fluid: &str,
        delay: i32,
    ) -> bool {
        let Some(kind) = FluidKind::from_name(fluid) else {
            return false;
        };
        let at = BlockPos::new(pos[0], pos[1], pos[2]);
        let time = self.game_time();
        ticks.schedule_fluid(time, at, kind, delay) != ScheduleResult::MissingChunk
    }
    /// The state `state` with its property `key` set to `value`, as a mod block
    /// builds the state it is placed in: `native_state_with_property(cable,
    /// "facing", "east")` (`BlockState.setValue`). A block's own declared
    /// properties (`state_properties` in its block file) are set the same way.
    /// `state` itself when the property already has that value; None when the
    /// block has no such property, or the property no such value.
    pub(super) fn native_state_with_property(
        &self,
        state: BlockStateId,
        key: &str,
        value: &str,
    ) -> Option<BlockStateId> {
        with_property(self.registry, state, key, value)
    }
    /// The block `state` is a state of (`BlockState.getBlock()`): its name,
    /// `mymod:cable`. `native_state_for_placement` is asked for every block
    /// placed and given the block's default state: compare this with your
    /// block to answer only for it. None for an unknown state.
    pub(super) fn native_state_block(&self, state: BlockStateId) -> Option<&str> {
        self.registry.state(state).map(|s| &*s.name)
    }
}

/// `EntityBlock.newBlockEntity` of block `name`: a declared mod type starting
/// with `initial` data, a mod's sign, else Horizon's own. Shared by the
/// Host's and a level hook's whole-block writes.
fn block_entity_for_block(
    registry: &BlockRegistry,
    facts: &horizon_client::blocks::BlockFacts,
    name: &str,
    initial: Compound,
) -> Option<BlockEntity> {
    if let Some(kind) = horizon_client::block_entity_types::type_for(name) {
        return Some(BlockEntity::Mod(Box::new(EngineBlockEntity::new(
            kind.to_owned(),
            ModBlockEntityFields {
                nbt: initial,
                ..Default::default()
            },
        ))));
    }
    // Undeclared content blocks retain their template's vanilla behaviour.
    // `SignBlock.newBlockEntity`: a mod's sign is one by its template's class.
    if !name.starts_with("minecraft:")
        && let Some(state) = registry.default_state(name)
        && let Some(form) = facts.get(state).sign_form()
    {
        let text = facts.sign_text(name).map_or_else(Default::default, |d| {
            horizon_client::signs::SignText {
                color: d.color,
                glowing: d.glowing,
                ..Default::default()
            }
        });
        return Some(BlockEntity::Sign(horizon_client::signs::SignData {
            hanging: form.hanging,
            front: text.clone(),
            back: text,
            ..Default::default()
        }));
    }
    BlockEntity::for_block(name)
}

/// After a `Level.setBlock` of `state` over `previous`: the block whose block
/// entity the write creates, when the write changed the block's name and the
/// new block landed.
fn block_entity_to_create(
    facts: &horizon_client::blocks::BlockFacts,
    changed: bool,
    previous: BlockStateId,
    state: BlockStateId,
    landed: BlockStateId,
) -> Option<String> {
    let new = &facts.get(state).name;
    (changed && facts.get(previous).name != *new && facts.get(landed).name == *new)
        .then(|| new.to_string())
}

/// Stores a new block entity at `pos`, keeping an existing one of the same kind.
fn put_block_entity(world: &mut horizon_client::world::World, pos: [i32; 3], entity: BlockEntity) {
    let key = (pos[0], pos[1], pos[2]);
    if world
        .block_entities
        .get(&key)
        .is_none_or(|existing| std::mem::discriminant(existing) != std::mem::discriminant(&entity))
    {
        world.block_entities.insert(key, entity);
    }
}

/// Helpers for native mods, kept as stable as the hook points.
// Native mods use these; nothing in Horizon does.
#[allow(dead_code)]
impl Game<'_> {
    /// Sets a block in the current authoritative level through `Level.setBlock`.
    /// Returns false for another dimension, an invalid state, or no change.
    /// `flags` selects vanilla's neighbour, shape and client updates. A block
    /// entity block gets its block entity.
    pub(super) fn native_set_block(
        &mut self,
        dimension: &str,
        pos: [i32; 3],
        state: BlockStateId,
        flags: i32,
    ) -> bool {
        if (self.serving.is_none() && self.options.dedicated.is_none())
            || dimension != self.dim.id()
            || self.pack.assets.registry.state(state).is_none()
        {
            return false;
        }
        let changed =
            self.with_level(|_, level, ticks| level.native_set_block(ticks, pos, state, flags));
        let _ = self.process_level_events();
        changed
    }
    /// The item entities of the server level `dimension` whose box meets
    /// the box `min`..`max`, in the level's order: vanilla's
    /// `EntityGetter.getEntitiesOfClass(ItemEntity.class, box, ENTITY_STILL_ALIVE)`, as a
    /// hopper's `getItemsAtAndAbove` asks for its block and the one above
    /// (`[x, y, z]`..`[x + 1, y + 2, z + 1]`). Empty for a level not in place
    /// (as `native_set_block`) and on a client of a server.
    pub(super) fn native_item_entities(
        &self,
        dimension: &str,
        min: [f64; 3],
        max: [f64; 3],
    ) -> Vec<NativeItemEntity> {
        if (self.serving.is_none() && self.options.dedicated.is_none())
            || dimension != self.dim.id()
        {
            return Vec::new();
        }
        self.dropped
            .iter()
            .filter(|e| e.stack.count > 0 && box_meets(&e.bounds(), min, max))
            .map(|e| NativeItemEntity {
                id: e.id,
                position: [e.position.x, e.position.y, e.position.z],
                stack: e.stack.clone(),
                // The helper exposes an unsigned delay; keep its projection.
                pickup_delay: e.pickup_delay.clamp(0, i64::from(u32::MAX)) as u32,
            })
            .collect()
    }
    /// Runs `change` on the mod block entity at `pos` (a generator charging
    /// the machine beside it) and returns what it returns; none for any
    /// other block entity, and for the one ticking now, which is out of the
    /// world. Afterwards, as its `setChanged` would, its chunk is saved next
    /// time and, when its items changed, comparators read it again.
    pub(super) fn native_with_mod_block_entity<R>(
        &mut self,
        pos: [i32; 3],
        change: impl FnOnce(&mut EngineBlockEntity) -> R,
    ) -> Option<R> {
        let key = (pos[0], pos[1], pos[2]);
        let Some(BlockEntity::Mod(m)) = self.world.block_entities.get_mut(&key) else {
            return None;
        };
        if m.id().is_empty() {
            return None;
        }
        let items = m.items.clone();
        let out = change(m);
        let moved = m.items != items;
        self.world.modified.insert((key.0 >> 4, key.2 >> 4));
        if moved {
            self.container_changed(key);
        }
        Some(out)
    }
    /// A data file of the game's resources, content mods' included
    /// (`data/mymod/machines.json`), when there is one: what a mod's content
    /// says, read at runtime. Only relative paths of plain names. API 20.
    pub(super) fn native_data(&self, path: &str) -> Option<Vec<u8>> {
        let plain = |p: &str| !p.is_empty() && p != "." && p != ".." && !p.contains('\\');
        if !path.starts_with("data/") || !path.split('/').all(plain) {
            return None;
        }
        fs::read(self.options.resources.join(path)).ok()
    }
    pub(super) fn native_block_collision_in(
        &self,
        world: &horizon_client::world::World,
        pos: [i32; 3],
    ) -> Vec<[f64; 6]> {
        self.block_collision_shape_in(world, pos)
            .boxes()
            .iter()
            .map(|bounds| {
                let min = bounds.min_position();
                let max = bounds.max_position();
                [min.x, min.y, min.z, max.x, max.y, max.z]
            })
            .collect()
    }
    /// The player, in a world.
    pub(super) fn native_player(&self) -> Option<NativePlayer> {
        let p = self.player.as_ref()?;
        Some(NativePlayer {
            x: p.position.x,
            y: p.position.y,
            z: p.position.z,
            yaw: self.yaw,
            pitch: self.pitch,
            dimension: self.dim.id(),
        })
    }
    /// A native mod's own folder: `horizon-mods/.horizon-mod-data/<id>/`,
    /// where the WebAssembly mod of the same id kept its storage, so a
    /// player's settings carry over. Not created here.
    pub(super) fn native_data_dir(&self, mod_id: &str) -> PathBuf {
        Self::mod_data_root().join(mod_id)
    }
    pub(super) fn mod_data_root() -> PathBuf {
        Self::game_dir()
            .join(mod_list::DIR)
            .join(".horizon-mod-data")
    }
    /// The game's `config/` folder beside `options.txt`, shared by every mod
    /// as on Fabric and NeoForge. The same folder on the window, its server
    /// thread and a LAN host; a dedicated server's is in its own folder,
    /// beside `server.properties`. Made when missing. Mods reach their own
    /// files in it through `Host::config_files`.
    pub(super) fn native_config_dir() -> std::io::Result<PathBuf> {
        config_dir(&Self::game_dir())
    }
    /// No world is open: a client without a player (the title screen). A
    /// server's `Game` (the server thread, a dedicated server, a LAN host)
    /// has no player of its own, and is always in its world.
    fn outside_world(&self) -> bool {
        self.player.is_none() && false
    }
    /// Every item and recipe the game knows, made once (the same for every
    /// call); none outside a world. They are the window's own (the content
    /// mods' and vanilla's), with the server's recipes of the types a mod had
    /// it send (`native_datapack_sync`) in their place; made again when those
    /// arrive (`native_recipes_received`).
    pub(super) fn native_recipes(&mut self) -> Option<Arc<NativeRecipes>> {
        self.player.as_ref()?;
        self.made_recipes()
    }
}

/// A mob's kind as the hook points name it (`minecraft:zombie`).
pub(super) fn mob_kind(mob: &horizon_client::mobs::Mob) -> String {
    crate::game::native_namespaced_id::namespaced(&mob.kind.id)
}

/// The level behaviour's answer, or `None` to try `useWithoutItem`.
/// `ServerPlayerGameMode.useItemOn` lets both FAIL and PASS go to the item.
/// Both success swing sources swing on this server path.
pub(super) fn block_use_result(
    answer: mod_api::InteractionResult,
) -> Option<horizon_world::level::InteractionResult> {
    use horizon_world::level::InteractionResult;
    match answer {
        mod_api::InteractionResult::Success | mod_api::InteractionResult::SuccessServer => {
            Some(InteractionResult::Success)
        }
        mod_api::InteractionResult::Consume => Some(InteractionResult::Consume),
        mod_api::InteractionResult::TryWithEmptyHand => None,
        _ => Some(InteractionResult::Pass),
    }
}

/// What the mods answered a right click on a block (`right_click_block`).
pub(super) enum BlockClickAnswer {
    /// Cancelled: this answers the click instead of the block and the item.
    Canceled(mod_api::InteractionResult),
    /// The block's use and the item's `useOn` run as these say.
    Go {
        use_block: TriState,
        use_item: TriState,
    },
}

/// What `BonemealEvent` settled for one use of bone meal.
pub(super) struct BonemealAsk {
    /// `Some(successful)` when a mod canceled the event: `growCrop`'s answer.
    pub crop: Option<bool>,
    /// `isValidBonemealTarget` as the event asked it, when a mod listens:
    /// `growCrop` takes it instead of asking again.
    pub check: Option<BonemealCheck>,
}

/// Where the game calls the hook points.
impl Game<'_> {
    /// `BonemealEvent` for bone meal from `stack` on `pos`, used by the
    /// acting player (`by_player`) or by a dispenser or a villager:
    /// `Some(successful)` when a mod canceled it, the answer `growCrop`
    /// gives in vanilla's place. The caller keeps what the mods did to
    /// `stack`.
    pub(super) fn bonemeal(
        &mut self,
        pos: BlockPos,
        by_player: bool,
        stack: &mut ItemStack,
    ) -> BonemealAsk {
        if !mod_api::__private::listened(mod_api::HookId::Bonemeal) {
            return BonemealAsk {
                crop: None,
                check: None,
            };
        }
        let client = false;
        let check = self.with_level(|b, level, _| b.is_valid_bonemeal_target(level, pos, client));
        let valid = check.is_valid();
        let state = self.world.block(pos.x, pos.y, pos.z);
        let (player, player_entity_id) = if by_player {
            let (uuid, entity) = self.native_clicker();
            (Some(uuid), Some(entity))
        } else {
            (None, None)
        };
        let (mut successful, mut canceled) = (false, false);
        let mut snapshot = Some(stack.clone());
        self.native_bonemeal(crate::game::native_hooks::fire::BonemealCall {
            dimension: self.dim.level(),
            player,
            player_entity_id,
            pos,
            state,
            source: BonemealSource::Interaction,
            stack: &mut snapshot,
            valid_bonemeal_target: valid,
            successful: &mut successful,
            canceled: &mut canceled,
        });
        match snapshot {
            Some(next) => *stack = next,
            None => stack.count = 0,
        }
        BonemealAsk {
            crop: canceled.then_some(successful),
            check: Some(check),
        }
    }
    /// Whether a mod canceled `placer` writing `state` at `pos` (`entity_place`):
    /// `placer` is an entity id, `placed_against` the block it goes against and
    /// `companion` the second half of a bed, door or tall plant.
    pub(super) fn place_canceled(
        &mut self,
        placer: u32,
        pos: BlockPos,
        state: BlockStateId,
        placed_against: BlockStateId,
        companion: Option<(BlockPos, BlockStateId)>,
    ) -> bool {
        if !mod_api::__private::listened(mod_api::HookId::EntityPlace) {
            return false;
        }
        let placement = match companion {
            None => Placement::Single,
            Some((at, extra)) => {
                let (Ok(state), Ok(extra)) = (state.try_into_api(), extra.try_into_api()) else {
                    return false;
                };
                Placement::Multi(Vec::from([
                    BlockPlacement::new(pos, state),
                    BlockPlacement::new(at, extra),
                ]))
            }
        };
        let placer = self.entity(placer);
        let mut canceled = false;
        self.native_entity_place(placer, pos, state, placed_against, placement, &mut canceled);
        canceled
    }
    /// Whether a mod canceled the block the player's `ctx` would place
    /// (`entity_place`): nothing is previewed unless a mod listens.
    pub(super) fn player_place_canceled(&mut self, ctx: &PlaceContext) -> bool {
        if !mod_api::__private::listened(mod_api::HookId::EntityPlace) {
            return false;
        }
        let Some(preview) =
            self.with_level(|b, level, ticks| b.placement_preview(level, ticks, ctx))
        else {
            return false;
        };
        let against_pos = ctx.placed_against_pos(preview.pos);
        let against = self
            .world
            .block(against_pos.x, against_pos.y, against_pos.z);
        let (_, entity) = self.native_clicker();
        self.place_canceled(
            entity.cast_unsigned(),
            preview.pos,
            preview.state,
            against,
            preview.companion,
        )
    }
    /// `MobEvent::PlaceBlock`: a mob sets `state` at `pos` unless a mod
    /// cancels `entity_place`. Enderman attempts use `enderman_place_block`.
    pub(super) fn mob_place_block(
        &mut self,
        mob: u32,
        pos: [i32; 3],
        state: BlockStateId,
    ) -> crate::Result<()> {
        let at = BlockPos::new(pos[0], pos[1], pos[2]);
        let against = self.world.block(pos[0], pos[1] - 1, pos[2]);
        let kind = if self.ecs.is_active() {
            self.ecs.kind(mob).map(str::to_owned)
        } else {
            self.remote_entities
                .iter()
                .find(|m| m.id == mob)
                .map(|m| m.kind.id.clone())
        };
        if self.place_canceled(mob, at, state, against, None) {
            return Ok(());
        }
        let facts = self.res.facts();
        let from = facts
            .get(self.world.block(pos[0], pos[1], pos[2]))
            .name
            .clone();
        let to = facts.get(state).name.clone();
        self.level_set(pos, state)?;
        if let Some(kind) = kind {
            self.native_entity_change_block(mob, &kind, pos, &from, &to);
        }
        Ok(())
    }
    /// The player clicking, as the block-use hooks name them: the acting
    /// player's UUID and entity id on a server, this client's own player on
    /// a client.
    pub(super) fn native_clicker(&mut self) -> (u128, i32) {
        let entity = self
            .lan_acting_entity
            .map_or_else(|| self.own_entity_id(), |id| id as i32);
        (self.native_acting_id(), entity)
    }
    /// `PlayerInteractEvent.RightClickBlock` for a click on `hit` with
    /// `hand` holding `stack`: what the mods let it use.
    pub(super) fn right_click_block(
        &mut self,
        hit: &super::Hit,
        hand: Hand,
        stack: Option<&ItemStack>,
    ) -> BlockClickAnswer {
        let (mut use_block, mut use_item) = (TriState::Default, TriState::Default);
        if !mod_api::__private::listened(mod_api::HookId::RightClickBlock) {
            return BlockClickAnswer::Go {
                use_block,
                use_item,
            };
        }
        let Ok(snapshot) = stack.try_into_api() else {
            return BlockClickAnswer::Go {
                use_block,
                use_item,
            };
        };
        let click = self.block_click(hit, hand, snapshot.as_ref());
        let (mut result, mut canceled) = (mod_api::InteractionResult::Pass, false);
        self.native_right_click_block(
            &click,
            &mut use_block,
            &mut use_item,
            &mut result,
            &mut canceled,
        );
        if canceled {
            BlockClickAnswer::Canceled(result)
        } else {
            BlockClickAnswer::Go {
                use_block,
                use_item,
            }
        }
    }
    /// The chosen phase's broadcast event, before its scoped behaviour.
    pub(super) fn use_item_on_block(
        &mut self,
        hit: &super::Hit,
        hand: Hand,
        stack: Option<&ItemStack>,
        phase: UsePhase,
    ) -> Option<mod_api::InteractionResult> {
        if !mod_api::__private::listened(mod_api::HookId::UseItemOnBlock) {
            return None;
        }
        let snapshot = stack.try_into_api().ok()?;
        let click = self.block_click(hit, hand, snapshot.as_ref());
        let (mut result, mut canceled) = (mod_api::InteractionResult::TryWithEmptyHand, false);
        self.native_use_item_on_block(&click, phase, &mut result, &mut canceled);
        if !canceled {
            return None;
        }
        if phase == UsePhase::ItemAfterBlock
            && result.consumes_action()
            && let Some(stack) = stack
        {
            self.with_hand(hand, |game| {
                {
                    game.award_stat("used", &stack.item, 1);
                }
                game.apply_after_use_components(stack);
            });
        }
        Some(result)
    }
    /// `ItemStack.onItemUseFirst` for a click on a block: the broadcast
    /// event's `ItemBeforeBlock` phase, then the held item's Part. Canceling skips that Part.
    /// Only `Pass` continues to the block.
    pub(super) fn item_use_first(
        &mut self,
        hit: &super::Hit,
        hand: Hand,
        stack: &ItemStack,
    ) -> Option<mod_api::InteractionResult> {
        if let Some(answer) =
            self.use_item_on_block(hit, hand, Some(stack), UsePhase::ItemBeforeBlock)
        {
            return Some(answer);
        }
        if !mod_api::__private::has_part_hook(mod_api::HookId::OnItemUseFirst) {
            return None;
        }
        let snapshot = stack.try_into_api().ok()?;
        let click = self.block_click(hit, hand, Some(&snapshot));
        let mut answer = None;
        self.native_on_item_use_first(&snapshot, &click, &mut answer);
        answer.filter(|answer| *answer != mod_api::InteractionResult::Pass)
    }
    /// `GameRuleChangedEvent` once the server set the rule `name` to `value`
    /// (`GameRules.set` with a server).
    pub(super) fn game_rule_changed(
        &mut self,
        name: &str,
        value: GameRuleValue,
        actor: Option<&GameRuleActor<'_>>,
    ) {
        if mod_api::__private::listened(mod_api::HookId::GameRuleChanged) {
            self.native_game_rule_changed(name, value, actor);
        }
    }
    /// `MobSplitEvent` for the dying cube mob `m`, which made `count`
    /// children: how many to spawn.
    pub(super) fn mob_split_count(&mut self, m: &horizon_client::mobs::Mob, count: u32) -> u32 {
        if !mod_api::__private::listened(mod_api::HookId::MobSplit) {
            return count;
        }
        let (size, kind) = (m.extra.size, mob_kind(m));
        let (mut count, mut cancel) = (count, false);
        self.native_mob_split(m.id, &kind, size, size / 2, &mut count, &mut cancel);
        if cancel { 0 } else { count.min(64) }
    }
    /// `AnimalTameEvent` for the mob at `index`, whose taming roll by the
    /// player `tamer` (`tamer_entity_id`) succeeded: true when a mod cancels
    /// it, and the taming fails.
    pub(super) fn animal_tame_cancelled(
        &mut self,
        index: usize,
        tamer: u128,
        tamer_entity_id: i32,
    ) -> bool {
        if !mod_api::__private::listened(mod_api::HookId::AnimalTame) {
            return false;
        }
        let m = &self.remote_entities[index];
        let (entity, kind) = (m.id, mob_kind(m));
        let mut cancel = false;
        self.native_animal_tame(tamer, tamer_entity_id, entity, &kind, &mut cancel);
        cancel
    }
    /// `entity_tame` for the pet at `index`, just tamed by its owner.
    pub(super) fn pet_tamed(&mut self, index: usize) {
        let m = &self.remote_entities[index];
        let (entity, kind) = (m.id, mob_kind(m));
        let owner = m
            .extra
            .owner
            .map(horizon_client::lan::playerdata::ints_uuid);
        self.native_entity_tame(entity, &kind, owner);
    }
    /// The acting player as the hook points name one: the guest whose turn
    /// it is, else the host.
    pub(super) fn native_acting_id(&mut self) -> u128 {
        horizon_client::lan::playerdata::ints_uuid(self.acting_player_uuid())
    }

    /// Tells the mods of the chunks that came into this level.
    pub(super) fn native_chunks_loaded(&mut self) {
        let loaded = std::mem::take(&mut self.world.loaded_chunks);
        let dimension = self.dim.id();
        if let Some(host) = self.lan.as_mut() {
            host.hub
                .terrain_residency_changed(loaded.iter().map(|&(x, z, _)| (x, z)));
            let lit = self
                .world
                .light_log
                .iter()
                .flat_map(|log| log.keys().copied());
            host.hub.warm_admitted_terrain(
                &self.world,
                &self.streaming,
                loaded.iter().map(|&(x, z, _)| (x, z)).chain(lit),
            );
        }
        for (x, z, new) in loaded {
            self.native_chunk_load(dimension, [x, z], new);
        }
    }
    /// Tells the mods of the chunks leaving this level, after their last save.
    pub(super) fn native_chunks_unloaded(
        &mut self,
        unloaded: &[std::sync::Arc<horizon_client::chunk::Chunk>],
    ) {
        let dimension = self.dim.id();
        if let Some(host) = self.lan.as_mut() {
            host.hub
                .terrain_residency_changed(unloaded.iter().map(|chunk| (chunk.x, chunk.z)));
        }
        for chunk in unloaded {
            self.native_chunk_and_block_entities_unload(dimension, [chunk.x, chunk.z]);
        }
    }
    /// The players still here leave with the server (`PlayerList.removeAll`):
    /// the mods hear it once, whichever path closes it.
    pub(super) fn native_players_leaving(&mut self) {
        let Some(host) = self.lan.as_mut() else {
            return;
        };
        if std::mem::replace(&mut host.players_told_left, true) {
            return;
        }
        let players: Vec<(u32, u128, String, crate::game::Dim)> = host
            .hub
            .guests
            .iter()
            .filter(|g| g.placed && g.participates_online())
            .map(|g| {
                (
                    g.conn,
                    g.uuid,
                    g.name.clone(),
                    crate::game::Dim::from_id(&g.dimension),
                )
            })
            .collect();
        // They are all still here: each hears of its leave first, then of its logout, in the
        // level they are in.
        let back = self.dim;
        for (conn, player, name, level) in &players {
            if self.enter_level(*level).is_ok() {
                self.announce_player_leaving(*conn, *player, name, "Server closed");
            }
        }
        if let Err(e) = self.enter_level(back) {
            log::error!("could not reopen {}: {e}", back.id());
        }
        for (_, player, name, _) in players {
            self.native_player_logged_out(player, &name);
        }
    }
    /// The mobs from index `from` on are new in the world.
    pub(super) fn native_mobs_joined(&mut self, from: usize, natural: bool) {
        self.join_spawn_tags(from);
        // A mod can keep a mob out; it is then taken out and not told of.
        self.native_joins_flush(false, 0);
        for i in from..self.remote_entities.len() {
            let m = &self.remote_entities[i];
            let (kind, pos) = (mob_kind(m), [m.position.x, m.position.y, m.position.z]);
            self.native_mob_spawned(self.dim.id(), &kind, pos, natural);
        }
    }
    /// The acting player's blow killed mob `index`.
    pub(super) fn native_kill_event(&mut self, index: usize) {
        let (entity, kind) = (
            self.remote_entities[index].id,
            mob_kind(&self.remote_entities[index]),
        );
        let player = self.native_acting_id();
        self.native_mob_killed(player, entity, &kind);
    }
    /// The acting player's hit landed on mob `index`, holding `stack`.
    pub(super) fn native_hurt_enemy_event(&mut self, stack: &ItemStack, index: usize) {
        let mob = &self.remote_entities[index];
        let kind = ["minecraft:", mob.kind.id.trim_start_matches("minecraft:")].concat();
        self.native_hurt_enemy(stack, mob.id, &kind);
    }
    /// `Inventory.tick` for the acting player: each carried stack goes through
    /// `native_inventory_tick`.
    pub(super) fn native_inventory_ticks(&mut self) {
        if self.player.is_none() {
            return;
        }
        for i in 0..self.inventory.len() {
            self.native_slot_tick(i as i32, i == self.selected);
        }
        // Head first here; `Inventory` saves feet as 100 and head as 103.
        for i in 0..self.armor.len() {
            self.native_slot_tick(103 - i as i32, false);
        }
        self.native_slot_tick(-106, false);
    }
    fn native_slot_tick(&mut self, slot: i32, selected: bool) {
        if !mod_api::__private::has_part_hook(mod_api::HookId::InventoryTick) {
            return;
        }
        let Some(stack) = self.native_carried_slot(slot).and_then(|s| s.clone()) else {
            return;
        };
        self.native_inventory_tick(&stack, slot, selected);
        if let Some(stack) = self.native_carried_slot(slot)
            && stack.as_ref().is_some_and(|s| s.count == 0)
        {
            *stack = None;
        }
    }
    /// The slot a player's carried stack is in, by `native_inventory_tick`'s
    /// numbers: none for another number.
    pub(super) fn native_carried_slot(&mut self, slot: i32) -> Option<&mut Option<ItemStack>> {
        match slot {
            0.. if (slot as usize) < self.inventory.len() => self.inventory.get_mut(slot as usize),
            100..=103 => self.armor.get_mut(103 - slot as usize),
            -106 => Some(&mut self.offhand),
            _ => None,
        }
    }
    /// `EntityBlock.newBlockEntity` of block `name`: Horizon's own, else a
    /// native mod's, made as a write of the level creates it
    /// (`GameLevel::new_block_entity`). Only a declared mod type builds the
    /// level.
    #[cfg(test)]
    pub(super) fn new_block_entity(&mut self, name: &str) -> Option<BlockEntity> {
        if horizon_client::block_entity_types::type_for(name).is_none() {
            return block_entity_for_block(
                self.pack.assets.registry,
                self.res.facts(),
                name,
                Compound::default(),
            );
        }
        self.with_level(|_, level, _| level.new_block_entity(name))
    }
    /// Compile static state-factory ownership once per level, including on clients.
    pub(super) fn ensure_stateful_entity_table(&mut self) {
        if self.world.block_entities.has_stateful_entity_table() {
            return;
        }
        let registry = self.pack.assets.registry;
        let mut blocks = FxHashMap::default();
        let states = (0..registry.len() as u32)
            .map(|id| {
                let Some(state) = registry.state(BlockStateId(id)) else {
                    return false;
                };
                if state.name.starts_with("minecraft:") {
                    return false;
                }
                *blocks.entry(state.name.as_str()).or_insert_with(|| {
                    let mut stateful = false;
                    self.native_stateful_block_entity(&state.name, &mut stateful);
                    stateful
                })
            })
            .collect();
        self.world.block_entities.set_stateful_entities(states);
    }
    /// Create a block entity from an actual state using the level's creation hook.
    pub(super) fn new_block_entity_for_state(
        &mut self,
        state: BlockStateId,
    ) -> Option<BlockEntity> {
        self.with_level(|_, level, _| level.new_block_entity_for_state(state))
    }
    /// The containers of the mod blocks' block entities, by block state
    /// (`native_block_entity_container`), checked.
    pub(super) fn native_containers(&self) -> FxHashMap<u32, Arc<ModContainer>> {
        let registry = self.pack.assets.registry;
        let mut by_block: FxHashMap<String, Option<Arc<ModContainer>>> = FxHashMap::default();
        let mut out = FxHashMap::default();
        for i in 0..registry.len() as u32 {
            let Some(state) = registry.state(horizon_world::registry::BlockStateId(i)) else {
                continue;
            };
            if state.name.starts_with("minecraft:") {
                continue;
            }
            let container = by_block.entry(state.name.clone()).or_insert_with(|| {
                let mut container = None;
                self.native_block_entity_container(&state.name, &mut container);
                container.map(|mut c| {
                    c.slots = c.slots.min(256);
                    let n = c.slots;
                    for list in c.faces.iter_mut().chain([&mut c.inputs, &mut c.outputs]) {
                        list.retain(|&s| s < n);
                    }
                    c.menu = c
                        .menu
                        .filter(|m| mod_menu(m).is_some_and(|(_, size)| size == n));
                    Arc::new(c)
                })
            });
            if let Some(c) = container {
                out.insert(i, Arc::clone(c));
            }
        }
        out
    }
    /// The fluid tanks of the mod blocks' block entities, by block state
    /// (`native_block_entity_fluids`), checked as the containers are.
    pub(super) fn native_fluid_tables(&self) -> FxHashMap<u32, Arc<ModFluidTanks>> {
        let registry = self.pack.assets.registry;
        let mut by_block: FxHashMap<String, Option<Arc<ModFluidTanks>>> = FxHashMap::default();
        let mut out = FxHashMap::default();
        for i in 0..registry.len() as u32 {
            let Some(state) = registry.state(horizon_world::registry::BlockStateId(i)) else {
                continue;
            };
            if state.name.starts_with("minecraft:") {
                continue;
            }
            let tanks = by_block.entry(state.name.clone()).or_insert_with(|| {
                let mut fluids = None;
                self.native_block_entity_fluids(&state.name, &mut fluids);
                fluids.map(|mut f| {
                    f.tanks.truncate(16);
                    // Clamped, not dropped: the face and fill lists name
                    // tanks by number, which must not shift.
                    for c in &mut f.tanks {
                        *c = (*c).clamp(1, 1_000_000);
                    }
                    let n = f.tanks.len();
                    for list in f.faces.iter_mut().chain([&mut f.inputs, &mut f.outputs]) {
                        list.retain(|&t| t < n);
                    }
                    Arc::new(f)
                })
            });
            if let Some(t) = tanks {
                out.insert(i, Arc::clone(t));
            }
        }
        out
    }
    /// The energy buffers of the mod blocks' block entities, by block state
    /// (`native_block_entity_energy`), checked as the containers are.
    pub(super) fn native_energy_tables(&self) -> FxHashMap<u32, Arc<ModEnergyBuffer>> {
        let registry = self.pack.assets.registry;
        let mut by_block: FxHashMap<String, Option<Arc<ModEnergyBuffer>>> = FxHashMap::default();
        let mut out = FxHashMap::default();
        for i in 0..registry.len() as u32 {
            let Some(state) = registry.state(horizon_world::registry::BlockStateId(i)) else {
                continue;
            };
            if state.name.starts_with("minecraft:") {
                continue;
            }
            let buffer = by_block.entry(state.name.clone()).or_insert_with(|| {
                let mut energy = None;
                self.native_block_entity_energy(&state.name, &mut energy);
                energy.filter(|e| e.capacity >= 1).map(|mut e| {
                    e.max_insert = e.max_insert.clamp(0, e.capacity);
                    e.max_extract = e.max_extract.clamp(0, e.capacity);
                    Arc::new(e)
                })
            });
            if let Some(b) = buffer {
                out.insert(i, Arc::clone(b));
            }
        }
        out
    }
    /// Whether a native mod's block `name` has a ticking block entity.
    pub(in crate::game) fn native_ticks(&self, name: &str) -> bool {
        let mut ticks = false;
        self.native_block_entity_ticks(name, &mut ticks);
        ticks
    }
    /// A mod block's container at `key` opens its menu: its slots grown to
    /// the menu's. False for any other block.
    pub(super) fn open_mod_menu(&mut self, key: Pos) -> bool {
        let state = self.world.block(key.0, key.1, key.2).0;
        let Some(c) = self.world.block_entities.mod_container(state) else {
            return false;
        };
        let slots = c.slots;
        if c.menu.is_none() {
            return false;
        }
        let Some(BlockEntity::Mod(m)) = self.world.block_entities.get_mut(&key) else {
            return false;
        };
        if m.items.len() < slots {
            m.items.resize(slots, None);
        }
        true
    }
    /// `ContainerOpenersCounter` for the mod containers of the level in place:
    /// everyone with one open, recounted every tick, so a change is heard
    /// whichever way an opener left (`native_mod_container_viewers`). The
    /// openers are the guests (but not a spectator, `BarrelBlockEntity.startOpen`),
    /// the host's own player, and the copper golems that opened it
    /// (`chest_openers`).
    pub(super) fn tick_mod_container_viewers(&mut self) {
        let mut now: HashMap<(i32, i32, i32), u32> = HashMap::new();
        let mut open = |game: &Self, pos: (i32, i32, i32), n: u32| {
            if n > 0 && game.mod_menu_at(pos).is_some() {
                *now.entry(pos).or_default() += n;
            }
        };
        if let Some(host) = &self.lan {
            for g in host
                .hub
                .guests
                .iter()
                .filter(|g| host.hub.here(g) && !g.spectator())
            {
                if let Some(m) = &g.menu
                    && let Slots::Block(pos) = m.slots
                {
                    open(self, pos, 1);
                }
            }
        }
        if let Some(pos) = self.container.filter(|_| !self.spectator()) {
            open(self, pos, 1);
        }
        for (&pos, &(golems, _)) in &self.world.chest_openers {
            open(self, pos, golems.max(0) as u32);
        }
        let dim = self.dim.id();
        for (pos, previous, viewers) in viewer_changes(&mut self.mod_container_viewers, dim, now) {
            self.native_mod_container_viewers(dim, [pos.0, pos.1, pos.2], viewers, previous);
        }
    }
    /// The menu type of the mod container at `key`, if it has one.
    pub(super) fn mod_menu_at(&self, key: Pos) -> Option<&'static str> {
        let state = self.world.block(key.0, key.1, key.2).0;
        let c = self.world.block_entities.mod_container(state)?;
        mod_menu(c.menu.as_deref()?).map(|(menu, _)| menu)
    }
    /// Where the open mod screen puts container slot `slot`.
    pub(super) fn native_screen_slot(&self, slot: usize) -> Option<(f32, f32)> {
        let s = self.shown_native_screen()?;
        s.screen
            .slots
            .get(slot)
            .copied()
            .flatten()
            .map(|p| (p[0], p[1]))
    }
    /// The container slots the open mod screen shows (300.. ids).
    pub(super) fn native_screen_slots(&self) -> Option<Vec<usize>> {
        let s = self.shown_native_screen()?;
        let shown = s.screen.slots.iter().enumerate();
        Some(
            shown
                .filter(|(_, p)| p.is_some())
                .map(|(i, _)| 300 + i)
                .collect(),
        )
    }
    /// The first row of the player's inventory on the open mod screen.
    pub(super) fn native_screen_inventory_y(&self) -> Option<f32> {
        self.shown_native_screen().map(|s| s.screen.inventory_y)
    }
    /// The open mod screen's inventory's left column.
    pub(super) fn native_screen_inventory_x(&self) -> Option<f32> {
        self.shown_native_screen().map(|s| s.screen.inventory_x)
    }
    /// The open mod screen's width.
    pub(super) fn native_screen_width(&self) -> Option<f32> {
        self.shown_native_screen().map(|s| s.screen.width)
    }
    /// Puts back a mod block entity taken out for a call, unless the call
    /// removed or replaced it, with what its `setChanged` does.
    pub(super) fn put_back_mod_block_entity(
        &mut self,
        pos: Pos,
        taken: EngineBlockEntity,
        items: &[Option<ItemStack>],
    ) {
        if let Some(BlockEntity::Mod(m)) = self.world.block_entities.get_mut(&pos)
            && m.id().is_empty()
        {
            let moved = taken.items != items;
            **m = taken;
            // BlockEntity.setChanged: the next save writes its chunk, and
            // comparators read a container whose items changed.
            self.world.modified.insert((pos.0 >> 4, pos.2 >> 4));
            if moved {
                self.container_changed(pos);
            }
        } else {
            self.drop_unreturned_mod_contents(pos, taken);
        }
    }
    /// Menu button `id` on the mod container at `pos` (`clickMenuButton`),
    /// with the acting player's cursor stack (`carried`, read and written).
    pub(super) fn mod_menu_button(
        &mut self,
        context: mod_api::item::menu::MenuButtonContext,
        carried: &mut Option<ItemStack>,
    ) {
        let pos = (context.pos[0], context.pos[1], context.pos[2]);
        if self.mod_menu_at(pos).is_none() {
            return;
        }
        let Some(BlockEntity::Mod(m)) = self.world.block_entities.get_mut(&pos) else {
            return;
        };
        if m.id().is_empty() {
            return;
        }
        let mut taken = std::mem::take(&mut **m);
        let items = taken.items.clone();
        self.api_menu_button(&context, &mut taken, carried);
        self.put_back_mod_block_entity(pos, taken, &items);
    }
    /// Options > Mods' rows for loaded code mods, after the content-only rows.
    pub(super) fn list_native_mods(&mut self) {
        let registered = super::mods::collect_key_mappings(
            self.options.execution_role,
            mod_api::__private::key_mappings(),
        );
        let native = self.mods.native.clone();
        // Wasm proxies are installed in the same dispatch table as native mods. Their rows
        // need the same live switch and settings page, not a frozen content-only toggle.
        let mut rows: Vec<_> = self
            .settings
            .mods
            .installed
            .iter()
            .filter(|row| {
                mod_api::__private::mod_index(&row.id).is_some()
                    && !native.built().iter().any(|m| m.id == row.id)
            })
            .cloned()
            .collect();
        rows.extend(native.packages().iter().map(|package| {
            let m = &package.manifest;
            self.settings
                .mods
                .info
                .insert(m.id.clone(), horizon_client::mod_list::ModInfo::of(m, None));
            Installed {
                id: m.id.clone(),
                title: [m.name.as_str(), " ", m.version.as_str()].concat(),
                description: m.description.clone(),
                problem: None,
                dev: false,
                native: None,
            }
        }));
        self.settings
            .mods
            .installed
            .retain(|row| !rows.iter().any(|code| code.id == row.id));
        for mut row in rows {
            let settings = self.native_page_settings(&row.id);
            let mappings: Vec<KeyMapping> = registered
                .iter()
                .filter(|(owner, _)| *owner == row.id)
                .map(|(_, k)| KeyMapping {
                    id: k.id.clone(),
                    label: k.label.clone(),
                    key: k.key.clone(),
                    modifiers: k.modifiers.bits,
                    context: crate::game::api_values::TryIntoEngine::try_into_engine(
                        &k.context,
                        &(),
                    )
                    .unwrap(),
                    category: k.category.clone(),
                })
                .collect();
            row.native = Some(NativePage { mappings, settings });
            self.settings.mods.installed.push(row);
        }
    }
    /// The settings mod `mod_id` registered, with their kept values, as its page shows them.
    fn native_page_settings(&self, mod_id: &str) -> Vec<NativeSetting> {
        mod_api::__private::settings()
            .iter()
            .filter(|(owner, _)| *owner == mod_id)
            .map(|(_, spec)| {
                let kept = self.stored_setting(mod_id, &spec.key);
                let value = match (&spec.kind, spec.read(kept.as_deref())) {
                    (_, mod_api::SettingValue::Toggle(on)) => NativeSettingValue::Toggle(on),
                    (
                        mod_api::SettingKind::Choice { options, .. },
                        mod_api::SettingValue::Choice(index),
                    ) => NativeSettingValue::Choice {
                        options: options.clone(),
                        index,
                    },
                    (
                        mod_api::SettingKind::Range { min, max, step, .. },
                        mod_api::SettingValue::Range(value),
                    ) => NativeSettingValue::Range {
                        min: *min,
                        max: *max,
                        step: *step,
                        value,
                    },
                    (_, mod_api::SettingValue::Color(rgb)) => NativeSettingValue::Color(rgb),
                    _ => NativeSettingValue::Toggle(false),
                };
                NativeSetting {
                    key: spec.key.clone(),
                    label: spec.label.clone(),
                    tooltip: spec.tooltip.clone(),
                    value,
                }
            })
            .collect()
    }
    /// Setting `j` of native mod `i`'s page (its index in Options > Mods) changed on the page:
    /// kept as the mod registered it, then the mod is told (`setting_changed`).
    pub(super) fn native_setting_event(&mut self, i: usize, j: usize) {
        let Some(row) = self.settings.mods.installed.get(i) else {
            return;
        };
        let Some(setting) = row.native.as_ref().and_then(|page| page.settings.get(j)) else {
            return;
        };
        let (id, key) = (row.id.clone(), setting.key.clone());
        let value = match &setting.value {
            NativeSettingValue::Toggle(on) => mod_api::SettingValue::Toggle(*on),
            NativeSettingValue::Choice { index, .. } => mod_api::SettingValue::Choice(*index),
            NativeSettingValue::Range { value, .. } => mod_api::SettingValue::Range(*value),
            NativeSettingValue::Color(rgb) => mod_api::SettingValue::Color(*rgb),
        };
        let kept = mod_api::__private::setting(&id, &key).and_then(|spec| spec.store(value));
        let Some(kept) = kept else {
            return;
        };
        if self.store_setting(&id, &key, &kept) {
            self.native_setting_changed(&id, &key);
        }
    }
    /// The pose of native mods' camera shake in this frame (`native_camera_shake`),
    /// none while not playing.
    pub(super) fn native_shake_matrix(&self, partial: f32) -> glam::Mat4 {
        if !self.playing() {
            return glam::Mat4::IDENTITY;
        }
        let mut shake = Shake::default();
        self.native_camera_shake(partial, &mut shake);
        if shake == Shake::default() {
            return glam::Mat4::IDENTITY;
        }
        shake_pose(bounded_shake(shake, self.screen_effect_scale()))
    }

    /// The camera native mods draw this frame from (`native_camera_pose`), starting from
    /// `view`, vanilla's position, yaw and pitch.
    pub(super) fn native_camera_pose_now(
        &mut self,
        partial: f32,
        view: (glam::DVec3, f32, f32),
    ) -> (glam::DVec3, f32, f32) {
        let (mut position, mut yaw, mut pitch) = (view.0.to_array(), view.1, view.2);
        self.native_camera_pose(partial, &mut position, &mut yaw, &mut pitch);
        let posed = posed_camera(view, position, yaw, pitch);
        if self.apply_requested_camera_type()
            && let Some(refreshed) = self.camera_view(partial)
        {
            // Refresh vanilla components, not the listener's explicit overrides.
            return (
                if posed.0 == view.0 {
                    refreshed.0
                } else {
                    posed.0
                },
                if posed.1 == view.1 {
                    refreshed.1
                } else {
                    posed.1
                },
                if posed.2 == view.2 {
                    refreshed.2
                } else {
                    posed.2
                },
            );
        }
        posed
    }

    /// The roll native mods draw this frame with (`native_camera_roll`), in degrees about
    /// the view direction; 0 when no mod rolls it.
    pub(super) fn native_camera_roll_now(&self, partial: f32) -> f32 {
        let mut roll = 0.0;
        self.native_camera_roll(partial, &mut roll);
        posed_roll(roll)
    }

    /// What native mods hand shader packs in this frame (`native_shader_params`),
    /// with values that aren't finite as 0.
    pub(super) fn native_shader_params_now(&self, partial: f32) -> [[f32; 4]; 8] {
        let mut params = [[0.0; 4]; 8];
        self.native_shader_params(partial, &mut params);
        finite_params(params)
    }
    /// The minimap a native mod shows.
    pub(super) fn native_minimap_now(&self) -> Option<mod_api::Minimap> {
        let mut minimap = None;
        self.native_minimap(&mut minimap);
        minimap.map(|m| mod_api::Minimap {
            size: finite(m.size, 16.0, 256.0),
            blocks_per_pixel: finite(m.blocks_per_pixel, 0.5, 4.0),
            ..m
        })
    }
}

/// Whether `rgba` is a whole `width` x `height` image that fits a
/// `slot_width` x `slot_height` atlas slot (a native mod's `mod_image`).
fn mod_image_fits(width: u32, height: u32, rgba: usize, slot_width: u32, slot_height: u32) -> bool {
    width > 0
        && height > 0
        && width <= slot_width
        && height <= slot_height
        && rgba == width as usize * height as usize * 4
}

/// Whether F3's debug screen, open when `debug`, covers `corner`
/// (`DebugScreenOverlay.extractRenderState`): its left and right columns
/// the top corners, and its charts the bottom corners `charts` (bottom left,
/// bottom right) says they are drawn in.
fn debug_takes(corner: Corner, debug: bool, charts: [bool; 2]) -> bool {
    debug
        && match corner {
            Corner::TopLeft | Corner::TopRight => true,
            Corner::BottomLeft => charts[0],
            Corner::BottomRight => charts[1],
        }
}

/// A menu of `MOD_MENUS` or a content mod's menu type, by its name.
fn mod_menu(name: &str) -> Option<(&'static str, usize)> {
    MOD_MENUS
        .into_iter()
        .find(|(menu, _)| *menu == name)
        .or_else(|| horizon_client::containers::mod_menu_type(name).map(|m| (m.id, m.slots)))
}

/// `state` with its property `key` set to `value`; none when the block has
/// no such property or value.
pub(super) fn with_property(
    registry: &BlockRegistry,
    state: BlockStateId,
    key: &str,
    value: &str,
) -> Option<BlockStateId> {
    let mut s = registry.state(state)?.clone();
    match s.properties.get(key) {
        Some(v) if v == value => return Some(state),
        Some(_) => {}
        None => return None,
    }
    s.properties.insert(key.into(), value.into());
    registry.resolve(&s).ok()
}

/// The state a mod's property write changes `state` to; none when the block
/// has no such property or value, or already has it: `LevelChunk.setBlockState`
/// returns null for the same state, so `Level.setBlock` returns false.
fn property_change(
    registry: &BlockRegistry,
    state: BlockStateId,
    key: &str,
    value: &str,
) -> Option<BlockStateId> {
    with_property(registry, state, key, value).filter(|&id| id != state)
}

/// `config/` in the game folder, beside `options.txt`, made when missing.
fn config_dir(game_dir: &Path) -> std::io::Result<PathBuf> {
    let dir = game_dir.join("config");
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// `params` with each value that isn't finite as 0, so a mod's mistake can't
/// put a NaN in every pixel of a pack's picture.
fn finite_params(params: [[f32; 4]; 8]) -> [[f32; 4]; 8] {
    params.map(|v| v.map(|x| if x.is_finite() { x } else { 0.0 }))
}

/// Whether `play_local_sound` takes this sound: a sound event's id (`sound_id`, as
/// `native_play_sound` takes), a finite position, a volume and pitch at or above 0, a repeat
/// gap that isn't negative, and a start a minute away at most.
fn local_sound_ok(sound: &SoundPlay, after: u32) -> bool {
    sound_id(&sound.identifier)
        && sound
            .position
            .iter()
            .all(|v| v.is_finite() && v.abs() <= 30_000_000.0)
        && sound.volume.is_finite()
        && sound.volume >= 0.0
        && sound.pitch.is_finite()
        && sound.pitch >= 0.0
        && sound.delay >= 0
        && after <= 1200
}

/// `v` within `lo..=hi`; `lo` when it is not a number.
fn finite(v: f32, lo: f32, hi: f32) -> f32 {
    if v.is_finite() { v.clamp(lo, hi) } else { lo }
}

/// An item entity as `native_item_entities` lists it.
/// Read by mods only.
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub(super) struct NativeItemEntity {
    /// The entity's ID.
    pub id: u32,
    pub position: [f64; 3],
    pub stack: ItemStack,
    /// Ticks before a player may pick it up (`ItemEntity.pickupDelay`);
    /// hoppers take it whatever this is.
    pub pickup_delay: u32,
}

/// `AABB.intersects`: the boxes overlap, touching faces excluded.
fn box_meets(b: &horizon_core::geometry::Aabb, min: [f64; 3], max: [f64; 3]) -> bool {
    let (lo, hi) = (b.min_position(), b.max_position());
    lo.x < max[0]
        && hi.x > min[0]
        && lo.y < max[1]
        && hi.y > min[1]
        && lo.z < max[2]
        && hi.z > min[2]
}

/// What `mod_screen_open(mod_id)` does over `screen` (`blocked`: an
/// overlay, the title screen, no window or no player; `owner`'s own screen
/// open): `None` opens it, `Some` answers without changing anything (true:
/// `mod_id`'s is already open).
fn mod_screen_opens(
    screen: Screen,
    blocked: bool,
    owner: Option<&str>,
    mod_id: &str,
) -> Option<bool> {
    if screen == Screen::Mod {
        return Some(owner == Some(mod_id));
    }
    if screen != Screen::Playing || blocked {
        return Some(false);
    }
    None
}

fn client_packet_view(packet: &horizon_protocol::packets::play::Clientbound) -> ClientPacket {
    use horizon_protocol::packets::play::Clientbound;
    use mod_api::Identifier;
    use std::sync::LazyLock;

    static TYPES: LazyLock<Vec<Identifier>> =
        LazyLock::new(|| packet_types(horizon_protocol::ids::play::clientbound::NAMES));
    let payload = match packet {
        Clientbound::CustomPayload(payload) => Some(payload),
        _ => None,
    };
    packet_view(&TYPES, packet.id(), payload)
}

/// `client_packet_send`'s view of a packet the game sends: as `client_packet_view`.
fn serverbound_packet_view(packet: &horizon_protocol::packets::play::Serverbound) -> ClientPacket {
    use horizon_protocol::packets::play::Serverbound;
    use mod_api::Identifier;
    use std::sync::LazyLock;

    static TYPES: LazyLock<Vec<Identifier>> =
        LazyLock::new(|| packet_types(horizon_protocol::ids::play::serverbound::NAMES));
    let payload = match packet {
        Serverbound::CustomPayload(payload) => Some(payload),
        _ => None,
    };
    packet_view(&TYPES, packet.id(), payload)
}

/// One direction's packet type identifiers, by protocol id.
fn packet_types(names: &[&str]) -> Vec<mod_api::Identifier> {
    names
        .iter()
        .map(|name| mod_api::Identifier::parse(name).expect("protocol packet type identifier"))
        .collect()
}

/// A packet's type, `horizon:unknown` outside the protocol's table, and its custom payload.
fn packet_view(
    types: &[mod_api::Identifier],
    id: i32,
    payload: Option<&horizon_protocol::packets::configuration::CustomPayload>,
) -> ClientPacket {
    use mod_api::Identifier;
    use std::sync::LazyLock;

    static UNKNOWN: LazyLock<Identifier> =
        LazyLock::new(|| Identifier::parse("horizon:unknown").unwrap());
    let id = usize::try_from(id)
        .ok()
        .and_then(|id| types.get(id))
        .unwrap_or(&UNKNOWN)
        .clone();
    let payload = payload.map(|p| ClientPayload::new(p.channel.clone(), p.data.clone()));
    ClientPacket::new(id, payload)
}

#[cfg(test)]
mod tests {
    #[test]
    fn client_packet_view_exposes_the_type_and_custom_payload_only() {
        use horizon_protocol::packets::{
            configuration::{CustomPayload, Ping},
            play::Clientbound,
        };
        let ping = super::client_packet_view(&Clientbound::Ping(Ping { id: 7 }));
        assert_eq!(ping.id.to_string(), "minecraft:ping");
        assert_eq!(ping.custom_payload, None);

        let payload = super::client_packet_view(&Clientbound::CustomPayload(CustomPayload {
            channel: "example:message".into(),
            data: vec![0, 127, 255],
        }));
        assert_eq!(payload.id.to_string(), "minecraft:custom_payload");
        let payload = payload.custom_payload.unwrap();
        assert_eq!(payload.channel, "example:message");
        assert_eq!(payload.data, [0, 127, 255]);

        for id in [-1, i32::MAX] {
            let unknown = super::client_packet_view(&Clientbound::Unknown(id, vec![9]));
            assert_eq!(unknown.id.to_string(), "horizon:unknown");
            assert_eq!(unknown.custom_payload, None);
        }
    }

    #[test]
    fn serverbound_packet_view_names_the_packets_the_game_sends() {
        use horizon_protocol::packets::{
            configuration::CustomPayload,
            play::{Serverbound, SetCarriedItem},
        };
        let slot = super::serverbound_packet_view(&Serverbound::SetCarriedItem(SetCarriedItem {
            slot: 3,
        }));
        assert_eq!(slot.id.to_string(), "minecraft:set_carried_item");
        assert_eq!(slot.custom_payload, None);

        let payload = super::serverbound_packet_view(&Serverbound::CustomPayload(CustomPayload {
            channel: "example:message".into(),
            data: vec![1, 2],
        }));
        assert_eq!(payload.id.to_string(), "minecraft:custom_payload");
        let payload = payload.custom_payload.unwrap();
        assert_eq!(
            (payload.channel.as_str(), payload.data.as_slice()),
            ("example:message", &[1, 2][..])
        );

        let unknown = super::serverbound_packet_view(&Serverbound::Unknown(-1, vec![9]));
        assert_eq!(unknown.id.to_string(), "horizon:unknown");
    }

    #[test]
    #[ignore = "daily"]
    fn enchantment_registry_returns_records_and_unknown_cost() {
        super::super::native_entity_refs::tests::with_game(|game| {
            let all = game.enchantments();
            let sharpness = all.iter().find(|e| e.id == "minecraft:sharpness").unwrap();
            assert_eq!((sharpness.max_level, sharpness.curse), (5, false));
            let binding = all
                .iter()
                .find(|e| e.id == "minecraft:binding_curse")
                .unwrap();
            assert!(binding.curse);
            assert!(
                game.enchantment_min_cost("minecraft:sharpness", 1)
                    .is_some()
            );
            assert_eq!(game.enchantment_min_cost("missing:unknown", 1), None);
        });
    }
    #[test]
    #[ignore = "daily"]
    fn translate_gives_the_text_of_a_defined_key_and_none_otherwise() {
        use horizon_client::lang::{Language, ResourceSource, scoped};
        struct Empty;
        impl ResourceSource for Empty {
            fn read(&self, _: &str) -> Option<Vec<u8>> {
                None
            }
        }
        super::super::native_entity_refs::tests::with_game(|game| {
            let (got, _) = scoped(Language::load(&[&Empty], "en_us"), || {
                (
                    game.translate("horizon.options.controller"),
                    game.translate("missing.key"),
                )
            });
            assert_eq!(got, (Some("Controller".to_owned()), None));
        });
    }
    #[test]
    #[ignore = "daily"]
    fn a_trait_mod_measures_text_through_its_draw_context() {
        use super::super::host::draw::GameDrawContext;
        use super::super::native_entity_refs::tests::with_game;
        use mod_api::DrawContext;

        with_game(|game| {
            let mut batch = horizon_client::hud::HudBatch::scratch([320, 240]);
            let draw = GameDrawContext::new(game, &mut batch);
            // A server's HUD atlas is metrics-only (upstream 2e5a2629a): no sprite sheets,
            // while an upper-case A still advances 6 GUI pixels.
            assert_eq!(draw.text_width("AA"), 12.0);
            assert_eq!(draw.text_width(""), 0.0);
        });
    }

    #[test]
    #[ignore = "daily"]
    fn declared_types_create_tick_and_reload_through_game() {
        use super::super::native_entity_refs::tests::{record_key, run_fresh_cases, with_game_at};
        use horizon_client::{chunk::Chunk, containers::BlockEntity, save};
        use mod_api::{self, *};
        use std::sync::Arc;

        const CHILD: &str = "HORIZON_DECLARED_BE_GAME_CHILD";
        if run_fresh_cases(
            "game::native_hooks::tests::declared_types_create_tick_and_reload_through_game",
            CHILD,
            &["valid", "unknown_block", "unknown_type", "block_not_type"],
        ) {
            return;
        }

        const FIRST: &str = "be_test:press";
        const SECOND: &str = "be_test:ornate_banner";
        const EMPTY: &str = "be_test:blank";
        const PLAIN: &str = "be_test:plain_banner";
        #[derive(Default)]
        struct Keys {
            own: std::sync::OnceLock<BlockEntityTypeKey>,
            shared: std::sync::OnceLock<BlockEntityTypeKey>,
            vanilla: std::sync::OnceLock<BlockEntityTypeKey>,
        }
        trait Probe {
            fn value(&self) -> u32;
        }
        struct Marker(u32);
        impl Probe for Marker {
            fn value(&self) -> u32 {
                self.0
            }
        }
        const PROBE: CapabilityKey<dyn Probe> = CapabilityKey::new("be_test:probe");
        struct Press;
        #[mod_api::horizon_part]
        impl BlockPart for Press {}
        #[mod_api::horizon_part]
        impl BlockEntityPart for Press {
            fn block_entity(
                &self,
                _: LevelSide<'_>,
                _: &mut BlockEntityEvent<'_>,
            ) -> Option<Compound> {
                Some(Compound::from([("energy".into(), Tag::Long(6300))]))
            }
            fn block_entity_tick(
                &self,
                _: &mut dyn ServerHost,
                event: &mut BlockEntityTickEvent<'_>,
            ) {
                let Some(Tag::Long(energy)) = event.entity.nbt.get_mut(&"energy".into()) else {
                    panic!("factory payload missing");
                };
                *energy += 1;
            }
        }
        struct User {
            keys: Arc<Keys>,
            unknown: Option<&'static str>,
        }
        #[mod_api::__subscriptions]
        impl Hooks for User {
            fn register(&self, registrar: &mut dyn Registrar, _: &mod_api::RegistrationContext) {
                let kind = registrar.block_entity_type_key("be_test:shared");
                registrar
                    .block_entity_part(&kind, Press)
                    .expect("native type");
                registrar
                    .block_entity_capability(&PROBE, &kind, |_, _| Some(Box::new(Marker(6300))));
                record_key(&self.keys.shared, kind);
                let vanilla = registrar.block_entity_type_key("minecraft:chest");
                registrar
                    .block_entity_capability(&PROBE, &vanilla, |_, _| Some(Box::new(Marker(26))));
                record_key(&self.keys.vanilla, vanilla);
                if let Some(id) = self.unknown {
                    registrar.block_entity_type_key(id);
                }
            }
        }
        struct TestMod {
            invalid: bool,
            keys: Arc<Keys>,
        }
        #[mod_api::__subscriptions]
        impl Hooks for TestMod {
            fn register(&self, registrar: &mut dyn Registrar, _: &mod_api::RegistrationContext) {
                let kind = registrar
                    .block_entity_type("shared")
                    .blocks(if self.invalid {
                        &["be_test:missing"]
                    } else {
                        &[FIRST, SECOND]
                    })
                    .build();
                record_key(&self.keys.own, kind);
                registrar
                    .block_entity_type("empty")
                    .blocks(&[EMPTY])
                    .build();
            }
            fn blocks(&self, _context: &mod_api::RegistrationContext, event: &mut BlocksEvent<'_>) {
                event.blocks.extend(
                    [FIRST, SECOND, EMPTY, PLAIN]
                        .map(|id| mod_api::Block::new(id, "minecraft:white_banner")),
                );
            }
        }
        let case = std::env::var(CHILD).unwrap();
        let invalid = case == "unknown_block";
        let unknown = match case.as_str() {
            "unknown_type" => Some("other:missing"),
            "block_not_type" => Some("minecraft:red_shulker_box"),
            _ => None,
        };
        let keys = Arc::new(Keys::default());
        let mods: Mods = Arc::from([
            ModEntry::new(
                "be_user",
                Box::new(User {
                    keys: keys.clone(),
                    unknown,
                }),
            ),
            ModEntry::new(
                "be_test",
                Box::new(TestMod {
                    invalid,
                    keys: keys.clone(),
                }),
            ),
        ]);
        mod_platform::__private::install(mods.clone());
        crate::game::api_values::install_parts(&mods).unwrap();
        let base = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").unwrap());
        let root = super::super::mods::content_root(&base, &Default::default());
        if invalid {
            let error = root.unwrap_err().to_string();
            assert!(error.contains("be_test:shared"), "{error}");
            assert!(error.contains("be_test:missing"), "{error}");
            assert!(!std::path::Path::new(".horizon-cache").exists());
            return;
        }
        if let Some(id) = unknown {
            let error = root.unwrap_err().to_string();
            assert!(error.contains("be_user"), "{error}");
            assert!(error.contains("unknown block entity type"), "{error}");
            assert!(error.contains(id), "{error}");
            assert!(horizon_client::block_entity_types::current().is_none());
            return;
        }
        assert_eq!(keys.shared.get(), keys.own.get());
        with_game_at(root.unwrap().prepared().to_path_buf(), |game| {
            game.options.dedicated = Some(Vec::new());
            game.world
                .chunks
                .insert((0, 0), Arc::new(Chunk::empty(0, 0)));
            let level = game.level(game.dim.id()).unwrap();
            let stone = game
                .pack
                .assets
                .registry
                .default_state("minecraft:stone")
                .unwrap();
            for (x, block) in [
                (1, FIRST),
                (2, SECOND),
                (3, EMPTY),
                (4, PLAIN),
                (5, "minecraft:chest"),
            ] {
                let state = game.pack.assets.registry.default_state(block).unwrap();
                assert_eq!(
                    game.level_set_block(level, [x, 63, 0], stone, BlockUpdateFlags::NONE),
                    Ok(true)
                );
                assert_eq!(
                    game.level_set_block(level, [x, 64, 0], state, BlockUpdateFlags::NONE),
                    Ok(true)
                );
                let pos = (x, 64, 0);
                if block == PLAIN {
                    // No declared type claims it, and no mod adds it to `minecraft:banner`'s
                    // valid blocks, so its template creates no entity (`validateBlockState`).
                    assert!(game.world.block_entities.get(&pos).is_none());
                    assert!(game.block_entity(level, [x, 64, 0]).is_none());
                    continue;
                }
                let handle = game.block_entity(level, [x, 64, 0]).unwrap();
                if block == "minecraft:chest" {
                    assert_eq!(handle.kind(game).as_ref(), keys.vanilla.get());
                    let probe = level.capability(game, &PROBE, [x, 64, 0], None).unwrap();
                    assert_eq!((probe.value(), probe.owner()), (26, "be_user".into()));
                    continue;
                }
                let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get(&pos) else {
                    panic!("declared type did not override its banner template");
                };
                if block == EMPTY {
                    assert_eq!(entity.id(), "be_test:empty");
                    assert!(entity.nbt.is_empty());
                    continue;
                }
                assert_eq!(entity.id(), "be_test:shared");
                assert_eq!(entity.nbt.get(&"energy".into()), Some(&Tag::Long(6300)));
                assert_eq!(handle.kind(game).as_ref(), keys.shared.get());
                let probe = level.capability(game, &PROBE, [x, 64, 0], None).unwrap();
                assert_eq!((probe.value(), probe.owner()), (6300, "be_user".into()));
                game.tick_mod_block_entity(pos);
                let saved = save::block_entity_encode::encode(
                    pos,
                    game.world.block_entities.get(&pos).unwrap(),
                    block,
                );
                assert_eq!(saved.get("energy"), Some(&Tag::Long(6301)));
                let (saved_pos, restored) =
                    save::decode_block_entity_for_block(&saved, block).unwrap();
                assert_eq!(saved_pos, pos);
                assert_eq!(restored.id(), "be_test:shared");
                assert!(save::decode_block_entity_for_block(&saved, EMPTY).is_none());
                game.world.block_entities.insert(pos, restored);
                game.tick_mod_block_entity(pos);
                let saved = save::block_entity_encode::encode(
                    pos,
                    game.world.block_entities.get(&pos).unwrap(),
                    block,
                );
                assert_eq!(saved.get("energy"), Some(&Tag::Long(6302)));
            }

            // A level hook's `LevelWriteHost::set_block` fires the same hook in its write.
            let first = game.pack.assets.registry.default_state(FIRST).unwrap();
            use crate::game::api_values::TryIntoApi;
            let written = game.with_level(|_, level, ticks| {
                LevelWriteHost::set_block(
                    &mut super::super::mod_api_host::LevelTicks { level, ticks },
                    [5, 64, 0],
                    first.try_into_api().unwrap(),
                    BlockUpdateFlags::NONE,
                )
            });
            assert!(written);
            let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get(&(5, 64, 0)) else {
                panic!("a level write did not create the declared block entity");
            };
            assert_eq!(entity.id(), "be_test:shared");
            assert_eq!(entity.nbt.get(&"energy".into()), Some(&Tag::Long(6300)));
        });
    }

    #[test]
    #[ignore = "daily"]
    fn replacing_mod_block_entity_payload_keeps_its_type_and_saved_identity() {
        use super::super::native_entity_refs::tests::with_game;
        use horizon_client::chunk::Chunk;
        use horizon_client::containers::{BlockEntity, ModBlockEntity, ModBlockEntityFields};
        use horizon_client::save::{block_entity_encode, decode_block_entity};
        use std::sync::Arc;

        with_game(|game| {
            game.world
                .chunks
                .insert((0, 0), Arc::new(Chunk::empty(0, 0)));
            let pos = (1, 64, 1);
            let machine = ModBlockEntity::new("test:machine".into(), Default::default());
            let made = machine.revision();
            game.world
                .block_entities
                .insert(pos, BlockEntity::Mod(Box::new(machine)));
            assert_eq!(
                game.native_with_mod_block_entity([1, 64, 1], |entity| {
                    **entity = ModBlockEntityFields {
                        data: vec![7],
                        ..Default::default()
                    };
                    assert_eq!(entity.id(), "test:machine");
                }),
                Some(())
            );
            let entity = game.world.block_entities.get(&pos).unwrap();
            let BlockEntity::Mod(machine) = entity else {
                panic!("not a mod block entity");
            };
            assert_eq!(machine.id(), "test:machine");
            assert_eq!(machine.data, [7]);
            assert_ne!(machine.revision(), made);
            assert!(game.world.modified.contains(&(0, 0)));
            let saved = block_entity_encode::encode(pos, entity, "minecraft:stone");
            let (saved_pos, loaded) = decode_block_entity(&saved).unwrap();
            assert_eq!(saved_pos, pos);
            assert_eq!(loaded.id(), "test:machine");
        });
    }

    #[test]
    fn hand_pose_rejects_invalid_components() {
        let identity = super::hand_pose_matrix([0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0]);
        assert_eq!(identity, glam::Mat4::IDENTITY);
        assert_eq!(
            super::hand_pose_matrix([
                f32::NAN,
                17.0,
                f32::INFINITY,
                3601.0,
                -3601.0,
                f32::NAN,
                0.0
            ]),
            identity
        );
        let point = super::hand_pose_matrix([1.0, 2.0, 3.0, 0.0, 0.0, 0.0, 2.0])
            .transform_point3(glam::Vec3::X);
        assert_eq!(point, glam::Vec3::new(3.0, 2.0, 3.0));
    }
    /// Exercise the gameplay constructor, where name-based entities compete
    /// with a mod block's sign template.
    #[test]
    #[ignore = "daily"]
    fn mod_sign_constructor_prioritizes_templates_over_suffixes() {
        // This fixture adds blocks that are absent from the shared vanilla
        // game's frozen process-wide block definitions.
        if super::super::native_entity_refs::tests::run_fresh_cases(
            "game::native_hooks::tests::mod_sign_constructor_prioritizes_templates_over_suffixes",
            "HORIZON_SIGN_CONSTRUCTOR_CHILD",
            &["signs"],
        ) {
            return;
        }
        use super::super::{Audio, Game, Options, Progress, Settings, load_resources};
        use horizon_client::mod_content::{ContentMod, build};
        use horizon_mod_package::content::{Block, Content};

        let base = std::path::PathBuf::from(
            std::env::var_os("HORIZON_TEST_RESOURCES")
                .expect("set HORIZON_TEST_RESOURCES to prepared resources"),
        );
        let root = std::env::temp_dir().join(format!(
            "horizon-sign-constructor-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let cases = [
            ("ruby:notice_banner", "minecraft:oak_sign", false),
            ("ruby:board_shelf", "minecraft:oak_wall_sign", false),
            ("ruby:lantern_head", "minecraft:oak_hanging_sign", true),
            (
                "ruby:bracket_skull",
                "minecraft:oak_wall_hanging_sign",
                true,
            ),
        ];
        let mut content = Content::declared("ruby");
        let blocks: Vec<_> = cases
            .iter()
            .map(|&(id, template, _)| (id, template))
            .chain([("ruby:road_sign", "minecraft:stone")])
            .map(|(id, template)| Block {
                id: id.into(),
                template: template.into(),
                item: false,
                sign_text_color: (id == "ruby:notice_banner").then(|| "light_blue".into()),
                sign_text_glow: (id == "ruby:notice_banner").then_some(true),
                ..Default::default()
            })
            .collect();
        content.declare(&blocks, &[]).unwrap();
        build(
            &base,
            std::path::Path::new(""),
            &[ContentMod {
                id: "ruby".into(),
                content,
                jars: Vec::new(),
            }],
            &root,
            &horizon_mod_package::enabled::EnabledMods::new([("ruby".into(), mod_api::Side::Both)]),
        )
        .unwrap();
        let settings = Settings::default();
        let loaded = load_resources(
            horizon_assets::resource_source::ResourceRoot::directory(root.clone()),
            Vec::new(),
            None,
            &root,
            &settings,
            std::time::Instant::now(),
            false,
            super::ExecutionRole::IntegratedServer,
            &Progress::new(),
            &mut |_, _| unreachable!("no menu handoff"),
        )
        .unwrap();
        let mut options = Options::parse(std::iter::empty()).unwrap().unwrap();
        options.resources = root.clone();
        options.execution_role = super::ExecutionRole::IntegratedServer;
        let mut game = Game::new(
            &loaded.resources,
            loaded.pack,
            loaded.gui.unwrap(),
            options,
            settings,
            Audio::silent(&horizon_assets::resource_source::ResourceSource::directory(
                root.to_path_buf(),
            )),
            None,
        )
        .unwrap();

        for (name, hanging) in cases
            .map(|(name, _, hanging)| (name, hanging))
            .into_iter()
            .chain([
                ("minecraft:oak_sign", false),
                ("minecraft:oak_hanging_sign", true),
            ])
        {
            match game.new_block_entity(name) {
                Some(super::BlockEntity::Sign(sign)) => {
                    assert_eq!(sign.hanging, hanging, "{name}")
                }
                entity => panic!("{name}: expected a sign, got {entity:?}"),
            }
        }
        // `createDefaultSignText`: a declared colour and glow on both sides of a new sign, black
        // and unlit on one that declares none.
        let mut text = |name| match game.new_block_entity(name) {
            Some(super::BlockEntity::Sign(sign)) => {
                assert_eq!(sign.front.color, sign.back.color);
                assert_eq!(sign.front.glowing, sign.back.glowing);
                (sign.front.color, sign.front.glowing)
            }
            entity => panic!("{name}: expected a sign, got {entity:?}"),
        };
        assert_eq!(text("ruby:notice_banner"), (3, true));
        assert_eq!(text("ruby:board_shelf"), (15, false));
        assert_eq!(text("minecraft:oak_sign"), (15, false));
        assert!(game.new_block_entity("ruby:road_sign").is_none());
        assert!(matches!(
            game.new_block_entity("minecraft:white_banner"),
            Some(super::BlockEntity::Banner(_))
        ));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// With no window to lay a screen out in there is no slot under the pointer.
    #[test]
    #[ignore = "daily"]
    fn no_item_is_hovered_without_a_window() {
        use super::super::{Audio, Game, Options, Progress, Settings, load_resources};

        let root = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").unwrap());
        let settings = Settings::default();
        let loaded = load_resources(
            horizon_assets::resource_source::ResourceRoot::directory(root.clone()),
            Vec::new(),
            None,
            &root,
            &settings,
            std::time::Instant::now(),
            false,
            crate::game::execution_role::ExecutionRole::IntegratedServer,
            &Progress::new(),
            &mut |_, _| unreachable!("no menu handoff"),
        )
        .unwrap();
        let mut options = Options::parse(std::iter::empty()).unwrap().unwrap();
        options.resources = root.clone();
        let game = Game::new(
            &loaded.resources,
            loaded.pack,
            loaded.gui.unwrap(),
            options,
            settings,
            Audio::silent(&horizon_assets::resource_source::ResourceSource::directory(
                root.to_path_buf(),
            )),
            None,
        )
        .unwrap();
        assert_eq!(game.screen_hovered_item(), None);
    }

    /// A mod's own screen opens only over the world, never over another
    /// screen, an overlay (the title screen, a book, loading, no window) or
    /// another mod's; opening it again is a no-op.
    #[test]
    fn a_mod_screen_opens_only_over_the_world() {
        use super::{Screen, mod_screen_opens};
        assert_eq!(mod_screen_opens(Screen::Playing, false, None, "a"), None);
        assert_eq!(
            mod_screen_opens(Screen::Playing, true, None, "a"),
            Some(false)
        );
        for other in [
            Screen::Paused,
            Screen::Inventory,
            Screen::Chat,
            Screen::Options,
        ] {
            assert_eq!(mod_screen_opens(other, false, None, "a"), Some(false));
        }
        assert_eq!(
            mod_screen_opens(Screen::Mod, false, Some("a"), "a"),
            Some(true)
        );
        assert_eq!(
            mod_screen_opens(Screen::Mod, false, Some("b"), "a"),
            Some(false)
        );
    }

    /// The characters a `ScreenEvent.CharacterTyped.Pre` sees are the
    /// printable ones of a text: a cancelled one leaves that text and a
    /// control character (a key, not text) is never offered a hook.
    #[test]
    fn typed_text_keeps_control_characters_and_drops_cancelled_ones() {
        let mut offered = Vec::new();
        let left = super::screen_text_hooked("a\u{7f}b\u{1b}c", |c| {
            offered.push(c);
            c == 'b'
        });
        assert_eq!(offered, ['a', 'b', 'c']);
        assert_eq!(left.as_deref(), Some("a\u{7f}\u{1b}c"));
        // Every printable character cancelled: no text is left to type.
        assert_eq!(super::screen_text_hooked("ab", |_| true), None);
        // Nothing to offer: an empty text stays empty and keys alone stay whole.
        assert_eq!(super::screen_text_hooked("", |_| true).as_deref(), Some(""));
        assert_eq!(
            super::screen_text_hooked("\u{7f}\t", |_| true).as_deref(),
            Some("\u{7f}\t")
        );
        // What no hook cancels is handed back as it came in.
        assert_eq!(
            super::screen_text_hooked("hello", |_| false).as_deref(),
            Some("hello")
        );
    }

    /// The window's key path: only a key's printable characters go on to the
    /// hook and the text box.
    #[test]
    fn a_key_offers_only_its_printable_characters() {
        // Backspace, Escape and the like are keys, not text.
        assert_eq!(super::chars_to_type("a\u{7f}\u{1b}b", true), "ab");
        // Nothing typed: nothing to hand over.
        assert_eq!(super::chars_to_type("", true), "");
        // Ctrl+A is a shortcut: its `a` reaches no hook and no box.
        assert_eq!(super::chars_to_type("a", false), "");
    }

    /// A mod's camera replaces vanilla's value by value; one that isn't finite keeps vanilla's.
    #[test]
    fn a_mod_camera_replaces_each_finite_value() {
        use super::posed_camera;
        let vanilla = (glam::DVec3::new(1.0, 2.0, 3.0), 10.0, 20.0);
        assert_eq!(
            posed_camera(vanilla, [-40.5, 90.0, 1e7], 270.0, -35.0),
            (glam::DVec3::new(-40.5, 90.0, 1e7), 270.0, -35.0)
        );
        assert_eq!(
            posed_camera(
                vanilla,
                [f64::NAN, 5.0, f64::INFINITY],
                f32::NAN,
                f32::NEG_INFINITY
            ),
            (glam::DVec3::new(1.0, 5.0, 3.0), 10.0, 20.0)
        );
    }

    /// A mod's roll is kept within a turn, however far it turns (no shake bound, no Distortion
    /// Effects scale), and one that isn't finite is none.
    #[test]
    fn a_mod_camera_roll_is_kept_unless_it_isnt_finite() {
        use super::posed_roll;
        for roll in [0.0, 22.5, -22.5, 90.0, -180.0, 359.5] {
            assert_eq!(posed_roll(roll), roll);
        }
        // Whole turns go, keeping the sign of what is left; no roll is +0.
        assert_eq!(posed_roll(382.5), 22.5);
        assert_eq!(posed_roll(-382.5), -22.5);
        assert_eq!(posed_roll(1_000_000_000.0), 1_000_000_000.0 % 360.0);
        assert!(posed_roll(1_000_000_000.0).abs() < 360.0);
        for roll in [720.0, -720.0, -0.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(posed_roll(roll).to_bits(), 0.0f32.to_bits(), "{roll}");
        }
    }

    /// A shake is scaled by Distortion Effects, bounded on each axis, and a
    /// value that isn't finite is none.
    #[test]
    fn a_shake_is_scaled_and_bounded() {
        use super::{Shake, bounded_shake, shake_pose};
        let shake = Shake {
            pitch: 4.0,
            yaw: -30.0,
            roll: f32::NAN,
            offset: [0.2, 2.0, f32::INFINITY],
        };
        assert_eq!(
            bounded_shake(shake, 1.0),
            Shake {
                pitch: 4.0,
                yaw: -10.0,
                roll: 0.0,
                offset: [0.2, 0.5, 0.0],
            }
        );
        assert_eq!(bounded_shake(shake, 0.5).pitch, 2.0);
        assert_eq!(bounded_shake(shake, 0.0), Shake::default());
        assert_eq!(shake_pose(Shake::default()), glam::Mat4::IDENTITY);
    }

    /// A mod's shader values that aren't finite read as 0.
    #[test]
    fn shader_params_that_arent_finite_read_as_zero() {
        let mut params = [[1.0; 4]; 8];
        params[0] = [f32::NAN, 2.0, f32::INFINITY, -3.0];
        let out = super::finite_params(params);
        assert_eq!(out[0], [0.0, 2.0, 0.0, -3.0]);
        assert_eq!(out[7], [1.0; 4]);
    }

    /// A client mod's sound needs a namespaced event, finite values and a
    /// start a minute away at most.
    #[test]
    fn a_local_sound_is_checked() {
        use super::local_sound_ok;
        use mod_api::{SoundPlay, SoundSource};
        let at = |id| SoundPlay::at(id, SoundSource::Hostile, [0.5, 64.0, 0.5]);
        assert!(local_sound_ok(
            &at("ironfall:boom_far").with_volume(8.0),
            140
        ));
        assert!(local_sound_ok(&at("minecraft:entity.generic.explode"), 0));
        assert!(local_sound_ok(
            &SoundPlay::ui("minecraft:ui.button.click"),
            1200
        ));
        assert!(!local_sound_ok(&at("boom"), 0));
        assert!(!local_sound_ok(&at("ironfall:"), 0));
        assert!(!local_sound_ok(&at("Ironfall:Boom"), 0));
        let nowhere = SoundPlay::at("ironfall:boom", SoundSource::Hostile, [f64::NAN, 0.0, 0.0]);
        assert!(!local_sound_ok(&nowhere, 0));
        assert!(!local_sound_ok(&at("ironfall:boom").with_volume(-1.0), 0));
        assert!(!local_sound_ok(
            &at("ironfall:boom").with_pitch(f32::NAN),
            0
        ));
        assert!(!local_sound_ok(&at("ironfall:boom").looping(-1), 0));
        assert!(!local_sound_ok(&at("ironfall:boom"), 1201));
    }

    /// `native_play_sound` takes vanilla's and mods' sound ids, nothing else.
    #[test]
    fn sound_ids_are_identifiers() {
        for id in [
            "minecraft:block.note_block.harp",
            "mymod:machine/hum",
            "my_mod.x-1:a",
        ] {
            assert!(super::sound_id(id), "{id}");
        }
        for id in [
            "",
            "hum",
            ":hum",
            "mymod:",
            "MyMod:hum",
            "mymod:Hum",
            "my/mod:hum",
            "a:b c",
        ] {
            assert!(!super::sound_id(id), "{id}");
        }
    }

    /// A mod's property write: none for a key or value the block lacks, the
    /// state itself when it already has the value, else the new state.
    #[test]
    fn with_property_finds_the_state() {
        let facts = br#"{"defaults":{"minecraft:air":0,"test:crop":1},"states":[
            ["air",true,false,"","minecraft:air"],
            ["crop0",false,false,"",{"id":"test:crop","properties":{"age":"0"}}],
            ["crop1",false,false,"",{"id":"test:crop","properties":{"age":"1"}}]
        ]}"#;
        let registry = horizon_world::registry::BlockRegistry::from_facts_json(&facts[..]).unwrap();
        use horizon_world::registry::BlockStateId;
        let (age0, age1) = (BlockStateId(1), BlockStateId(2));
        assert_eq!(
            super::with_property(&registry, age0, "age", "0"),
            Some(age0)
        );
        assert_eq!(
            super::with_property(&registry, age0, "age", "1"),
            Some(age1)
        );
        assert_eq!(super::with_property(&registry, age0, "age", "9"), None);
        assert_eq!(super::with_property(&registry, age0, "lit", "true"), None);
        // What both property setters write: nothing (false) when the
        // property already has the value, as `Level.setBlock`.
        assert_eq!(super::property_change(&registry, age0, "age", "0"), None);
        assert_eq!(
            super::property_change(&registry, age0, "age", "1"),
            Some(age1)
        );
        assert_eq!(super::property_change(&registry, age0, "age", "9"), None);
        assert_eq!(super::property_change(&registry, age0, "lit", "true"), None);
    }

    /// The state a mod block is placed in is built from its own default state
    /// and one of its properties, its declared ones (`state_properties` in its
    /// block file) included, and keeps the properties it already had.
    #[test]
    fn a_placement_state_is_built_from_a_blocks_own_properties() {
        let facts = br#"{"defaults":{"minecraft:air":0,"test:pipe":1},"states":[
            ["air",true,false,"","minecraft:air"],
            ["north_gas",false,false,"",{"id":"test:pipe","properties":{"carries":"gas","facing":"north"}}],
            ["east_gas",false,false,"",{"id":"test:pipe","properties":{"carries":"gas","facing":"east"}}],
            ["north_items",false,false,"",{"id":"test:pipe","properties":{"carries":"items","facing":"north"}}],
            ["east_items",false,false,"",{"id":"test:pipe","properties":{"carries":"items","facing":"east"}}]
        ]}"#;
        let registry = horizon_world::registry::BlockRegistry::from_facts_json(&facts[..]).unwrap();
        use horizon_world::registry::BlockStateId;
        let (north_gas, east_gas, east_items) = (BlockStateId(1), BlockStateId(2), BlockStateId(4));
        // The declared property changes, and `carries` stays as it was.
        assert_eq!(
            super::with_property(&registry, north_gas, "facing", "east"),
            Some(east_gas)
        );
        assert_eq!(
            super::with_property(&registry, east_gas, "carries", "items"),
            Some(east_items)
        );
        // A value the property does not have builds nothing.
        assert_eq!(
            super::with_property(&registry, east_gas, "facing", "up"),
            None
        );
    }
    /// Both property setters return false for a write that changes nothing:
    /// each goes through `property_change`, none returns true on its own.
    #[test]
    fn property_setters_refuse_no_change() {
        // A Windows checkout has CRLF line ends: the searches below look for "\n".
        let source = include_str!("base.rs").replace("\r\n", "\n");
        let code = source.split("#[cfg(test)]").next().unwrap_or_default();
        for head in [
            "    pub(in crate::game) fn native_set_block_property(\n        &mut self,\n        ticks:",
            "    pub(in crate::game) fn set_block_property_with(",
        ] {
            let body = code.split(head).nth(1).expect(head);
            let body = &body[..body.find("\n    }\n").expect(head)];
            assert!(body.contains("property_change("), "{head}");
            assert!(!body.contains("return true"), "{head}");
        }
    }
    /// A data packet for an old window leaves the open mod screen alone.
    #[test]
    fn mod_screen_data_is_its_windows_only() {
        let mut s = super::OpenNativeScreen {
            layout_revision: 0,
            key: (0, 0, 0),
            window: 4,
            block: "gtmod:machine".into(),
            screen: mod_api::Screen::default(),
            data: Vec::new(),
            extra: None,
        };
        assert!(s.set_data(4, 1, 7));
        assert!(!s.set_data(3, 1, 99));
        assert_eq!(s.data, [0, 7]);
    }

    #[test]
    fn native_breathing_stays_hookable() {
        let source = include_str!("../../breathing.rs").replace("\r\n", "\n");
        let hook = source.find("pub fn native_living_breathe(").unwrap();
        // From its doc comment's first line, so an attribute above it counts.
        let start = source[..hook].find("/// An entity's breathing").unwrap();
        let body = &source[start..hook + source[hook..].find("\n}\n").unwrap()];
        assert!(body.contains("#[inline(never)]"));
        for line in body.lines() {
            assert!(!line.contains("cfg"), "a cfg in the breathing hook: {line}");
            for mac in ["!(", "![", "! {", "!{"] {
                assert!(!line.contains(mac), "a macro in the breathing hook: {line}");
            }
        }
    }

    #[test]
    fn native_loot_kinds_stay_hookable() {
        let source = include_str!("../../../../horizon-world/src/loot.rs").replace("\r\n", "\n");
        for hook in [
            "pub fn native_invoke_loot_function(",
            "pub fn native_invoke_loot_condition(",
        ] {
            let at = source
                .find(hook)
                .unwrap_or_else(|| panic!("{hook} not found"));
            let start = source[..at].rfind("///").unwrap();
            let body = &source[start..start + source[start..].find("\n}\n").unwrap()];
            for line in body.lines() {
                assert!(!line.contains("cfg"), "a cfg in a loot hook point: {line}");
                for mac in ["!(", "![", "! {", "!{"] {
                    assert!(!line.contains(mac), "a macro in a loot hook point: {line}");
                }
            }
        }
    }

    #[test]
    fn native_ingredient_stays_hookable() {
        let source = include_str!("../../crafting/native_hooks.rs").replace("\r\n", "\n");
        let at = source.find("pub fn native_ingredient(").unwrap();
        let start = source[..at].rfind("///").unwrap();
        let body = &source[start..start + source[start..].find("\n}\n").unwrap()];
        for line in body.lines() {
            assert!(
                !line.contains("cfg"),
                "a cfg in the ingredient hook: {line}"
            );
            for mac in ["!(", "![", "! {", "!{"] {
                assert!(!line.contains(mac), "a macro: {line}");
            }
        }
    }

    #[test]
    fn loot_table_load_stays_hookable() {
        let source = include_str!("../../../../horizon-world/src/loot.rs").replace("\r\n", "\n");
        // From its doc comment's last line, so an attribute above it counts.
        let hook = source.find("pub fn native_loot_table_load(").unwrap();
        let start = source[..hook].rfind("///").unwrap();
        let end = source[start..]
            .find("\n}\n")
            .or_else(|| source[start..].find("\r\n}\r\n"))
            .unwrap();
        let body = &source[start..start + end];
        for line in body.lines() {
            assert!(
                !line.contains("cfg"),
                "a cfg in the loot hook point: {line}"
            );
            for mac in ["!(", "![", "! {", "!{"] {
                assert!(
                    !line.contains(mac),
                    "a macro in the loot hook point: {line}"
                );
            }
        }
    }

    #[test]
    fn lifecycle_source_stays_hookable() {
        let source = include_str!("../modding/native_lifecycle.rs");
        let code = source.split("#[cfg(test)]").next().unwrap_or_default();
        for line in code.lines().filter(|l| !l.trim_start().starts_with("//")) {
            assert!(!line.contains("cfg"), "a cfg in lifecycle source: {line}");
            for mac in ["!(", "![", "! {", "!{"] {
                assert!(!line.contains(mac), "a macro in lifecycle source: {line}");
            }
        }
    }

    #[test]
    fn the_damage_hook_names_the_source() {
        use super::{Attack, HurtSource, Hurting, hurt_server_args, mob_source};
        use horizon_client::health::NO_ATTACK;
        use horizon_client::mobs::{Damage, DamageKind as K, Who};
        let source = |kind, attacker| {
            let s = mob_source(
                &Damage {
                    kind,
                    attacker,
                    from: None,
                    attacker_species: None,
                },
                &Attack {
                    attacker: 7,
                    projectile: None,
                    direct: None,
                    player_uuid: None,
                    held: Default::default(),
                    ..Default::default()
                },
            );
            (s.damage_type, s.causing, s.direct)
        };
        let zombie = Some(Who::Mob(3));
        // A registered type of a datapack reaches the hook under its own id.
        let scorch = K::Named(horizon_client::mobs::NamedDamage::new("demo:scorch", 0));
        assert_eq!(source(scorch, None), ("demo:scorch", 0, 0));
        assert_eq!(source(scorch, zombie), ("demo:scorch", 3, 0));
        assert_eq!(source(K::MobAttack, zombie), ("minecraft:mob_attack", 3, 3));
        assert_eq!(source(K::Thorns, zombie), ("minecraft:thorns", 3, 3));
        assert_eq!(source(K::Arrow, zombie), ("minecraft:arrow", 3, 0));
        let during = |kind| {
            let s = mob_source(
                &Damage {
                    kind,
                    attacker: zombie,
                    from: None,
                    attacker_species: None,
                },
                &Attack {
                    attacker: 7,
                    projectile: Some(912),
                    direct: None,
                    player_uuid: None,
                    held: Default::default(),
                    ..Default::default()
                },
            );
            (s.damage_type, s.direct)
        };
        // The impact's projectile is the direct entity only when the type names it.
        assert_eq!(during(K::Arrow), ("minecraft:arrow", 912));
        assert_eq!(during(K::WitherSkull), ("minecraft:wither_skull", 912));
        assert_eq!(during(K::IndirectMagic), ("minecraft:indirect_magic", 912));
        // magic() and a blast do not, even while that projectile's attack is in scope.
        assert_eq!(during(K::Magic), ("minecraft:magic", 0));
        assert_eq!(during(K::Explosion), ("minecraft:player_explosion", 0));
        let blast = source(K::Explosion, zombie);
        assert_eq!(blast, ("minecraft:player_explosion", 3, 0));
        // A player behind a mob's hit is the one whose attack the level lends; a fall has no
        // source.
        let hit = source(K::PlayerAttack, Some(Who::Player));
        assert_eq!(hit, ("minecraft:player_attack", 7, 7));
        assert_eq!(source(K::Fall, None), ("minecraft:fall", 0, 0));
        let arrow = HurtSource {
            damage_type: "minecraft:arrow",
            causing: 4,
            direct: 0,
            position: None,
        };
        let player = (2, "minecraft:player", arrow);
        assert_eq!(
            hurt_server_args(Hurting::Player(2, arrow), &NO_ATTACK),
            player
        );
        let dragon = (5, "minecraft:ender_dragon", arrow);
        assert_eq!(
            hurt_server_args(Hurting::Dragon(5, arrow), &NO_ATTACK),
            dragon
        );
        assert_eq!(
            hurt_server_args(Hurting::Entity(6, "minecraft:item", arrow), &NO_ATTACK),
            (6, "minecraft:item", arrow)
        );
    }

    /// A mod container opens a slot grid or a furnace's menu, by its slot
    /// count; no menu with rules of its own (a brewing stand's, a crafter's).
    #[test]
    fn mod_menus_are_slot_grids_and_furnaces() {
        use super::mod_menu;
        assert_eq!(mod_menu("minecraft:hopper"), Some(("minecraft:hopper", 5)));
        assert_eq!(mod_menu("minecraft:generic_9x3").map(|m| m.1), Some(27));
        assert_eq!(mod_menu("minecraft:furnace").map(|m| m.1), Some(3));
        assert_eq!(mod_menu("minecraft:brewing_stand"), None);
        assert_eq!(mod_menu("hopper"), None);
    }

    /// The config folder is `config/` beside `options.txt`, made when
    /// missing.
    #[test]
    fn config_dir_is_config_in_the_game_folder() {
        use super::config_dir;
        let root = std::env::temp_dir().join(format!("horizon-config-dir-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        assert!(!root.join("config").exists());
        let dir = config_dir(&root).unwrap();
        assert_eq!(dir, root.join("config"));
        assert!(dir.is_dir());
        assert_eq!(config_dir(&root).unwrap(), dir);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Vanilla's F3 draws a column in each top corner (`extractLines`, left
    /// and right aligned), and its charts along the bottom.
    #[test]
    fn the_debug_screen_takes_both_top_corners_and_its_charts_corners() {
        use super::{Corner, debug_takes};
        for corner in Corner::ALL {
            assert!(!debug_takes(corner, false, [true, true]), "{corner:?}");
        }
        assert!(debug_takes(Corner::TopLeft, true, [false, false]));
        assert!(debug_takes(Corner::TopRight, true, [false, false]));
        assert!(!debug_takes(Corner::BottomLeft, true, [false, false]));
        assert!(!debug_takes(Corner::BottomRight, true, [false, false]));
        assert!(debug_takes(Corner::BottomLeft, true, [true, false]));
        assert!(!debug_takes(Corner::BottomRight, true, [true, false]));
        assert!(debug_takes(Corner::BottomRight, true, [false, true]));
    }

    /// An image is taken whole, and only when it fits its slot.
    #[test]
    fn a_mod_image_is_taken_whole_and_only_when_it_fits() {
        use super::mod_image_fits;
        assert!(mod_image_fits(4, 2, 32, 1024, 1024));
        assert!(mod_image_fits(1024, 1024, 1024 * 1024 * 4, 1024, 1024));
        assert!(!mod_image_fits(0, 2, 0, 1024, 1024));
        assert!(!mod_image_fits(4, 2, 31, 1024, 1024));
        assert!(!mod_image_fits(4, 2, 33, 1024, 1024));
        assert!(!mod_image_fits(2048, 1, 2048 * 4, 1024, 1024));
    }
}

#[cfg(test)]
mod item_fall_on_tests {
    use super::*;
    use mod_api::{BlockPart, FallOnEvent, Hooks, LevelRandomHost, ModEntry, Registrar};
    use mod_platform::testing::with_mods;
    use std::sync::{Arc, Mutex};

    /// What the part on stone heard of a landing: entity, kind, block, fall and damage. It
    /// asks for the block to be destroyed.
    #[derive(Clone, Default)]
    struct Lands(Arc<Mutex<Vec<(i32, String, BlockPos, f64, f32)>>>);
    #[mod_api::horizon_part]
    impl BlockPart for Lands {
        fn fall_on(&self, _: &mut dyn LevelRandomHost, event: &mut FallOnEvent<'_>) {
            self.0.lock().unwrap().push((
                event.entity,
                event.kind.to_owned(),
                event.pos,
                event.distance,
                event.damage,
            ));
            event.request(mod_api::LandingWrite::DestroyBlock { drop: false });
        }
    }
    struct Mod(Lands);
    #[mod_api::__subscriptions]
    impl Hooks for Mod {
        fn register(&self, r: &mut dyn Registrar, _: &mod_api::RegistrationContext) {
            let block_key = r.block_key("minecraft:stone");
            r.block_part(block_key, self.0.clone());
        }
    }

    static LOAN_VALUE: mod_api::AttachmentType<i32> = mod_api::AttachmentType {
        namespace: "loan_test",
        name: "progress",
        default: || 0,
        codec: None,
        copy_on_death: false,
        sync: false,
    };

    use horizon_core::random::RandomSource as _;

    struct LoanPart(std::sync::Arc<()>);
    #[mod_api::horizon_part]
    impl BlockPart for LoanPart {
        fn fall_on(&self, host: &mut dyn LevelRandomHost, event: &mut FallOnEvent<'_>) {
            let mut random = host.random().expect("authoritative landing random");
            assert_eq!(random.next_int(), 1553932502);
            assert!(host.set_random(random));
            LOAN_VALUE
                .set(host.attachments(), mod_api::Holder::Level, 73)
                .unwrap();
            event.request(mod_api::LandingWrite::DestroyBlock { drop: false });
            std::panic::panic_any(self.0.clone());
        }
    }
    struct LoanMod(std::sync::Arc<()>);
    #[mod_api::__subscriptions]
    impl Hooks for LoanMod {
        fn register(&self, r: &mut dyn Registrar, _: &mod_api::RegistrationContext) {
            let key = r.block_key("minecraft:stone");
            r.block_part(key, LoanPart(self.0.clone()));
        }
    }

    /// Only the inner production loan is exercised: the outer dropped-item pass
    /// is deliberately not claimed to survive this unwind.
    #[test]
    #[ignore = "daily"]
    fn item_fall_on_returns_progressed_level_loan_after_part_panic() {
        let payload = std::sync::Arc::new(());
        let mods: mod_api::Mods = vec![ModEntry::new(
            "loan_test",
            Box::new(LoanMod(payload.clone())),
        )]
        .into();
        crate::game::api_values::install_parts(&mods).unwrap();
        let _ = horizon_client::block_contact::HOOKS.set(BLOCK_CONTACT_HOOKS);
        with_mods(mods, || {
            let mut loaded = 0;
            for dimension in [crate::game::Dim::Overworld, crate::game::Dim::Nether] {
                let scratch = std::env::temp_dir()
                    .join(format!("item-loan-{}-{dimension:?}", std::process::id()));
                super::super::native_entity_refs::tests::with_game(|game| {
                    game.world_root = scratch.clone();
                    game.enter_level(dimension).unwrap();
                    loaded += 1;
                    let stone = game
                        .pack
                        .assets
                        .registry
                        .default_state("minecraft:stone")
                        .unwrap();
                    game.world
                        .chunks
                        .insert((0, 0), Arc::new(horizon_client::chunk::Chunk::empty(0, 0)));
                    let pos = BlockPos::new(1, 64, 1);
                    game.world.set(pos.x, pos.y, pos.z, stone).unwrap();
                    *game.world.random = horizon_core::random::LegacyRandom::new(12345);
                    LOAN_VALUE
                        .set(&mut game.world.attachments, mod_api::Holder::Level, 11)
                        .unwrap();
                    game.world.attachment_syncs.insert(mod_api::Holder::Level);
                    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        horizon_client::block_contact::in_level(Some(game.dim.id()), || {
                            game.item_fall_on(42, pos, 5.0);
                        });
                    }))
                    .expect_err("actual Part must panic");
                    assert!(std::sync::Arc::ptr_eq(
                        &payload,
                        caught.downcast_ref::<std::sync::Arc<()>>().unwrap()
                    ));
                    assert_eq!(
                        game.world.random.next_int(),
                        -2090749135,
                        "continue the progressed stream"
                    );
                    assert_eq!(
                        LOAN_VALUE
                            .get(&mut game.world.attachments, mod_api::Holder::Level)
                            .unwrap(),
                        73
                    );
                    assert!(
                        game.world
                            .attachment_syncs
                            .contains(&mod_api::Holder::Level)
                    );
                    game.drain_entity_inside();
                    assert_eq!(
                        game.world.block(pos.x, pos.y, pos.z),
                        stone,
                        "aborted landing writes were not queued"
                    );
                });
                let _ = std::fs::remove_dir_all(scratch);
            }
            assert_eq!(loaded, 2, "both resource fixtures ran");
        });
    }

    /// A dropped item that falls onto a block with a `fall_on` part is told to it once, as
    /// `minecraft:item`, with the whole fall and no damage; the write the part asks for lands
    /// after the items' batch; and an item resting there lands no more.
    #[test]
    #[ignore = "daily"]
    fn a_dropped_item_lands_on_a_fall_on_part() {
        let heard = Lands::default();
        let mods: mod_api::Mods = vec![ModEntry::new("lands", Box::new(Mod(heard.clone())))].into();
        crate::game::api_values::install_parts(&mods).unwrap();
        let _ = horizon_client::block_contact::HOOKS.set(BLOCK_CONTACT_HOOKS);
        with_mods(mods, || {
            super::super::native_entity_refs::tests::with_game(|game| {
                game.options.dedicated = Some(Vec::new());
                let state = |name| game.pack.assets.registry.default_state(name).unwrap();
                let (stone, air) = (state("minecraft:stone"), state("minecraft:air"));
                for x in -1..=1 {
                    for z in -1..=1 {
                        let mut chunk = horizon_client::chunk::Chunk::empty(x, z);
                        chunk.lit = true;
                        game.world.chunks.insert((x, z), std::sync::Arc::new(chunk));
                    }
                }
                game.world.set(1, 64, 1, stone).unwrap();
                let level = game.level(game.dim.id()).unwrap();
                let stack = ItemStack::new("minecraft:apple", 1);
                let item = game
                    .level_spawn_item(level, 1.5, 70.0, 1.5, stack, 10)
                    .unwrap();
                if let Some(i) = game.dropped.iter_mut().find(|i| i.id == item.id()) {
                    i.velocity = horizon_core::geometry::Vec3::ZERO;
                }
                // The part's write destroys the stone after the batch; it is put back at once,
                // so the item rests on it again and must not be heard landing twice.
                let mut destructions = 0;
                for _ in 0..80 {
                    game.ticked_entities_through = u32::MAX;
                    horizon_client::block_contact::in_level(Some(game.dim.id()), || {
                        game.tick_items();
                    });
                    if game.world.block(1, 64, 1) == air {
                        destructions += 1;
                        game.world.set(1, 64, 1, stone).unwrap();
                        let heard = heard.0.lock().unwrap();
                        assert_eq!(heard.len(), 1, "the write landed after the batch");
                    }
                }
                assert_eq!(destructions, 1, "one observed landing-write destruction");
                let heard = heard.0.lock().unwrap().clone();
                assert_eq!(heard.len(), 1, "one landing: {heard:?}");
                let (entity, kind, pos, distance, damage) = &heard[0];
                assert_eq!(*entity, item.id() as i32);
                assert_eq!(kind, "minecraft:item");
                assert_eq!(*pos, BlockPos::new(1, 64, 1));
                // From rest at y 70 onto the stone's top at 65.
                assert!((distance - 5.0).abs() < 1e-3, "fell {distance}");
                assert_eq!(*damage, 0.0);
            });
        });
    }
}
