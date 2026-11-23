//! Small API-owned recipe and input values.
use super::{TryIntoApi, TryIntoEngine};
use std::convert::Infallible;

impl TryIntoApi for horizon_items::firework::FireworkShape {
    type Api = myriade_types::FireworkShape;
    type Error = Infallible;
    fn try_into_api(&self) -> Result<Self::Api, Self::Error> {
        use horizon_items::firework::FireworkShape as E;
        use myriade_types::FireworkShape as A;
        Ok(match self {
            E::SmallBall => A::SmallBall,
            E::LargeBall => A::LargeBall,
            E::Star => A::Star,
            E::Creeper => A::Creeper,
            E::Burst => A::Burst,
        })
    }
}
impl TryIntoEngine<()> for myriade_types::FireworkShape {
    type Engine = horizon_items::firework::FireworkShape;
    type Error = Infallible;
    fn try_into_engine(&self, _: &()) -> Result<Self::Engine, Self::Error> {
        use horizon_items::firework::FireworkShape as E;
        use myriade_types::FireworkShape as A;
        Ok(match self {
            A::SmallBall => E::SmallBall,
            A::LargeBall => E::LargeBall,
            A::Star => E::Star,
            A::Creeper => E::Creeper,
            A::Burst => E::Burst,
            _ => unreachable!("all API firework shapes are handled"),
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn small_value_conversion_cases() {
        use horizon_items::firework::FireworkShape as F;
        for (index, shape) in [F::SmallBall, F::LargeBall, F::Star, F::Creeper, F::Burst]
            .iter()
            .enumerate()
        {
            let api = shape.try_into_api().unwrap();
            assert_eq!(api as usize, index);
            assert_eq!(api.try_into_engine(&()).unwrap(), *shape);
        }
    }
}
