//! Registered raw payload sends; compatibility and pressure are connection-owned.
#![allow(unused_variables)]
use crate::*;

/// Player-targeted sends, exposed naturally through [`PlayerRef::send_payload`].
#[horizon_mod_api_macros::host_area]
pub trait PayloadsBackendMut {
    /// Side: server.
    /// Queue exact application bytes for this live player's connection. The channel
    /// must be declared locally and negotiated for the current phase and direction.
    /// Publication waits for root commit; abort publishes nothing. Queued is not a
    /// delivery acknowledgement. No generic registered-body or per-tick count cap applies.
    #[access(WorldOnly)]
    fn player_send_payload(
        &mut self,
        player: PlayerRef,
        id: &PayloadId,
        bytes: PayloadBytes,
    ) -> Result<PayloadAccepted, PayloadSendError>;
}

