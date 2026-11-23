//! Drawing values returned by the engine's item property readers.
use super::TryIntoApi;

impl TryIntoApi for horizon_items::properties::ItemBar {
    type Api = myriade_types::ItemBar;
    type Error = std::convert::Infallible;

    fn try_into_api(&self) -> Result<Self::Api, Self::Error> {
        Ok(myriade_types::ItemBar {
            width: self.width,
            color: self.color,
        })
    }
}
