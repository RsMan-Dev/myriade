//! `player_pong`: only a client's answer to a ping the server sent is told to the game, once,
//! with its id, in order.
use super::*;
use horizon_protocol::packets::{configuration::Ping, play::Serverbound as S};

fn with_input_context<T>(run: impl FnOnce(&mut TickCtx<'_>) -> T) -> T {
    let mut world = World::new(horizon_world::registry::BlockStateId(0), 0);
    let mut next_entity = 2;
    let border = crate::worldborder::WorldBorder::default();
    run(&mut TickCtx {
        sea_level: 63,
        world: &mut world,
        entities: &[],
        next_entity: &mut next_entity,
        spawn: [0.5, 64.0, 0.5],
        spawn_angle: [0.0; 2],
        spawn_dimension: "minecraft:overworld",
        difficulty: 2,
        difficulty_locked: false,
        clock_rate: 1.0,
        tick_rate: 20.0,
        tick_frozen: false,
        tick_steps: Default::default(),
        collision: None,
        border: &border,
        streaming: None,
    })
}

/// A hub at tick 10 whose guest 0 stands at `(0, 64, 0)`, entered its level at tick 0.
fn standing(inbox: Vec<S>) -> Hub {
    let (mut hub, _rx) = hub_with_a_guest();
    hub.ticks = 10;
    hub.guests.update(0, |g| {
        g.load_timeout = 0;
        g.position = [0.0, 64.0, 0.0];
        g.first_good = g.position;
        g.yaw = 0.0;
        g.pitch = 0.0;
        g.last_reported = (g.position, [0.0, 0.0]);
        g.ticks = 1;
        g.inbox.extend(inbox.into_iter().map(InboxItem::uncredited));
    });
    hub
}

fn drain(hub: &mut Hub) -> Vec<HostEvent> {
    with_input_context(|ctx| hub.drain_packets(ctx))
}

fn pongs(events: &[HostEvent]) -> Vec<(u32, i32)> {
    events
        .iter()
        .filter_map(|e| match e {
            HostEvent::Pong { conn, id } => Some((*conn, *id)),
            _ => None,
        })
        .collect()
}

/// The server pings guest 0 with `id`.
fn ping(hub: &Hub, id: i32) {
    hub.guests[0].send(&horizon_protocol::packets::play::Clientbound::Ping(Ping {
        id,
    }));
}

fn hub_with(inbox: Vec<S>, pings: &[i32]) -> Hub {
    let mut hub = standing(inbox);
    hub.pong_listened = true;
    for id in pings {
        ping(&hub, *id);
    }
    hub
}

#[test]
fn a_pong_answering_a_ping_is_told_to_the_game_in_order_with_its_id() {
    let mut hub = hub_with(
        vec![S::Pong(Ping { id: 7 }), S::Pong(Ping { id: -3 })],
        &[7, -3],
    );
    let conn = hub.guests[0].conn;
    assert_eq!(pongs(&drain(&mut hub)), [(conn, 7), (conn, -3)]);
}

#[test]
fn an_unsolicited_pong_queues_nothing() {
    let mut hub = hub_with(vec![S::Pong(Ping { id: 7 })], &[8]);
    assert!(pongs(&drain(&mut hub)).is_empty());
}

#[test]
fn a_repeated_pong_for_the_same_ping_queues_once() {
    let mut hub = hub_with(vec![S::Pong(Ping { id: 7 }); 3], &[7]);
    let conn = hub.guests[0].conn;
    assert_eq!(pongs(&drain(&mut hub)), [(conn, 7)]);
}

#[test]
fn the_pings_waiting_for_an_answer_stay_bounded() {
    let hub = hub_with(vec![], &[]);
    for id in 0..10_000 {
        ping(&hub, id);
    }
    assert_eq!(
        hub.guests[0].pings.lock().unwrap().len(),
        crate::lan::ping_ledger::MAX_PENDING
    );
}

#[test]
fn with_no_listener_a_pong_queues_nothing() {
    let mut hub = hub_with(vec![S::Pong(Ping { id: 7 })], &[7]);
    hub.pong_listened = false;
    assert!(pongs(&drain(&mut hub)).is_empty());
}
