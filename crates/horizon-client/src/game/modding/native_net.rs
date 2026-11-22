//! Private custom-book connection state.
use std::collections::HashMap;

#[derive(Default)]
pub(super) struct NativeNet {
    pub(super) placement: super::native_book_placement::State,
    pub(super) book_connections:
        HashMap<u32, horizon_client::recipe_book::settings_connection::Connection>,
}
