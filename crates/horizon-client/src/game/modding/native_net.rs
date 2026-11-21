//! Private custom-book connection state.
use std::collections::HashMap;

#[derive(Default)]
pub(super) struct NativeNet {
    pub(super) placement: super::native_book_placement::State,
    pub(super) book_connections:
        HashMap<u32, horizon_client::recipe_book::settings_connection::Connection>,
    pub(super) book_view: horizon_client::recipe_book::settings_sync::ClientView,
    pub(super) book_request: u64,
    pub(super) book_ready: bool,
    pub(super) book_retry_at: Option<std::time::Instant>,
    /// A mod's `disconnect` (its reason, if any), carried out at the end of the frame.
    pub(super) leave: Option<super::native_disconnect::LeaveRequest>,
}
