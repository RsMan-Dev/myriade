//! Phase vocabulary enters through the same game-local conversion boundary.
//! The two mappings repeat `myriade_level::api_context::{load_origin, loop_side}`,
//! which this crate cannot call.
use super::TryIntoApi;
use horizon_world::native_load_context::{DataLoadOrigin, LoopSide};
use std::convert::Infallible;

impl TryIntoApi for DataLoadOrigin {
    type Api = myriade_types::DataLoadOrigin;
    type Error = Infallible;
    fn try_into_api(&self) -> Result<Self::Api, Self::Error> {
        Ok(match self {
            DataLoadOrigin::ServerDataLoad => myriade_types::DataLoadOrigin::ServerDataLoad,
            _ => unreachable!("all engine load origins are mapped"),
        })
    }
}
impl TryIntoApi for LoopSide {
    type Api = myriade_types::LoopSide;
    type Error = Infallible;
    fn try_into_api(&self) -> Result<Self::Api, Self::Error> {
        Ok(match self {
            LoopSide::Server => myriade_types::LoopSide::Server,
            _ => unreachable!("all engine loop sides are mapped"),
        })
    }
}
