use crate::*;
use std::{fmt::Debug, marker::PhantomData};

/// Where a stack lives.
#[derive(Debug, PartialEq, Eq)]
pub enum ItemPlace {
    /// Slot `slot` of a player's inventory, by `inventory_tick`'s numbers: 0..36 (0..9
    /// the hotbar), the armour 100..=103 (feet to head), the off hand -106
    /// (`ItemAccess.forPlayerSlot`).
    PlayerSlot {
        /// The player's UUID.
        player: u128,
        /// The slot number, as listed above.
        slot: i32,
    },
    /// What a player holds in `hand` (`forPlayerInteraction`).
    PlayerHand {
        /// The player's UUID.
        player: u128,
        /// Which hand.
        hand: Hand,
    },
    /// The stack on the cursor of a player's open menu, `menu` (`forPlayerCursor`): nothing
    /// once the menu is closed, replaced or its player gone.
    PlayerCursor {
        /// The player's open menu.
        menu: MenuRef,
    },
    /// Slot `slot` of the block entity at `pos` as `side` reaches it, through its [`ITEMS`]
    /// handler (`forHandlerIndex`).
    BlockEntity {
        /// The level the block entity is in.
        level: LevelRef,
        /// The block entity's position.
        pos: [i32; 3],
        /// The face the handler is reached from; `None` for no particular side.
        side: Option<Direction>,
        /// The slot's index in the handler.
        slot: usize,
    },
    /// The stack of item entity `id` in the level in place.
    ItemEntity {
        /// The item entity's network id.
        id: u32,
    },
    /// Slot `slot` of the inventory of entity `entity` in the level in place: a chest or hopper
    /// minecart's, a chest boat's, or a chested donkey's, mule's or llama's
    /// (`Capabilities.Item.ENTITY` through `forHandlerIndex`).
    EntitySlot {
        /// The entity's network id.
        entity: u32,
        /// The slot's index in the entity's inventory.
        slot: usize,
    },
    /// Slot `slot` of living entity `entity`'s equipment (NeoForge's
    /// `LivingEntityEquipmentWrapper` through `Capabilities.Item.ENTITY`), reached through
    /// [`ENTITY_ITEMS`](crate::ENTITY_ITEMS). Its write is the equipment write
    /// `LivingRef::set_equipment` makes, so the equip and unequip game event, sound and
    /// attribute updates are the same, and a transaction that aborts puts it back.
    Equipment {
        /// The entity's network id.
        entity: u32,
        /// The equipment slot.
        slot: EquipmentSlot,
    },
    /// Slot `slot` of the item at `parent`, through its [`ITEM_ITEMS`] handler: a backpack in a
    /// backpack.
    Nested {
        /// The place of the stack that holds the container.
        parent: Box<ItemPlace>,
        /// The slot's index inside that stack's container.
        slot: usize,
    },
    /// Slot `slot` of a player's open menu, `menu`, a fork addition (NeoForge 26.3 has no
    /// `ItemAccess` for a menu slot): the storage the slot shows ([`MenuRef::slot_backing`]),
    /// its own slot only, under the slot's own rules first ([`MenuRef::slot_holds`],
    /// [`MenuRef::slot_gives`]). Nothing where the menu is closed, replaced or its player gone;
    /// nothing goes into or out of the crafting result.
    MenuSlot {
        /// The open menu.
        menu: MenuRef,
        /// The slot's index in that menu.
        slot: usize,
    },
    /// A place a mod keeps, such as an accessory slot: read and written by the [`PlaceProvider`]
    /// it registered as `provider`, which alone knows what `key` means.
    Mod {
        /// The provider's registered name.
        provider: &'static str,
        /// The provider's own key for the place.
        key: u64,
    },
    /// A stack outside the world (`ItemAccess.forStack`): a crafting result, a preview, a stack
    /// about to be given. [`ItemsServerHost::scratch_stack`](crate::ItemsServerHost::scratch_stack)
    /// makes one in a transaction and
    /// [`scratch_take`](crate::ItemsServerHost::scratch_take) takes it back out. It
    /// lives until the outermost transaction closes; an abort puts it back as it was, or drops
    /// it if the aborted transaction made it. A write never changes its item, only its count and
    /// components, as `StackItemAccess`: what doesn't fit, or is another item, is refused.
    Scratch(u64),
    /// The place inside, where an insert goes into that slot only
    /// (`ItemAccess.forHandlerIndexStrict`): around a [`BlockEntity`](Self::BlockEntity),
    /// [`EntitySlot`](Self::EntitySlot) or [`Nested`](Self::Nested) slot, what doesn't fit is
    /// refused instead of spread to the other slots. Around any other place it is that place.
    Strict(Box<ItemPlace>),
    /// One item at a time of the place inside (`ItemAccess.oneByOne`): its count reads at most
    /// 1 and an extract takes at most 1, so a handler acting on the whole stack acts on one.
    /// Inserts go to the place inside as they would.
    OneByOne(Box<ItemPlace>),
    /// The place inside, as a player with infinite materials reaches it
    /// (`ItemAccess.forInfiniteMaterials`): while `player` has them (creative's `instabuild`),
    /// an extract gives without taking and an insert is taken whole, one of it going to the
    /// player only if they have none: into their inventory, or dropped at the root commit when
    /// it is full. Inserting nothing gives nothing. Otherwise it is the place inside.
    ///
    /// Wrappers may nest to any depth in a mod's own code: reading and writing undo them in a
    /// loop. One crossing the wasm boundary nests at most 15 wrappers deep (the wire's 32
    /// levels, two for each); a deeper one is refused there.
    InfiniteMaterials {
        /// The player's UUID.
        player: u128,
        /// The place the player reaches.
        inner: Box<ItemPlace>,
    },
    /// A stack outside the world whose item may change: a private two-slot copy, as
    /// `DispenseFluidContainer` runs a dispensed container, so an empty bucket can become a
    /// full one. [`ItemsServerHost::scratch_slots`](crate::ItemsServerHost::scratch_slots)
    /// makes one in a transaction with the stack in its first slot, and
    /// [`scratch_take_slots`](crate::ItemsServerHost::scratch_take_slots) takes both
    /// slots back out. Reads and extracts are the first slot's; an insert fills the first slot
    /// where it is empty or holds the same item and components, and what doesn't fit goes to
    /// the second (`forHandlerIndex`), as a changed item split off a stack does. It lives and
    /// aborts as a [`Scratch`](Self::Scratch) stack does.
    ScratchSlots(u64),
}

impl Clone for ItemPlace {
    fn clone(&self) -> Self {
        enum Link {
            Strict,
            One,
            Infinite(u128),
            Nested(usize),
        }
        let mut links = Vec::new();
        let mut current = self;
        let mut cloned = loop {
            match current {
                Self::Strict(inner) => {
                    links.push(Link::Strict);
                    current = inner;
                }
                Self::OneByOne(inner) => {
                    links.push(Link::One);
                    current = inner;
                }
                Self::InfiniteMaterials { player, inner } => {
                    links.push(Link::Infinite(*player));
                    current = inner;
                }
                Self::Nested { parent, slot } => {
                    links.push(Link::Nested(*slot));
                    current = parent;
                }
                Self::PlayerSlot { player, slot } => {
                    break Self::PlayerSlot {
                        player: *player,
                        slot: *slot,
                    };
                }
                Self::PlayerHand { player, hand } => {
                    break Self::PlayerHand {
                        player: *player,
                        hand: *hand,
                    };
                }
                Self::PlayerCursor { menu } => break Self::PlayerCursor { menu: *menu },
                Self::BlockEntity {
                    level,
                    pos,
                    side,
                    slot,
                } => {
                    break Self::BlockEntity {
                        level: *level,
                        pos: *pos,
                        side: *side,
                        slot: *slot,
                    };
                }
                Self::ItemEntity { id } => break Self::ItemEntity { id: *id },
                Self::EntitySlot { entity, slot } => {
                    break Self::EntitySlot {
                        entity: *entity,
                        slot: *slot,
                    };
                }
                Self::Equipment { entity, slot } => {
                    break Self::Equipment {
                        entity: *entity,
                        slot: *slot,
                    };
                }
                Self::MenuSlot { menu, slot } => {
                    break Self::MenuSlot {
                        menu: *menu,
                        slot: *slot,
                    };
                }
                Self::Mod { provider, key } => {
                    break Self::Mod {
                        provider,
                        key: *key,
                    };
                }
                Self::Scratch(id) => break Self::Scratch(*id),
                Self::ScratchSlots(id) => break Self::ScratchSlots(*id),
            }
        };
        for link in links.into_iter().rev() {
            cloned = match link {
                Link::Strict => Self::Strict(Box::new(cloned)),
                Link::One => Self::OneByOne(Box::new(cloned)),
                Link::Infinite(player) => Self::InfiniteMaterials {
                    player,
                    inner: Box::new(cloned),
                },
                Link::Nested(slot) => Self::Nested {
                    parent: Box::new(cloned),
                    slot,
                },
            };
        }
        cloned
    }
}

impl Drop for ItemPlace {
    fn drop(&mut self) {
        let mut current = match self {
            Self::Strict(inner)
            | Self::OneByOne(inner)
            | Self::InfiniteMaterials { inner, .. }
            | Self::Nested { parent: inner, .. } => {
                std::mem::replace(inner, Box::new(Self::Scratch(0)))
            }
            _ => return,
        };
        while let Self::Strict(inner)
        | Self::OneByOne(inner)
        | Self::InfiniteMaterials { inner, .. }
        | Self::Nested { parent: inner, .. } = &mut *current
        {
            let next = std::mem::replace(inner, Box::new(Self::Scratch(0)));
            current = next;
        }
    }
}

impl ItemPlace {









}

impl ItemPlace {
}
