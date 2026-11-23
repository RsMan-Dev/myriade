//! One bounded writer owner. Receiving callbacks never block their own receive loop
//! while a large reply waits for the peer to drain its socket or memory pipe.
use crate::net::byte_channel::{self, Sender, TrySendError};
use horizon_protocol::{
    Error, Result,
    connection::{Closer, ConnWriter, Framing},
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

use std::thread::JoinHandle;

const CAPACITY: usize = 16 * 1024 * 1024;
enum WriteJob {
    Packet(Vec<u8>),
    Disconnect(Vec<u8>),
}

#[derive(Clone)]
pub(crate) struct QueuedWriter(Arc<Inner>);
pub(crate) struct Inner {
    #[cfg(any(test, feature = "test-payload-probe"))]
    // attempts, successful send returns, active input bytes, active calls.
    // ConnWriter::send_contents returns after write_all; it does not flush.
    probe: Arc<[std::sync::atomic::AtomicUsize; 4]>,
    sender: Mutex<Option<Sender<WriteJob>>>,
    worker: Mutex<Option<JoinHandle<ConnWriter>>>,
    framing: Framing,
    closer: Arc<Closer>,
    alive: Arc<AtomicBool>,
    retired: AtomicBool,
    terminal: AtomicBool,
}
impl std::ops::Deref for QueuedWriter {
    type Target = Inner;
    fn deref(&self) -> &Inner {
        &self.0
    }
}

impl QueuedWriter {
    #[cfg(any(test, feature = "test-payload-probe"))]
    pub(crate) fn diagnostics(&self) -> String {
        let stats = match self.sender.try_lock() {
            Ok(sender) => sender.as_ref().map(|s| s.stats()),
            Err(_) => return "writer sender busy/poisoned".into(),
        };
        let window = stats.map(|s| CAPACITY.saturating_sub(s.reserved_bytes));
        format!(
            "writer window={window:?} capacity={CAPACITY} alive={} terminal={} retired={} stats={stats:?} send_attempts={} send_ok={} active_send_input_bytes={} active_sends={} flush_count=unavailable(no flush call)",
            self.alive.load(Ordering::Acquire),
            self.terminal.load(Ordering::Acquire),
            self.retired.load(Ordering::Acquire),
            self.probe[0].load(Ordering::Relaxed),
            self.probe[1].load(Ordering::Relaxed),
            self.probe[2].load(Ordering::Relaxed),
            self.probe[3].load(Ordering::Relaxed)
        )
    }
    pub(crate) fn new(mut writer: ConnWriter) -> Result<Self> {
        let framing = writer.framing();
        let closer = Arc::new(writer.closer()?);
        let alive = Arc::new(AtomicBool::new(true));
        let (sender, receiver) = byte_channel::channel(CAPACITY).expect("nonzero writer capacity");
        #[cfg(any(test, feature = "test-payload-probe"))]
        let probe = Arc::new(std::array::from_fn::<_, 4, _>(|_| {
            std::sync::atomic::AtomicUsize::new(0)
        }));
        #[cfg(any(test, feature = "test-payload-probe"))]
        let writing = probe.clone();
        let running = alive.clone();
        let worker = std::thread::Builder::new()
            .name("horizon-net-write".into())
            .spawn(move || {
                while let Ok(job) = receiver.recv() {
                    let disconnect = matches!(&job, WriteJob::Disconnect(_));
                    #[cfg(any(test, feature = "test-payload-probe"))]
                    {
                        let bytes = match &job {
                            WriteJob::Packet(b) | WriteJob::Disconnect(b) => b.len(),
                        };
                        writing[0].fetch_add(1, Ordering::Relaxed);
                        writing[2].store(bytes, Ordering::Relaxed);
                        writing[3].store(1, Ordering::Relaxed);
                    }
                    let result = match job {
                        WriteJob::Packet(body) | WriteJob::Disconnect(body) => writer.send(&body),
                    };
                    #[cfg(any(test, feature = "test-payload-probe"))]
                    {
                        if result.is_ok() {
                            writing[1].fetch_add(1, Ordering::Relaxed);
                        }
                        writing[2].store(0, Ordering::Relaxed);
                        writing[3].store(0, Ordering::Relaxed);
                    }
                    if result.is_err() || disconnect {
                        running.store(false, Ordering::Release);
                        writer.shutdown();
                        break;
                    }
                }
                writer
            })
            .map_err(|error| Error::new(format!("cannot start client writer: {error}")))?;
        Ok(Self(Arc::new(Inner {
            #[cfg(any(test, feature = "test-payload-probe"))]
            probe,
            sender: Mutex::new(Some(sender)),
            worker: Mutex::new(Some(worker)),
            retired: AtomicBool::new(false),
            terminal: AtomicBool::new(false),
            framing,
            closer,
            alive,
        })))
    }
    /// Stop acceptance, cancel unpublished reservations and drain published writes.
    pub(crate) fn drain_and_take(&self) -> Result<ConnWriter> {
        let mut sender = self.sender.lock().unwrap();
        self.retired.store(true, Ordering::Release);
        sender.take();
        drop(sender);
        let worker = self
            .worker
            .lock()
            .unwrap()
            .take()
            .ok_or_else(|| Error::new("writer already taken"))?;
        let writer = worker.join().map_err(|_| Error::new("writer panicked"))?;
        if !self.alive.swap(false, Ordering::AcqRel) {
            return Err(Error::new("writer disconnected"));
        }
        Ok(writer)
    }
    pub(crate) fn is_terminal(&self) -> bool {
        self.terminal.load(Ordering::Acquire)
    }
    pub(crate) fn framing(&self) -> Framing {
        self.framing
    }
    pub(crate) fn shutdown(&self) {
        if self.retired.load(Ordering::Acquire) {
            return;
        }
        self.alive.store(false, Ordering::Release);
        if !self.retired.load(Ordering::Acquire) {
            self.closer.close();
        }
        self.sender.lock().unwrap().take();
    }
    pub(crate) fn send(&self, body: &[u8]) -> Result<()> {
        if !self.alive.load(Ordering::Acquire) {
            return Err(Error::new("client writer disconnected"));
        }
        let owned = body.to_vec();
        let capacity = owned.capacity();
        let sender = self.sender.lock().unwrap();
        let Some(sender) = sender.as_ref() else {
            return Err(Error::new("writer retired"));
        };
        match sender.try_send(WriteJob::Packet(owned), capacity) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => {
                self.alive.store(false, Ordering::Release);
                self.closer.close();
                Err(Error::new("client writer backpressure"))
            }
            Err(TrySendError::Disconnected(_)) => Err(Error::new("client writer disconnected")),
        }
    }
    pub(crate) fn disconnect(&self, body: Vec<u8>) {
        let mut sender = self.sender.lock().unwrap();
        self.terminal.store(true, Ordering::Release);
        let Some(queue) = sender.take() else {
            return;
        };
        let bytes = body.capacity();
        if queue.try_send(WriteJob::Disconnect(body), bytes).is_err() {
            self.closer.close();
        }
    }
}
#[cfg(test)]
mod tests;

impl Drop for Inner {
    fn drop(&mut self) {
        if !self.retired.load(Ordering::Acquire) && !self.terminal.load(Ordering::Acquire) {
            self.closer.close();
        }
    }
}
