//! Signs as the server sees them: a guest's text for a sign, and the clicks a sign runs
//! (`/` commands of a waxed sign) or lets a player make on a hanging sign's chains.
use crate::game::Game;
use crate::game::commands::{Target, execute::Source};
use horizon_client::containers::BlockEntity;
use horizon_client::items::ItemStack;
use horizon_client::signs::{ClickAction, DYE_TEXT, click_action};
use horizon_protocol::packets::play::{self, Clientbound as C};

impl Game<'_> {
    pub(in crate::game) fn lan_sign_update(&mut self, conn: u32, mut p: play::SignUpdate) {
        let Some(host) = self.lan.as_mut() else {
            return;
        };
        let Some(g) = host.hub.guest(conn) else {
            return;
        };
        let (uuid, name) = (g.uuid, g.name.clone());
        let key = (p.pos[0], p.pos[1], p.pos[2]);
        let lines = p.lines.join(" ");
        if !self.chat_filter_text(
            conn,
            &[&lines],
            horizon_client::lan::hub::chat_filter::Place::Sign,
        ) {
            // The sign stays as it was: the editor closes, as after a sign the server refused.
            self.world.block_entities.set_sign_editor(key, None);
            // The player's client shows the text it sent: the sign as it is, again.
            self.mark_block_entity_updated(key);
            return;
        }
        let Some(BlockEntity::Sign(sign)) = self.world.block_entities.get(&key) else {
            return;
        };
        if sign.waxed || sign.allowed_editor != Some(uuid) {
            log::warn!("{name} just tried to change non-editable sign");
            return;
        }
        // Everything vanilla checks has passed: the mods hear what is about to
        // go on the sign, with the player's state swapped in, and may rewrite
        // or refuse it.
        if let Some((uuid, name, dimension)) = self
            .lan
            .as_ref()
            .and_then(|host| host.hub.guest(conn))
            .map(|g| (g.uuid, g.name.clone(), g.dimension.clone()))
        {
            let mut lines = p.lines.to_vec();
            let mut cancel = false;
            self.as_guest_messaged(conn, |game| {
                game.native_sign_changed(mod_api::__private::SignChangedCall {
                    player: uuid,
                    name: &name,
                    dimension: &dimension,
                    pos: p.pos,
                    front: p.front,
                    lines: &mut lines,
                    cancel: &mut cancel,
                });
            });
            if cancel {
                // The sign keeps its text: the editor closes, and the player's
                // client, which shows what it sent, gets the sign as it is.
                self.world.block_entities.set_sign_editor(key, None);
                self.mark_block_entity_updated(key);
                return;
            }
            lines.resize(4, String::new());
            for (line, new) in p.lines.iter_mut().zip(lines) {
                *line = new;
            }
        }
        if self
            .lan
            .as_ref()
            .and_then(|host| host.play.get(&conn))
            .is_none()
        {
            return;
        }
        // A hook may have broken the sign.
        let Some(BlockEntity::Sign(sign)) = self.world.block_entities.get_mut(&key) else {
            return;
        };
        // A hook may have broken the sign.
        if sign.waxed {
            self.world.block_entities.set_sign_editor(key, None);
            return;
        }
        let side = if p.front {
            &mut sign.front
        } else {
            &mut sign.back
        };
        for (line, new) in side.lines.iter_mut().zip(&p.lines) {
            *line = strip_formatting(new);
        }
        self.world.block_entities.set_sign_editor(key, None);
        self.mark_block_entity_updated(key);
    }

    pub(in crate::game) fn sign_can_execute_click_commands(&self, pos: (i32, i32, i32)) -> bool {
        // Only a waxed sign can; skip the facing maths for everything else.
        let Some(BlockEntity::Sign(sign)) = self.world.block_entities.get(&pos) else {
            return false;
        };
        sign.waxed && sign.can_execute_click_commands(self.facing_sign_front(pos))
    }

    fn facing_sign_front(&self, pos: (i32, i32, i32)) -> bool {
        let Some(player) = self.player.as_ref() else {
            return true;
        };
        let id = self.world.block(pos.0, pos.1, pos.2);
        let Some(state) = self.pack.assets.registry.state(id) else {
            return true;
        };
        let form = self.res.facts().get(id).sign_form();
        let prop = |key: &str| state.properties.get(key).map(String::as_str);
        // getYRotationDegrees: a wall sign's `Direction.toYRot`, a standing or
        // ceiling sign's rotation segment.
        let y_rot = if form.is_some_and(|f| f.wall) {
            match prop("facing") {
                Some("west") => 90.0,
                Some("north") => 180.0,
                Some("east") => 270.0,
                _ => 0.0,
            }
        } else {
            prop("rotation")
                .and_then(|r| r.parse::<f32>().ok())
                .unwrap_or(0.0)
                * 22.5
        };
        // getSignHitboxCenterPosition: only a wall sign's board is off-centre,
        // against the wall it hangs on.
        let centre = if form.is_some_and(|f| f.wall && !f.hanging) {
            match y_rot as i32 {
                90 => (0.9375, 0.5),
                180 => (0.5, 0.9375),
                270 => (0.0625, 0.5),
                _ => (0.5, 0.0625),
            }
        } else {
            (0.5, 0.5)
        };
        let xd = player.position.x - (f64::from(pos.0) + centre.0);
        let zd = player.position.z - (f64::from(pos.2) + centre.1);
        let player_y_rot = (zd.atan2(xd).to_degrees() - 90.0) as f32;
        // Mth.degreesDifferenceAbs.
        let diff = (player_y_rot - y_rot).rem_euclid(360.0);
        let diff = if diff >= 180.0 { 360.0 - diff } else { diff };
        diff <= 90.0
    }

    pub(in crate::game) fn chains_hanging_sign(
        &self,
        pos: [i32; 3],
        face: [i32; 3],
        held: Option<&ItemStack>,
    ) -> bool {
        // `itemStack.getItem() instanceof HangingSignItem`, by the item's class
        // and not its name.
        let hanging_sign = held.is_some_and(|s| {
            self.res
                .behaviours()
                .item_binding(&s.item)
                .is_some_and(|i| i.kind == horizon_world::level::ItemKind::HangingSign)
        });
        let key = (pos[0], pos[1], pos[2]);
        if !hanging_sign {
            return false;
        }
        let state = self.world.block(pos[0], pos[1], pos[2]);
        let Some(form) = self
            .res
            .facts()
            .get(state)
            .sign_form()
            .filter(|f| f.hanging)
        else {
            return false;
        };
        let free_side = if form.wall {
            // `isHittingEditableSide`: a wall hanging sign's board lies in its
            // FACING's plane, so its two faces are the editable text and only
            // its edges chain.
            let Some(facing) = self
                .pack
                .assets
                .registry
                .state(state)
                .and_then(|s| s.properties.get("facing"))
            else {
                return false;
            };
            let axis = match &**facing {
                "east" | "west" => 0,
                "up" | "down" => 1,
                _ => 2,
            };
            face.iter().position(|c| *c != 0) != Some(axis)
        } else {
            // `hitResult.getDirection().equals(Direction.DOWN)`.
            face == [0, -1, 0]
        };
        free_side && !self.sign_can_execute_click_commands(key)
    }
}

impl Game<'_> {
    /// `SignBlock.useItemOn` with a `SignApplicator` in hand (dyes, ink sacs,
    /// honeycomb): the item changes the side the player faces and is used up.
    /// Whether it did; a sign it does not apply to goes on to the empty hand.
    pub(in crate::game) fn use_sign_applicator(&mut self, pos: [i32; 3], held: Option<&ItemStack>) -> bool {
        let Some(stack) = held else {
            return false;
        };
        let key = (pos[0], pos[1], pos[2]);
        let waxed = match self.world.block_entities.get(&key) {
            Some(BlockEntity::Sign(sign)) => sign.waxed,
            _ => return false,
        };
        if waxed || !self.may_build() {
            return false;
        }
        let item = &*stack.item;
        let dye = if item.ends_with("_dye") {
            self.res
                .items()
                .get(&stack.item)
                .and_then(|i| i.dye.clone())
        } else {
            None
        };
        let colour = dye.and_then(|d| DYE_TEXT.iter().position(|(n, _)| *n == d));
        if !matches!(
            item,
            "minecraft:honeycomb" | "minecraft:glow_ink_sac" | "minecraft:ink_sac"
        ) && colour.is_none()
        {
            return false;
        }
        // `SignBlock.useItemOn`: a sign another player's editor is on takes no
        // dye, ink sac or honeycomb.
        if self.other_player_is_editing_sign(key) {
            return false;
        }
        let front = self.facing_sign_front(key);
        let Some(BlockEntity::Sign(sign)) = self.world.block_entities.get_mut(&key) else {
            return false;
        };
        let centre = pos.map(|v| f64::from(v) + 0.5);
        let sound = if item == "minecraft:honeycomb" {
            // HoneycombItem.tryApplyToSign: waxes any sign, empty or not.
            sign.waxed = true;
            "minecraft:item.honeycomb.wax_on"
        } else {
            let side = if front {
                &mut sign.front
            } else {
                &mut sign.back
            };
            // SignApplicator.canApplyToSign: the side must have text.
            if side.is_empty() {
                return false;
            }
            match colour {
                Some(c) if side.color != c as u8 => {
                    side.color = c as u8;
                    "minecraft:item.dye.use"
                }
                None if item == "minecraft:glow_ink_sac" && !side.glowing => {
                    side.glowing = true;
                    "minecraft:item.glow_ink_sac.use"
                }
                None if item == "minecraft:ink_sac" && side.glowing => {
                    side.glowing = false;
                    "minecraft:item.ink_sac.use"
                }
                _ => return false,
            }
        };
        if item == "minecraft:honeycomb" {
            self.level_event_at(3003, pos, 0);
        }
        self.audio.play(sound, Some(centre), 1.0, 1.0);
        self.mark_block_entity_updated(key);
        self.execute_sign_click_commands(pos, front);
        let state = self.world.block(pos[0], pos[1], pos[2]);
        self.post_block_game_event(
            "minecraft:block_change",
            pos,
            self.sculk_actor(),
            Some(state),
        );
        self.award_stat("used", &stack.item, 1);
        self.consume_held();
        self.trigger_block("minecraft:item_used_on_block", pos, stack);
        true
    }

    /// `SignBlock.useWithoutItem` (the block's `OpenMenu` effect): the side
    /// the player faces runs its click commands first; then a waxed sign
    /// plays the failed-interaction sound, a sign that ran one does nothing
    /// more, and otherwise a player who may build edits the side they face.
    pub(in crate::game) fn use_sign_empty_hand(&mut self, pos: [i32; 3]) {
        let key = (pos[0], pos[1], pos[2]);
        if !matches!(
            self.world.block_entities.get(&key),
            Some(BlockEntity::Sign(_))
        ) {
            return;
        }
        let front = self.facing_sign_front(key);
        self.execute_sign_click_commands(pos, front);
        // The commands may have changed the sign, or removed it.
        let Some(BlockEntity::Sign(sign)) = self.world.block_entities.get(&key) else {
            return;
        };
        if sign.waxed {
            // `getSignInteractionFailedSoundEvent`: a hanging sign has its own.
            let sound = if sign.hanging {
                "minecraft:block.hanging_sign.waxed_interact_fail"
            } else {
                "minecraft:block.sign.waxed_interact_fail"
            };
            self.audio
                .play(sound, Some(pos.map(|v| f64::from(v) + 0.5)), 1.0, 1.0);
        }
    }

    /// Whether `SignBlock.useWithoutItem` on the sign at `pos` consumes the
    /// click. Vanilla answers: a waxed sign (the failed-interaction sound), a
    /// side whose lines carry a click action (`executeClickCommandsIfPresent`,
    /// which runs it), and a player who may build (the text editor). Every
    /// other sign answers PASS, so `ServerPlayerGameMode.useItemOn` goes on to
    /// `ItemStack.useOn` and an adventure-mode `can_place_on` stack places
    /// against it. Vanilla also passes while another player is editing the
    /// sign and when its text has styled lines; neither is modelled here.
    pub(in crate::game) fn sign_consumes_click(&self, pos: [i32; 3]) -> bool {
        let key = (pos[0], pos[1], pos[2]);
        let Some(BlockEntity::Sign(sign)) = self.world.block_entities.get(&key) else {
            return false;
        };
        if sign.waxed || self.may_build() {
            return true;
        }
        let side = if self.facing_sign_front(key) {
            &sign.front
        } else {
            &sign.back
        };
        side.click
            .iter()
            .flatten()
            .any(|event| click_action(event).is_some())
    }

    /// `SignBlockEntity.executeClickCommandsIfPresent`: every line of the
    /// side whose style has a `run_command`, `show_dialog` or `custom` click
    /// event runs it as the acting player when the sign allows op features;
    /// otherwise the player is told click actions are disabled. Whether any
    /// line had one.
    pub(in crate::game) fn execute_sign_click_commands(&mut self, pos: [i32; 3], front: bool) -> bool {
        let key = (pos[0], pos[1], pos[2]);
        let Some(BlockEntity::Sign(sign)) = self.world.block_entities.get(&key) else {
            return false;
        };
        let allow_op_features = sign.allow_op_features;
        let side = if front { &sign.front } else { &sign.back };
        // The side as it was when the loop began: a command may change it.
        let events: Vec<_> = side.click.iter().flatten().cloned().collect();
        let mut any_click_command = false;
        for event in &events {
            match click_action(event) {
                Some(ClickAction::RunCommand(command)) => {
                    if allow_op_features {
                        self.run_sign_command(pos, &command);
                    }
                    any_click_command = true;
                }
                Some(ClickAction::ShowDialog(_)) => {
                    any_click_command = true;
                }
                // MinecraftServer.handleCustomClickAction only logs it.
                Some(ClickAction::Custom { id, payload }) => {
                    if allow_op_features {
                        log::debug!("Received custom click action {id} with payload {payload:?}");
                    }
                    any_click_command = true;
                }
                None => {}
            }
        }
        if !allow_op_features && any_click_command {
            self.sign_click_actions_disabled();
        }
        any_click_command
    }

    /// `SignBlock.otherPlayerIsEditingSign`: the sign at `key` belongs to
    /// someone else's editor right now.
    pub(in crate::game) fn other_player_is_editing_sign(&mut self, key: (i32, i32, i32)) -> bool {
        match self.world.block_entities.sign_editor(&key) {
            Some(holder) => holder != self.sign_editor_uuid(),
            None => false,
        }
    }

    /// Run a sign's `run_command` line as the prefixed command vanilla runs
    /// it as: a leading slash is allowed, errors are not reported (the NULL
    /// source).
    pub(in crate::game) fn run_sign_command(&mut self, pos: [i32; 3], command: &str) {
        let source = self.sign_command_source(pos);
        let line = command.strip_prefix('/').unwrap_or(command);
        self.exec = Default::default();
        let permission = std::mem::replace(&mut self.session.registries.permission, 2);
        let _ = self.execute_line(line, source);
        self.session.registries.permission = permission;
        // InventoryChangeTrigger fires as slots change, not on the next tick.
        if self.player.is_some() {
            self.check_inventory_trigger();
        }
    }

    /// `Player.sendOverlayMessage(CLICK_ACTIONS_DISABLED)`: red text on the
    /// action bar. A guest gets a packet, so its own client translates it.
    pub(in crate::game) fn sign_click_actions_disabled(&mut self) {
        const KEY: &str = "sign.click_actions_disabled";
        if self.lan_acting.is_some() {
            let message = horizon_client::text::Text::translate(KEY, &[])
                .with_color(horizon_client::text::RED);
            self.guest_packets.push(C::SystemChat(play::SystemChat {
                content: horizon_client::lan::data::component_nbt(&message.component()),
                overlay: true,
            }));
        }
    }

    /// `Player.getUUID` of the acting player: the one a sign remembers as its
    /// editor.
    pub(in crate::game) fn sign_editor_uuid(&mut self) -> u128 {
        horizon_client::lan::playerdata::ints_uuid(self.acting_player_uuid())
    }

}

impl Game<'_> {
    /// `SignBlockEntity.createCommandSourceStack`: the acting player runs the
    /// command from the middle of the sign's block, rotation zero, at the
    /// game master level, with `CommandSource.NULL` (no feedback, no failure
    /// messages).
    pub(in crate::game) fn sign_command_source(&self, pos: [i32; 3]) -> Source {
        Source {
            entity: self.player.is_some().then_some(Target::Player),
            pos: pos.map(|v| f64::from(v) + 0.5),
            rot: [0.0, 0.0],
            eyes: false,
            silent: true,
            store: None,
            dimension: self.dim,
            origin_dimension: self.dim,
            origin_player: self.player.is_some(),
            level: None,
            block: None,
            cart: None,
        }
    }
}

/// `ChatFormatting.stripFormatting`: drops each `§` that is followed by a formatting code.
fn strip_formatting(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '§' && chars.peek().is_some_and(|n| matches!(n.to_ascii_lowercase(), '0'..='9' | 'a'..='f' | 'k'..='o' | 'r')) {
            chars.next();
        } else {
            out.push(c);
        }
    }
    out
}
