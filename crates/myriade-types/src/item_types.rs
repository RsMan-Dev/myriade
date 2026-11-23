use crate::*;


/// An item requested at registration; its load-owned slot identifies the canonical name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ItemKey(pub u32);

/// One armor slot of an `ArmorHurtEvent`: the piece in `slot`, the durability
/// damage vanilla works out for it (`damage`, `max(1, hit / 4)`) and the damage it
/// takes (`new_damage`, starts equal; a mod may change it, below 1 for none; the
/// game casts it to a whole number as vanilla does).
#[derive(Clone, Debug, PartialEq)]
pub struct ArmorHurt {
    /// The equipment slot the armor piece sits in.
    pub slot: EquipmentSlot,
    /// The armor piece being damaged.
    pub stack: ItemStack,
    /// The durability damage vanilla computed.
    pub damage: f32,
    /// The durability damage to apply instead; starts equal to `damage`.
    pub new_damage: f32,
}

/// An item that drops where a living entity died or a block broke, before it is
/// an item entity in the level (`living_drops`, `block_drops`): what `ItemEntity`'s
/// constructor was given, and the item-entity state it joins the level with.
#[derive(Clone, Debug, PartialEq)]
pub struct ItemDrop {
    /// The dropped stack.
    pub stack: ItemStack,
    /// Where the item entity appears, in blocks.
    pub position: [f64; 3],
    /// Its motion in blocks per tick, as vanilla's constructor drew it.
    pub velocity: [f64; 3],
    /// Ticks before a player may pick it up.
    pub pickup_delay: u32,
    /// `ItemEntity.setThrower`: the UUID of the entity that threw it, none by default.
    pub thrower: Option<u128>,
    /// `ItemEntity.setTarget`: the UUID of the only player who may pick it up, none (anyone)
    /// by default.
    pub target: Option<u128>,
    /// `Entity.setNoGravity`: the item entity floats where it moves; false by default.
    pub no_gravity: bool,
    /// The item entity's starting `age` in ticks, 0 by default. It despawns at 6000;
    /// [`ItemDrop::UNLIMITED_LIFETIME`] (vanilla's `setUnlimitedLifetime`) never does.
    pub age: i32,
}

impl ItemDrop {
    /// A drop of `stack` at `position` with a `velocity` and `pickup_delay`.
    pub fn new(
        stack: ItemStack,
        position: [f64; 3],
        velocity: [f64; 3],
        pickup_delay: u32,
    ) -> Self {
        Self {
            stack,
            position,
            velocity,
            pickup_delay,
            thrower: None,
            target: None,
            no_gravity: false,
            age: 0,
        }
    }
    /// The `age` that keeps an item entity from despawning (`ItemEntity.setUnlimitedLifetime`).
    pub const UNLIMITED_LIFETIME: i32 = -32768;
    /// The same drop, thrown by `thrower` (`ItemEntity.setThrower`).
    pub fn with_thrower(self, thrower: Option<u128>) -> Self {
        Self { thrower, ..self }
    }
    /// The same drop, which only `target` may pick up (`ItemEntity.setTarget`).
    pub fn with_target(self, target: Option<u128>) -> Self {
        Self { target, ..self }
    }
    /// The same drop, with or without gravity (`Entity.setNoGravity`).
    pub fn with_no_gravity(self, no_gravity: bool) -> Self {
        Self { no_gravity, ..self }
    }
    /// The same drop, starting at `age` ticks ([`ItemDrop::UNLIMITED_LIFETIME`] never despawns).
    pub fn with_age(self, age: i32) -> Self {
        Self { age, ..self }
    }
}

/// The slots an item's attribute modifier applies in (vanilla's
/// `EquipmentSlotGroup`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EquipmentSlotGroup {
    /// `any`: every slot.
    Any,
    /// `mainhand`.
    MainHand,
    /// `offhand`.
    OffHand,
    /// `hand`: either hand.
    Hand,
    /// `feet`.
    Feet,
    /// `legs`.
    Legs,
    /// `chest`.
    Chest,
    /// `head`.
    Head,
    /// `armor`: feet, legs, chest, head and body.
    Armor,
    /// `body`: an animal's armor slot.
    Body,
    /// `saddle`.
    Saddle,
}

/// One attribute modifier an item stack grants (vanilla's
/// `ItemAttributeModifiers.Entry`): the attribute (`minecraft:armor`), the
/// modifier, and the slots it applies in.
#[derive(Clone, Debug, PartialEq)]
pub struct ItemAttributeModifier {
    /// The attribute's id.
    pub attribute: Identifier,
    /// The modifier.
    pub modifier: AttributeModifier,
    /// The slots it applies in.
    pub slot: EquipmentSlotGroup,
}

impl ItemAttributeModifier {
    /// `modifier` on `attribute`, in the slots of `slot`.
    pub fn new(
        attribute: Identifier,
        modifier: AttributeModifier,
        slot: EquipmentSlotGroup,
    ) -> Self {
        Self {
            attribute,
            modifier,
            slot,
        }
    }
}

/// The attribute modifiers of one item stack, as `item_attribute_modifier`
/// edits them (`ItemAttributeModifierEvent`'s list). It starts as the
/// stack's own `minecraft:attribute_modifiers`, or its item's, in order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ItemAttributeModifiers {
    pub entries: Vec<ItemAttributeModifier>,
    pub changed: bool,
}

impl ItemAttributeModifiers {
    /// The modifiers, in order.
    pub fn iter(&self) -> std::slice::Iter<'_, ItemAttributeModifier> {
        self.entries.iter()
    }
    /// How many modifiers there are.
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    /// `addModifier`: adds `modifier` on `attribute` in the slots of `slot`,
    /// unless `attribute` already has a modifier with its id. Whether it
    /// was added.
    pub fn add(
        &mut self,
        attribute: Identifier,
        modifier: AttributeModifier,
        slot: EquipmentSlotGroup,
    ) -> bool {
        if self.find(&attribute, &modifier.id).is_some() {
            return false;
        }
        self.entries
            .push(ItemAttributeModifier::new(attribute, modifier, slot));
        self.changed = true;
        true
    }
    /// `replaceModifier`: puts `modifier` on `attribute` in the slots of
    /// `slot`, in place of the one with its id, or at the end.
    pub fn replace(
        &mut self,
        attribute: Identifier,
        modifier: AttributeModifier,
        slot: EquipmentSlotGroup,
    ) {
        let entry = ItemAttributeModifier::new(attribute, modifier, slot);
        match self.find(&entry.attribute, &entry.modifier.id) {
            Some(i) => self.entries[i] = entry,
            None => self.entries.push(entry),
        }
        self.changed = true;
    }
    /// `removeModifier`: removes the modifier `id` of `attribute`. Whether
    /// there was one.
    pub fn remove(&mut self, attribute: &Identifier, id: &Identifier) -> bool {
        let Some(i) = self.find(attribute, id) else {
            return false;
        };
        self.entries.remove(i);
        self.changed = true;
        true
    }
    /// `removeIf`: keeps only the modifiers `keep` accepts. Whether any went.
    pub fn retain(&mut self, mut keep: impl FnMut(&ItemAttributeModifier) -> bool) -> bool {
        let len = self.entries.len();
        self.entries.retain(|entry| keep(entry));
        let removed = self.entries.len() != len;
        self.changed |= removed;
        removed
    }
    /// `removeAllModifiersFor`: removes every modifier of `attribute`.
    /// Whether any went.
    pub fn remove_all_for(&mut self, attribute: &Identifier) -> bool {
        self.retain(|entry| entry.attribute != *attribute)
    }
    /// `clearModifiers`: removes every modifier.
    pub fn clear(&mut self) {
        self.changed |= !self.entries.is_empty();
        self.entries.clear();
    }
    pub fn find(&self, attribute: &Identifier, id: &Identifier) -> Option<usize> {
        self.entries
            .iter()
            .position(|e| e.attribute == *attribute && e.modifier.id == *id)
    }
}

/// The enchantments `get_enchantment_level` (`GetEnchantmentLevelEvent`)
/// hands a mod: NeoForge's `ItemEnchantments.Mutable`, each enchantment's id
/// (`minecraft:sharpness`) with its level, in order. Private fields, edited
/// through methods, so the game knows whether a mod changed them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ItemEnchantments {
    pub entries: Vec<(Identifier, u32)>,
    pub changed: bool,
}

impl ItemEnchantments {
    /// The highest level an enchantment can have (`ItemEnchantments.MAX_LEVEL`).
    pub const MAX_LEVEL: u32 = 255;

    /// The enchantments and their levels, in order (`keySet` and `getLevel`).
    pub fn iter(&self) -> impl Iterator<Item = (&Identifier, u32)> {
        self.entries.iter().map(|(id, level)| (id, *level))
    }
    /// How many enchantments there are.
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    /// `getLevel`: `enchantment`'s level, 0 when it isn't there.
    pub fn level(&self, enchantment: &Identifier) -> u32 {
        self.find(enchantment).map_or(0, |i| self.entries[i].1)
    }
    /// `set`: gives `enchantment` `level`, at most [`Self::MAX_LEVEL`]; 0
    /// removes it.
    pub fn set(&mut self, enchantment: Identifier, level: u32) {
        let level = level.min(Self::MAX_LEVEL);
        match (self.find(&enchantment), level) {
            (Some(i), 0) => {
                self.entries.remove(i);
            }
            (Some(i), _) if self.entries[i].1 == level => return,
            (Some(i), _) => self.entries[i].1 = level,
            (None, 0) => return,
            (None, _) => self.entries.push((enchantment, level)),
        }
        self.changed = true;
    }
    /// `upgrade`: raises `enchantment` to `level` (at most
    /// [`Self::MAX_LEVEL`]) when that is higher than its own; 0 does nothing.
    pub fn upgrade(&mut self, enchantment: Identifier, level: u32) {
        if level > self.level(&enchantment) {
            self.set(enchantment, level);
        }
    }
    /// `removeIf`: keeps only the enchantments `keep` accepts. Whether any
    /// went.
    pub fn retain(&mut self, mut keep: impl FnMut(&Identifier, u32) -> bool) -> bool {
        let len = self.entries.len();
        self.entries.retain(|(id, level)| keep(id, *level));
        let removed = self.entries.len() != len;
        self.changed |= removed;
        removed
    }
    pub fn find(&self, enchantment: &Identifier) -> Option<usize> {
        self.entries.iter().position(|(id, _)| id == enchantment)
    }
}

/// `ItemDisplayContext`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ItemDisplayContext {
    /// No transform; the model as authored.
    None,
    /// Held in the left hand, seen from outside.
    ThirdPersonLeftHand,
    /// Held in the right hand, seen from outside.
    ThirdPersonRightHand,
    /// Held in the left hand, seen by its owner.
    FirstPersonLeftHand,
    /// Held in the right hand, seen by its owner.
    FirstPersonRightHand,
    /// Worn on the head.
    Head,
    /// Drawn in an inventory slot.
    Gui,
    /// Dropped as an item entity.
    Ground,
    /// Hung in an item frame.
    Fixed,
    /// Standing on a shelf.
    OnShelf,
}

impl ItemDisplayContext {
    /// Every context, in ordinal order (vanilla's id is the index).
    pub const ALL: [Self; 10] = [
        Self::None,
        Self::ThirdPersonLeftHand,
        Self::ThirdPersonRightHand,
        Self::FirstPersonLeftHand,
        Self::FirstPersonRightHand,
        Self::Head,
        Self::Gui,
        Self::Ground,
        Self::Fixed,
        Self::OnShelf,
    ];
    /// `getSerializedName`: `none`, `thirdperson_lefthand`, ... `on_shelf`.
    pub fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::ThirdPersonLeftHand => "thirdperson_lefthand",
            Self::ThirdPersonRightHand => "thirdperson_righthand",
            Self::FirstPersonLeftHand => "firstperson_lefthand",
            Self::FirstPersonRightHand => "firstperson_righthand",
            Self::Head => "head",
            Self::Gui => "gui",
            Self::Ground => "ground",
            Self::Fixed => "fixed",
            Self::OnShelf => "on_shelf",
        }
    }
    /// The context with that name, or None for one vanilla doesn't have.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.name() == name)
    }
}

/// Tool action requested by a block interaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolAbility {
    /// An axe strips a log.
    AxeStrip,
    /// An axe scrapes oxidation off copper.
    AxeScrape,
    /// An axe removes wax from copper.
    AxeWaxOff,
    /// A shovel makes a path block.
    ShovelFlatten,
    /// A shovel puts out a campfire.
    ShovelDouse,
    /// A hoe tills soil.
    HoeTill,
}

/// The behaviour about to run in `UseItemOnBlockEvent`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsePhase {
    /// `IItemExtension.onItemUseFirst`, before the item's Part and the
    /// block's behaviour.
    ItemBeforeBlock,
    /// `BlockBehaviour.useItemOn`, before the block's Part and template.
    Block,
    /// `Item.useOn`, after the block passed and before the item's Part.
    ItemAfterBlock,
}

/// Vanilla's `InteractionResult`, as a hook answers it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InteractionResult {
    /// `SUCCESS`: the action is used and the hand swings on the clicking
    /// client (`SwingSource.PREDICTED`).
    Success,
    /// `SUCCESS_SERVER`: the action is used and the server swings the hand.
    SuccessServer,
    /// `CONSUME`: the action is used, with no swing.
    Consume,
    /// `FAIL`: nothing happens and the click ends.
    Fail,
    /// `PASS`: nothing happens and the click goes on.
    #[default]
    Pass,
    /// `TRY_WITH_EMPTY_HAND`: for a block's `use_item_on`, try its
    /// `use_without_item` next, on the main hand only.
    TryWithEmptyHand,
}

impl InteractionResult {
    /// `consumesAction`: the click is used up.
    pub fn consumes_action(self) -> bool {
        matches!(self, Self::Success | Self::SuccessServer | Self::Consume)
    }
}
