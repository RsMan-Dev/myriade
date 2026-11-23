//! The lectern and the books a player writes: a book placed on a lectern, the lectern's menu
//! and its page turns, and a guest's edit of the book it holds.
use crate::game::{Game, command_text::PLAYER_NAME};
use myriade_engine::{
    block_entity_item,
    containers::{BlockEntity, Lectern},
    items::ItemStack,
    menu::{Kind, Menu},
    mobs::Who,
};
use myriade_engine::item_data::WrittenBook;
use horizon_core::{nbt::Tag, position::BlockPos};
use horizon_protocol::packets::play::{self, Clientbound as C};
use horizon_world::{level::flags as level_flags, registry::BlockStateId};
use myriade_types::Hand;

/// The off hand's slot in the player inventory (`shield::OFFHAND_SLOT`).
const OFFHAND_SLOT: i32 = 40;

impl Game<'_> {
    pub(in crate::game) fn lectern_placed(
        &mut self,
        pos: [i32; 3],
        state: BlockStateId,
        stack: &ItemStack,
        game_master: bool,
    ) -> myriade_engine::Result<BlockStateId> {
        if !game_master
            || !block_entity_item::carries_book(stack)
            || &*self.res.facts().get(state).name != "minecraft:lectern"
        {
            return Ok(state);
        }
        match self.with_props(state, &[("has_book", "true")]) {
            Some(new) if new != state => {
                self.level_set(pos, new)?;
                Ok(new)
            }
            _ => Ok(state),
        }
    }

    pub(in crate::game) fn lectern_menu(&self, key: (i32, i32, i32)) -> Option<Menu> {
        let Some(BlockEntity::Lectern(l)) = self.world.block_entities.get(&key) else {
            return None;
        };
        let mut menu = Menu::new(Kind::Lectern);
        menu.set_registry_id(Some("minecraft:lectern"));
        menu.set_slot(&self.menu_env(), 0, l.book.clone());
        menu.data[0] = l.page;
        Some(menu)
    }

    pub(in crate::game) fn lectern_menu_done(&mut self, key: (i32, i32, i32), menu: &Menu) {
        let Some(BlockEntity::Lectern(l)) = self.world.block_entities.get_mut(&key) else {
            return;
        };
        let (had, before) = (l.book.is_some(), l.page);
        l.book = menu.slot(0).cloned();
        l.page = menu.data[0];
        if had && l.book.is_none() {
            self.set_lectern_powered(key, false);
            self.set_lectern_has_book(key, false, None);
            self.container_changed(key);
        } else if menu.data[0] != before {
            self.set_lectern_powered(key, true);
            self.lectern_pulses.push((key, 2));
            self.level_event_at(1043, [key.0, key.1, key.2], 0);
            self.container_changed(key);
            if let (Some(player), Some(name)) = (self.lan_acting_uuid, self.lan_acting_name.clone())
            {
                self.native_player_lectern_page_change(
                    player,
                    &name,
                    [key.0, key.1, key.2],
                    menu.data[0],
                );
            }
        }
    }

    pub(in crate::game) fn lan_edit_book(&mut self, conn: u32, p: play::EditBook) {
        if !((0..9).contains(&p.slot) || p.slot == OFFHAND_SLOT) {
            return;
        }
        if let Some(title) = &p.title
            && !self.chat_filter_text(
                conn,
                &[title],
                myriade_engine::lan::hub::chat_filter::Place::Book,
            )
        {
            return;
        }
        let menu_slot = if p.slot == OFFHAND_SLOT {
            45
        } else {
            36 + p.slot as usize
        };
        let cancelled = self.as_guest(conn, |g| {
            let mut cancel = false;
            let writable = g
                .stack_in(p.slot as usize)
                .is_some_and(|stack| &*stack.item == "minecraft:writable_book");
            if writable
                && let (Some(player), Some(name)) = (g.lan_acting_uuid, g.lan_acting_name.clone())
            {
                g.native_player_edit_book(
                    player,
                    &name,
                    p.pages.len() as i32,
                    p.title.is_some(),
                    &mut cancel,
                );
            }
            if cancel {
                return true;
            }
            g.write_book(p.slot as usize, p.pages, p.title);
            false
        });
        // A cancelled edit leaves the client's own copy of the book: forget
        // what was sent so the next broadcast resends the server's stack.
        if cancelled == Some(true)
            && let Some(host) = self.lan.as_mut()
        {
            let _ = host
                .hub
                .guests
                .update_connection(conn, |g| g.menu_sent[menu_slot] = None);
        }
    }
}

impl Game<'_> {
    /// `WritableBookItem.use` / `WrittenBookItem.use` (`Player.openItemGui`).
    /// A server's player writes in its own client's editor
    /// (`LocalPlayer.openItemGui`), and reads on its own client once told to
    /// (`ServerPlayer.openItemGui`'s ClientboundOpenBookPacket).
    pub(in crate::game) fn use_book(&mut self, stack: &ItemStack) -> bool {
        let written = match &*stack.item {
            "minecraft:writable_book" => false,
            "minecraft:written_book" => true,
            _ => return false,
        };
        // `ServerPlayer.openItemGui` resolves a written book for the player
        // opening it before the screen reads it.
        let held;
        let stack = if written && self.resolve_held_book() {
            held = self.held().cloned();
            held.as_ref().unwrap_or(stack)
        } else {
            stack
        };
        if self.lan_acting.is_some() {
            if written {
                let hand = i32::from(self.hand == Hand::Off);
                self.guest_packets
                    .push(C::OpenBook(play::OpenBook { hand }));
            }
        }
        // WritableBookItem.use / WrittenBookItem.use count every open.
        self.award_stat("used", &stack.item, 1);
        self.swing_arm();
        true
    }

    /// `LecternBlock.useItemOn`: a book goes onto an empty lectern.
    pub(in crate::game) fn use_lectern(&mut self, pos: [i32; 3], held: Option<&ItemStack>) -> bool {
        let key = (pos[0], pos[1], pos[2]);
        if &*self
            .res
            .facts()
            .get(self.world.block(pos[0], pos[1], pos[2]))
            .name
            != "minecraft:lectern"
        {
            return false;
        }
        let has_book = matches!(self.world.block_entities.get(&key), Some(BlockEntity::Lectern(l)) if l.book.is_some());
        // `LecternBlock.useItemOn`: `ItemTags.LECTERN_BOOKS`, which a data
        // pack can add its own books to, not only the two vanilla items.
        let Some(book) = held.filter(|h| {
            world_items!(self)
                .item_tags
                .get("minecraft:lectern_books")
                .is_some_and(|books| books.contains(&*h.item))
        }) else {
            return false;
        };
        if has_book {
            return false;
        }
        let mut one = book.clone();
        one.count = 1;
        self.world.block_entities.insert(
            key,
            BlockEntity::Lectern(Lectern {
                book: Some(one),
                page: 0,
            }),
        );
        self.set_lectern_has_book(key, true, Some(Who::Player));
        if !self.creative {
            self.consume_held();
        }
        let at = [
            f64::from(pos[0]) + 0.5,
            f64::from(pos[1]) + 0.5,
            f64::from(pos[2]) + 0.5,
        ];
        self.audio
            .play("minecraft:item.book.put", Some(at), 1.0, 1.0);
        self.container_changed(key);
        if let (Some(player), Some(name)) = (self.lan_acting_uuid, self.lan_acting_name.clone()) {
            self.native_player_insert_lectern_book(player, &name, pos, &book.item);
        }
        true
    }

    /// The 2-tick pulses of turned lecterns.
    pub(in crate::game) fn tick_lecterns(&mut self) {
        let mut done = Vec::new();
        for (key, left) in &mut self.lectern_pulses {
            *left -= 1;
            if *left <= 0 {
                done.push(*key);
            }
        }
        self.lectern_pulses.retain(|(_, left)| *left > 0);
        for key in done {
            self.set_lectern_powered(key, false);
        }
    }

    pub(in crate::game) fn set_lectern_powered(&mut self, key: (i32, i32, i32), powered: bool) {
        self.set_lectern_property(key, "powered", powered);
    }

    /// `LecternBlock.resetBookState`: the property with the block change event.
    pub(in crate::game) fn set_lectern_has_book(&mut self, key: (i32, i32, i32), has_book: bool, who: Option<Who>) {
        // `LecternBlock.resetBookState`: `setBlockAndUpdate` writes the state and
        // the `block_change` post follows unconditionally, not only when the
        // state differs.
        self.set_lectern_property(key, "has_book", has_book);
        let after = self.world.block(key.0, key.1, key.2);
        self.post_block_game_event(
            "minecraft:block_change",
            [key.0, key.1, key.2],
            who,
            Some(after),
        );
    }

    pub(in crate::game) fn set_lectern_property(&mut self, key: (i32, i32, i32), name: &str, value: bool) {
        let state = self.world.block(key.0, key.1, key.2);
        let registry = self.pack.assets.registry;
        let Some(mut s) = registry.state(state).cloned() else {
            return;
        };
        if !s.properties.contains_key(name) {
            return;
        }
        s.properties.insert(name.into(), value.to_string());
        if let Some(new) = registry.id(&s) {
            let p = BlockPos::new(key.0, key.1, key.2);
            self.with_level(|b, level, ticks| {
                b.set_block(level, ticks, p, new, level_flags::UPDATE_ALL)
            });
        }
    }

    /// `ServerPlayer.openItemGui` -> `WrittenBookContent.resolveForItem`: the
    /// selectors and scores of the written book in the acting hand become
    /// their text for the player opening it, and the book is marked resolved
    /// (so it is never resolved again). True when the pages were rewritten.
    pub(in crate::game) fn resolve_held_book(&mut self) -> bool {
        let Some(mut book) = self
            .held()
            .and_then(|s| s.data())
            .and_then(|d| d.written.clone())
        else {
            return false;
        };
        if book.resolved {
            return false;
        }
        let string = |v: &Tag| match v {
            Tag::String(s) => Some(s.to_string_lossy()),
            _ => None,
        };
        let changed = book.resolve(|tag| {
            let Tag::Compound(c) = tag else { return None };
            if let Some(Tag::String(selector)) = c.get(&"selector".into()) {
                let selector = selector.to_string_lossy();
                return Some(self.resolve_selector_text(&selector));
            }
            let score = c.get(&"score".into())?;
            let name = score.get("name").and_then(string).unwrap_or_default();
            let objective = score.get("objective").and_then(string).unwrap_or_default();
            let name = if name == "*" {
                PLAYER_NAME
            } else {
                name.as_str()
            };
            Some(
                self.score
                    .board
                    .score(name, &objective)
                    .map_or_else(String::new, |s| s.value.to_string()),
            )
        });
        if let Some(stack) = self.held_mut() {
            stack.data_mut().written = Some(book);
        }
        // The guest turn restores this inventory to the hub, whose menu
        // synchronization sends the changed resolved stack back to the client.
        changed
    }
}

impl Game<'_> {
    /// The book and quill at `slot` takes the pages, or with a title becomes
    /// the written book its writer signed.
    fn write_book(&mut self, slot: usize, pages: Vec<String>, title: Option<String>) {
        let author = self
            .lan_acting_name
            .clone()
            .unwrap_or_else(|| crate::game::command_text::PLAYER_NAME.to_owned());
        let Some(stack) = self
            .stack_in_mut(slot)
            .as_mut()
            .filter(|s| &*s.item == "minecraft:writable_book")
        else {
            return;
        };
        if let Some(title) = title {
            // `signBook` transmutes the carried stack, so a renamed book and
            // quill stays renamed and keeps its other components.
            let mut written = stack.transmute("minecraft:written_book", stack.count);
            let data = written.data_mut();
            data.writable_pages = Vec::new();
            data.written = Some(WrittenBook {
                title,
                author,
                generation: 0,
                pages,
                page_tags: Vec::new(),
                // `signBook`'s `new WrittenBookContent(..., true)`: the pages
                // are literals, so the content is already resolved.
                resolved: true,
            });
            *stack = written;
        } else {
            stack.data_mut().writable_pages = if pages.iter().all(String::is_empty) {
                Vec::new()
            } else {
                pages
            };
            stack.tidy();
        }
    }
}
