use super::*;
use horizon_protocol::connection::{Connection, Pipe};
use std::time::Duration;

#[test]
fn terminal_disconnect_drains_even_when_last_handle_drops() {
    let (a, b) = Pipe::pair();
    let (_, physical) = Connection::local(a).split();
    let queue = QueuedWriter::new(physical).unwrap();
    let mut peer = Connection::local(b);
    peer.reader
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    queue.send(&[1, 11]).unwrap();
    queue.disconnect(vec![2, 22]);
    assert!(queue.send(&[3]).is_err());
    drop(queue);
    assert_eq!(peer.recv().unwrap(), (1, vec![11]));
    assert_eq!(peer.recv().unwrap(), (2, vec![22]));
    assert!(peer.recv().is_err());
}
