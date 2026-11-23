//! Lot 37 : les entités fixes et générales (porte-armures, cadres, tableaux, affichages, entités de mods) côté hôte. Ce
//! fichier ne porte que les lectures par id qui n'ont pas de point de coupe dans les fichiers du lot ; le reste reste à brancher.
use super::super::Game;

impl Game<'_> {
    /// The kind of the followed entity `id` in the ECS level (`EcsLevel::kind`, through its id table): None when the level is
    /// inert, or the id is unknown or asleep.
    pub(in crate::game) fn fixed_kind(&self, id: u32) -> Option<String> {
        self.ecs.kind(id).map(str::to_owned)
    }
}
