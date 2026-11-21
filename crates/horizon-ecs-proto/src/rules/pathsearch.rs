use horizon_client_lib::mobs::path::PathType;

/// `WalkNodeEvaluator.getPathTypeStatic` on an open cell: the type it takes from the block below (`None`: the neighbours decide it).
pub fn type_on_below(below: PathType) -> Option<PathType> {
    match below {
        PathType::Open | PathType::Water | PathType::Lava | PathType::Walkable => Some(PathType::Open),
        PathType::Fire => Some(PathType::Fire),
        PathType::Damaging => Some(PathType::Damaging),
        PathType::StickyHoney => Some(PathType::StickyHoney),
        PathType::PowderSnow => Some(PathType::OnTopOfPowderSnow),
        PathType::DamageCautious => Some(PathType::DamageCautious),
        PathType::Trapdoor => Some(PathType::OnTopOfTrapdoor),
        _ => None,
    }
}
