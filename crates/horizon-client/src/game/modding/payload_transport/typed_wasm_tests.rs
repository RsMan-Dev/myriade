//! The compiled wasm fixture's typed payload on a published server: the guest decodes the body,
//! refuses forged and retyped tokens, and replies through its own token; a guest trap after a
//! staged reply sends nothing, closes the session and stops the guest.
use super::typed_tests::{await_disconnect, await_reply, send};
use std::time::{Duration, Instant};

const NAME: &str =
    "game::payload_transport::typed_wasm_tests::compiled_guest_typed_payload_replies_and_traps";
const CHANNEL: &str = "wasm_fixture:typed";

