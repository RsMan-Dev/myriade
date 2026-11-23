use super::super::{TryIntoApi, TryIntoEngine};
use horizon_core::arm_pose::ArmPose as Engine;
use mod_api::ArmPose as Api;

macro_rules! poses {
    ($($pose:ident),* $(,)?) => {
        impl TryIntoApi for Engine {
            type Api = Api;
            type Error = std::convert::Infallible;
            fn try_into_api(&self) -> Result<Api, Self::Error> {
                Ok(match self { $(Engine::$pose => Api::$pose,)* Engine::Custom { .. } => Api::Empty, })
            }
        }
        impl TryIntoEngine<()> for Api {
            type Engine = Engine;
            type Error = String;
            fn try_into_engine(&self, _: &()) -> Result<Engine, Self::Error> {
                Ok(match self { $(Api::$pose => Engine::$pose,)* })
            }
        }
    };
}
poses!(
    Empty,
    Item,
    Block,
    Bow,
    CrossbowHold,
    CrossbowCharge,
    ThrowTrident,
    TootHorn,
    Spyglass,
    Spear,
    Brush
);
