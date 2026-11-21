//! The host's outputs (`HostOut`) drained into a typed list, in the order they were written, into the caller's buffer (its capacity
//! is kept). The server reads the list once per tick.
use crate::host_out::{HostEvent, HostOut};

pub fn drain_typed(out: &mut HostOut, into: &mut Vec<HostEvent>) {
    into.extend(out.drain());
}
