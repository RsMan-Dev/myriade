//! The connection: login, configuration, then the play state. A thread reads the packets; the verbs
//! and the tick run on the main thread through `Session::pump`.
use crate::{
    online::{self, Account},
    state::State,
};
use horizon_protocol::{
    PROTOCOL_VERSION,
    buf::Writer,
    connection::{ConnReader, ConnWriter, Connection},
    crypto::uuid_string,
    ids,
    packets::{
        Empty,
        configuration::{self as cfg, ClientInformation},
        handshake, login,
        play::{self, Clientbound as C, Serverbound as S},
    },
    text,
};
use std::{
    sync::mpsc::{self, Receiver, RecvTimeoutError, Sender},
    thread,
    time::{Duration, Instant},
};

/// What the reader thread hands over: a packet body (id first, as read) or the end of the connection.
pub enum Inbound {
    Packet(i32, Vec<u8>),
    Closed(String),
}

/// `ChunkBatchSizeCalculator`: the chunks per tick a vanilla client asks for, from how fast batches arrive.
pub struct BatchSize {
    nanos_per_chunk: f64,
    weight: u32,
    started: Instant,
}

impl BatchSize {
    fn new() -> Self {
        Self {
            nanos_per_chunk: 2_000_000.0,
            weight: 1,
            started: Instant::now(),
        }
    }

    fn finished(&mut self, size: i32) {
        if size > 0 {
            let per_chunk = self.started.elapsed().as_nanos() as f64 / f64::from(size);
            let clamped = per_chunk.clamp(self.nanos_per_chunk / 3.0, self.nanos_per_chunk * 3.0);
            let weight = f64::from(self.weight);
            self.nanos_per_chunk = (self.nanos_per_chunk * weight + clamped) / (weight + 1.0);
            self.weight = (self.weight + 1).min(49);
        }
    }

    fn desired(&self) -> f32 {
        (7_000_000.0 / self.nanos_per_chunk) as f32
    }
}

/// Reads packets until the connection ends, handing each to the main thread.
fn read_loop(reader: &mut ConnReader, tx: &Sender<Inbound>) {
    loop {
        let item = match reader.recv() {
            Ok((id, body)) => Inbound::Packet(id, body),
            Err(e) => {
                let _ = tx.send(Inbound::Closed(e.to_string()));
                return;
            }
        };
        if tx.send(item).is_err() {
            return;
        }
    }
}

/// A logged-in bot: its writer, what it knows, and the packets not handled yet.
pub struct Session {
    pub w: ConnWriter,
    pub st: State,
    rx: Receiver<Inbound>,
    /// Answer teleports by themselves (`tp_ack` turns this off).
    pub auto_ack: bool,
    /// The connection is over: nothing more is read or sent.
    pub closed: bool,
    batch: BatchSize,
    next_tick: Instant,
}

/// Who the bot logs in as; `account` is set when the server is in online mode.
pub struct Login {
    pub name: String,
    pub uuid: u128,
    pub account: Option<Account>,
    pub session_url: String,
}

/// Login and configuration (the vanilla client's first states), without asserts.
pub fn connect(host: &str, port: u16, login: &Login, view: i8) -> Result<Connection, String> {
    let e = |e: horizon_protocol::Error| e.to_string();
    let mut c = Connection::connect(host, port, Duration::from_secs(5)).map_err(e)?;
    c.reader
        .set_read_timeout(Some(Duration::from_secs(20)))
        .map_err(e)?;
    c.send(
        &handshake::Serverbound::Intention(handshake::Intention {
            protocol: PROTOCOL_VERSION,
            host: host.into(),
            port,
            intent: handshake::LOGIN,
        })
        .encode(),
    )
    .map_err(e)?;
    c.send(
        &login::Serverbound::Hello(login::ServerHello {
            name: login.name.clone(),
            uuid: login.uuid,
        })
        .encode(),
    )
    .map_err(e)?;
    loop {
        let (id, payload) = c.recv().map_err(e)?;
        match login::Clientbound::decode(id, &payload).map_err(e)? {
            login::Clientbound::Compression(p) => c.set_compression(p.threshold),
            login::Clientbound::Hello(h) => {
                let account = login
                    .account
                    .as_ref()
                    .ok_or("the server is in online mode: give a session file")?;
                let (key, secret) = online::answer(account, &h, &login.session_url)?;
                c.send(&login::Serverbound::Key(key).encode()).map_err(e)?;
                c.enable_encryption(&secret);
            }
            login::Clientbound::CustomQuery(q) => {
                let answer = login::CustomQueryAnswer {
                    transaction: q.transaction,
                    payload: None,
                };
                c.send(&login::Serverbound::CustomQueryAnswer(answer).encode())
                    .map_err(e)?;
            }
            login::Clientbound::LoginFinished(_) => {
                c.send(&login::Serverbound::LoginAcknowledged(Empty).encode())
                    .map_err(e)?;
                break;
            }
            login::Clientbound::Disconnect(d) => {
                return Err(format!("disconnected: {}", d.reason_json));
            }
            other => return Err(format!("unexpected login packet {other:?}")),
        }
    }
    c.send(&cfg::Serverbound::ClientInformation(ClientInformation::new(view)).encode())
        .map_err(e)?;
    loop {
        let (id, payload) = c.recv().map_err(e)?;
        match cfg::Clientbound::decode(id, &payload).map_err(e)? {
            // The bot knows the core pack, as the vanilla client does: it asks for no registry data.
            cfg::Clientbound::SelectKnownPacks(p) => {
                let reply = cfg::Serverbound::SelectKnownPacks(cfg::SelectKnownPacks { packs: p.packs });
                c.send(&reply.encode()).map_err(e)?;
            }
            cfg::Clientbound::KeepAlive(k) => {
                let reply = cfg::Serverbound::KeepAlive(cfg::KeepAlive { id: k.id });
                c.send(&reply.encode()).map_err(e)?;
            }
            cfg::Clientbound::Ping(p) => {
                let reply = cfg::Serverbound::Pong(cfg::Ping { id: p.id });
                c.send(&reply.encode()).map_err(e)?;
            }
            cfg::Clientbound::FinishConfiguration(_) => {
                c.send(&cfg::Serverbound::FinishConfiguration(Empty).encode())
                    .map_err(e)?;
                break;
            }
            cfg::Clientbound::Disconnect(d) => {
                let why = text::plain(&d.reason, &Default::default());
                return Err(format!("disconnected: {why}"));
            }
            _ => {}
        }
    }
    Ok(c)
}

impl Session {
    /// Starts the play state: the reader thread takes the read half, the writer stays here.
    pub fn start(mut c: Connection, name: &str, uuid: u128) -> Self {
        let _ = c.reader.set_read_timeout(None);
        let (mut reader, w) = c.split();
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || read_loop(&mut reader, &tx));
        let st = State {
            name: name.into(),
            uuid,
            ..Default::default()
        };
        Self {
            w,
            st,
            rx,
            auto_ack: true,
            closed: false,
            batch: BatchSize::new(),
            next_tick: Instant::now(),
        }
    }

    pub fn send(&mut self, p: S) -> Result<(), String> {
        self.w.send(&p.encode()).map_err(|e| e.to_string())
    }

    /// The arm swing: the serverbound `punch` of 26.3 (no typed packet for it, and no body).
    pub fn swing(&mut self) -> Result<(), String> {
        let mut b = Writer::new();
        b.varint(ids::play::serverbound::PUNCH);
        self.w.send(&b.data).map_err(|e| e.to_string())
    }

    /// Handles the packets that arrive within `wait`, then the tick if it is due.
    pub fn pump(&mut self, wait: Duration) -> Result<(), String> {
        if self.closed {
            return Ok(());
        }
        match self.rx.recv_timeout(wait) {
            Ok(item) => self.inbound(item)?,
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                return Err("disconnected: the reader stopped".into());
            }
        }
        while let Ok(item) = self.rx.try_recv() {
            self.inbound(item)?;
        }
        self.tick()
    }

    /// Pumps until `done` holds on the state, or fails on an error or after `limit`.
    pub fn wait(&mut self, limit: Duration, what: &str, done: impl Fn(&State) -> bool) -> Result<(), String> {
        let until = Instant::now() + limit;
        loop {
            if let Some(e) = &self.st.error {
                return Err(e.clone());
            }
            if done(&self.st) {
                return Ok(());
            }
            if Instant::now() >= until {
                return Err(format!("timeout waiting for {what}"));
            }
            if let Err(e) = self.pump(Duration::from_millis(5)) {
                self.st.error.get_or_insert(e.clone());
                return Err(e);
            }
        }
    }

    /// Accepts a teleport: the position the bot is at, with the id the server sent.
    pub fn ack(&mut self, id: i32) -> Result<(), String> {
        let p = &self.st.player;
        let [x, y, z] = p.pos;
        let (yaw, pitch) = (p.yaw, p.pitch);
        self.send(S::AcceptTeleportation(play::AcceptTeleportation { id, x, y, z, yaw, pitch }))
    }

    fn inbound(&mut self, item: Inbound) -> Result<(), String> {
        match item {
            Inbound::Packet(id, body) => self.handle(id, &body),
            Inbound::Closed(e) => Err(format!("disconnected: {e}")),
        }
    }

    fn handle(&mut self, id: i32, body: &[u8]) -> Result<(), String> {
        self.st.record(id, body);
        let p = match C::decode(id, body) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("myriade-bot: packet {id}: {e}");
                return Ok(());
            }
        };
        match &p {
            C::KeepAlive(k) => self.send(S::KeepAlive(cfg::KeepAlive { id: k.id })),
            C::Ping(k) => self.send(S::Pong(cfg::Ping { id: k.id })),
            C::ChunkBatchStart(_) => {
                self.batch.started = Instant::now();
                Ok(())
            }
            C::ChunkBatchFinished(f) => {
                self.batch.finished(f.size);
                let chunks_per_tick = self.batch.desired();
                self.send(S::ChunkBatchReceived(play::ChunkBatchReceived { chunks_per_tick }))
            }
            C::Disconnect(d) => {
                let why = format!("disconnected: {}", text::plain(&d.reason, &Default::default()));
                self.st.error = Some(why.clone());
                Err(why)
            }
            C::PlayerPosition(_) => {
                let loaded = self.st.loaded;
                if let Some(tp) = self.st.apply(&p)
                    && self.auto_ack
                {
                    self.ack(tp)?;
                }
                if !loaded && self.st.spawned {
                    self.st.loaded = true;
                    self.st.player.on_ground = true;
                    self.send(S::PlayerLoaded(Empty))?;
                }
                Ok(())
            }
            _ => {
                self.st.apply(&p);
                Ok(())
            }
        }
    }

    /// One client tick, as vanilla sends it: the position, then the tick end. The cheat timer scales
    /// the period.
    fn tick(&mut self) -> Result<(), String> {
        let now = Instant::now();
        if now < self.next_tick || !self.st.spawned {
            return Ok(());
        }
        let period = Duration::from_secs_f64(0.05 / self.st.cheat.timer);
        self.next_tick = (self.next_tick + period).max(now);
        let [x, y, z] = self.st.player.pos;
        let flags = u8::from(self.st.player.on_ground || self.st.cheat.ground);
        self.send(S::MovePlayerPos(play::MovePlayerPos { x, y, z, flags }))?;
        self.send(S::ClientTickEnd(Empty))
    }
}
