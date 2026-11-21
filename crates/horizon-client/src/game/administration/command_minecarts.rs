//! `MinecartCommandBlock`: its `BaseCommandBlock` run by a powered activator
//! rail (`performCommand`, `CloseableCommandBlockSource`), the entity data
//! that carries its command and last output to clients, and the edit screen's
//! `ServerboundSetCommandMinecartPacket` with `handleSetCommandMinecart`.
//! The state lives in the ECS cart (`cart_command`, `set_cart_command`), read
//! as a copy and written back by id.
use super::{Game, command_blocks::CommandBlockScreen, commands::{Target, execute::Source}};
use horizon_client::{
    command_block::{CommandBlock, Mode},
    text::Text,
};
use horizon_protocol::packets::play::{self, Serverbound as S};

impl Game<'_> {
    fn edit_command_cart<R>(&mut self, id: u32, edit: impl FnOnce(&mut CommandBlock) -> R) -> Option<R> {
        let mut state = self.ecs.cart_command(id)?;
        let out = edit(&mut state);
        self.ecs.set_cart_command(id, &state).then_some(out)
    }
    /// `BaseCommandBlock.performCommand` for the cart: false when it already
    /// ran this game tick.
    pub(in crate::game) fn perform_cart_command(&mut self, id: u32) -> bool {
        let game_time = self.world.game_time;
        let rules_work = self
            .ui
            .rules
            .bool_at(horizon_client::gamerules::vanilla::COMMAND_BLOCKS_WORK);
        let dimension = self.dim;
        let (Some(mut state), Some(view)) = (self.ecs.cart_command(id), self.ecs_view(id)) else {
            return false;
        };
        if state.last_execution == game_time {
            return false;
        }
        if state.command.eq_ignore_ascii_case("Searge") {
            state.last_output = Some(Text::literal("#itzlipofutzli"));
            state.success_count = 1;
            return self.ecs.set_cart_command(id, &state);
        }
        state.success_count = 0;
        let command = state.command.clone();
        let run = rules_work && !command.is_empty();
        if run {
            state.last_output = None;
        }
        // The state goes back before the command runs: it may write the cart itself.
        if !self.ecs.set_cart_command(id, &state) {
            return false;
        }
        if run {
            // MinecartCommandBase.createCommandSourceStack: the cart's
            // position and rotation at game master level.
            let source = Source {
                entity: Some(Target::Cart(id)),
                pos: view.position,
                rot: [view.yaw, 0.0],
                eyes: false,
                silent: false,
                store: None,
                dimension,
                origin_dimension: dimension,
                origin_player: false,
                level: None,
                block: None,
                cart: Some(id),
            };
            let successes = self.run_as_game_master(&command, source);
            self.edit_command_cart(id, |c| c.success_count = successes);
        }
        self.edit_command_cart(id, |c| {
            c.last_execution = if c.update_last_execution { game_time } else { -1 };
        });
        true
    }
    /// `CloseableCommandBlockSource.sendSystemMessage` for the cart: its last
    /// output stamped with the time. Nothing when it does not track output
    /// (`CommandSource.NULL`).
    pub(super) fn cart_command_output(&mut self, id: u32, message: Text) {
        self.edit_command_cart(id, |c| {
            if !c.track_output {
                return;
            }
            let time = chrono::Local::now().format("[%H:%M:%S] ").to_string();
            c.last_output = Some(Text::literal(time).append(message));
        });
    }
    /// `CommandSourceStack.sendSuccess` from a command minecart: its output
    /// when `send_command_feedback`, and `broadcastToAdmins` when
    /// `command_block_output`.
    pub(super) fn cart_command_success(&mut self, id: u32, message: Text, broadcast: bool) {
        let Some(name) = self
            .ecs
            .cart_command(id)
            .filter(|c| c.track_output)
            .map(|c| c.name())
        else {
            return;
        };
        let feedback = self
            .ui
            .rules
            .bool_at(horizon_client::gamerules::vanilla::SEND_COMMAND_FEEDBACK);
        if feedback {
            self.cart_command_output(id, message.clone());
        }
        if broadcast
            && feedback
            && self
                .ui
                .rules
                .bool_at(horizon_client::gamerules::vanilla::COMMAND_BLOCK_OUTPUT)
        {
            self.broadcast_source_to_admins(name, message);
        }
    }

    /// `LocalPlayer.openMinecartCommandBlock`: the editor over the cart's
    /// command (`MinecartCommandBlockEditScreen`).
    pub(super) fn open_cart_command_block(&mut self, id: u32) {
        let Some(block) = self.ecs.cart_command(id) else {
            return;
        };
        self.open_edit_screen((0, 0, 0), Some(id), block, Mode::Redstone, false);
    }
    /// The edit screen's Done for a cart (`populateAndSendPacket`, then
    /// `onDone` clears the last output of a cart that no longer tracks it): a
    /// server's client sends the packet, the window's own world applies it.
    pub(super) fn set_command_minecart(&mut self, id: u32, s: &CommandBlockScreen) {
        if !self.can_use_game_master_blocks() {
            self.system_message(Text::translate("advMode.notAllowed", &[]));
            return;
        }
        let command = s.command.value().to_owned();
        let track_output = s.track_output;
        // The output button set the cart's own flag as it was pressed.
        self.edit_command_cart(id, |c| c.track_output = track_output);
        {
            self.apply_cart_command(id, command, track_output);
        }
        if !track_output {
            self.edit_command_cart(id, |c| c.last_output = None);
        }
    }
    /// `ServerGamePacketListenerImpl.handleSetCommandMinecart` for a guest.
    pub(super) fn lan_set_command_minecart(&mut self, conn: u32, p: play::SetCommandMinecart) {
        let Some(g) = self.lan.as_ref().and_then(|host| host.hub.guest(conn)) else {
            return;
        };
        let allowed = g.creative() && g.permission >= 2;
        self.as_guest_messaged(conn, |game| {
            if allowed {
                game.apply_cart_command(p.entity as u32, p.command, p.track_output);
            } else {
                game.system_message(Text::translate("advMode.notAllowed", &[]));
            }
        });
    }
    /// What `handleSetCommandMinecart` does once the player may edit: the
    /// command (which resets the success count), output tracking, the cleared
    /// last output, and the chat note. The synched command and output are the
    /// cart's own components, so `onUpdated` has nothing to copy.
    pub(super) fn apply_cart_command(&mut self, id: u32, command: String, track_output: bool) {
        let enabled = self
            .ui
            .rules
            .bool_at(horizon_client::gamerules::vanilla::COMMAND_BLOCKS_WORK);
        if self
            .edit_command_cart(id, |c| {
                c.set_command(&command);
                c.track_output = track_output;
                if !track_output {
                    c.last_output = None;
                }
            })
            .is_none()
        {
            return;
        }
        if !command.is_empty() {
            let key = if enabled {
                "advMode.setCommand.success"
            } else {
                "advMode.setCommand.disabled"
            };
            self.system_message(Text::translate(key, &[Text::literal(command)]));
        }
    }
}
