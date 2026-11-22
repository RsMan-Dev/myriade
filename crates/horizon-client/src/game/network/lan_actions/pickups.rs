use super::*;

impl Game<'_> {
    /// Dropped items touching a guest go into their inventory
    /// (`ItemEntity.playerTouch`, then `ServerPlayer.onItemPickup`): the ECS takes them.
    pub(in crate::game) fn guest_pickups(&mut self, gi: usize) {
        self.ecs_guest_pickups(gi);
    }

    /// `guest_pickups` for the stacks P holds: P takes those in reach of the guest's feet, the guest's inventory takes what it can, and
    /// the rest lies at the feet again (a new entity: the id changes).
    fn ecs_guest_pickups(&mut self, gi: usize) {
        let Some(host) = self.lan.as_ref() else {
            return;
        };
        let g = &host.hub.guests[gi];
        if g.game_mode == SPECTATOR {
            return;
        }
        let (conn, position, id, uuid) = (g.conn, g.position, g.entity_id, g.uuid);
        // P takes every stack in reach; the rest the guest's inventory refuses lies at the feet again (below).
        let taken = self.ecs_with(|ecs, mw| {
            let who = horizon_ecs_proto::targeting::Who::Player(id as u32);
            ecs.pickup_items(mw, who, crate::lan::playerdata::uuid_ints(uuid), position, 1.5, &mut |s| s.count)
        });
        let mut counts = Vec::new();
        for (item, stack, at) in taken {
            let kind = stack.item.clone();
            let count = stack.count;
            let rest = self
                .as_guest(conn, |g| g.native_pickup_insert(&stack))
                .unwrap_or(Some(stack));
            let left = rest.as_ref().map_or(0, |r| r.count);
            if let Some(rest) = rest {
                self.ecs_drop(rest, at, [0.0; 3]);
            }
            let got = count.saturating_sub(left);
            if got == 0 {
                continue;
            }
            self.as_guest(conn, |g| g.award_stat("picked_up", &kind, got as i32));
            self.native_entity_left(item);
            counts.push((item, got));
        }
        if let Some(host) = self.lan.as_ref() {
            for (item, count) in counts {
                host.hub.take_item(item as i32, id, count as i32, position);
            }
        }
    }

    /// `guest_orbs` for the orbs P holds: one of those in reach of the guest, at random; P gives its value, the guest its points.
    fn ecs_guest_orbs(&mut self, gi: usize) {
        let Some(host) = self.lan.as_ref() else {
            return;
        };
        let g = &host.hub.guests[gi];
        let (conn, id, at, player, name) = (g.conn, g.entity_id, g.position, g.uuid, g.name.clone());
        let mut next = self.next_entity;
        let touching = self.ecs_with(|ecs, mw| ecs.pickup_orbs(mw, at, 1.0, &mut || { next += 1; next }));
        self.next_entity = next;
        if touching.is_empty() {
            return;
        }
        let pick = self.world.random.next_int_bound(touching.len() as i32).unwrap_or(0) as usize;
        let orb = touching[pick];
        let orb_at = self.ecs.position(orb).unwrap_or(at);
        let Some(value) = self.ecs_with(|ecs, mw| ecs.collect_orb(mw, orb)) else {
            return;
        };
        if !self.ecs.exists(orb) {
            self.native_entity_left(orb);
        }
        if let Some(host) = self.lan.as_mut() {
            host.play.entry(conn).or_default().xp_delay = 2;
        }
        self.native_player_exp_cooldown_change(player, &name, 2);
        if let Some(host) = self.lan.as_ref() {
            host.hub.take_item(orb as i32, id, 1, at);
        }
        let mut random = horizon_client::random_source::create();
        let pitch = (random.next_float() - random.next_float()) * 0.35 + 0.9;
        self.audio.play("minecraft:entity.experience_orb.pickup", Some(orb_at), 0.1, pitch);
        self.as_guest(conn, |game| {
            let rest = game.mending(value);
            if rest > 0 {
                game.give_experience_points(rest.min(i32::MAX as u32) as i32);
            }
        });
    }

    /// `Player.tick`'s takeXpDelay and `Player.aiStep`'s touch of the
    /// experience orbs in the guest's box grown by 1, 0.5, 1: one of them at
    /// random, at most every 2 ticks (`ExperienceOrb.playerTouch`), repairs
    /// with Mending and gives the rest as points. Everyone near hears it.
    pub(super) fn guest_orbs(&mut self, gi: usize) {
        let Some(host) = self.lan.as_mut() else {
            return;
        };
        let g = &host.hub.guests[gi];
        let (conn, player, name) = (g.conn, g.uuid, g.name.clone());
        let unable = g.spectator() || g.dead || g.health <= 0.0;
        let play = host.play.entry(conn).or_default();
        let cooldown = if play.xp_delay > 0 {
            play.xp_delay -= 1;
            Some(play.xp_delay)
        } else {
            None
        };
        let delayed = play.xp_delay > 0;
        if let Some(cooldown) = cooldown {
            self.native_player_exp_cooldown_change(player, &name, cooldown);
        }
        if unable || delayed {
            return;
        }
        self.ecs_guest_orbs(gi);
    }
}

#[cfg(test)]
#[path = "tests/pickup_post_tests.rs"]
mod pickup_post_tests;
