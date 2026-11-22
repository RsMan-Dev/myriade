//! Whole-aggregate block-state boundaries. No partial snapshot or draw is admitted.
use super::{TryIntoApi, block_state};
use crate::game::native_structures::TemplateSnapshot;
use mod_api::{BlockStateError, StructureTemplateBlock, StructureTemplateSnapshot};

impl TryIntoApi for TemplateSnapshot<'_> {
    type Api = StructureTemplateSnapshot;
    type Error = BlockStateError;
    fn try_into_api(&self) -> Result<Self::Api, Self::Error> {
        let index = block_state::current()?;
        let blocks = self
            .blocks
            .iter()
            .map(|block| {
                Ok(StructureTemplateBlock::new(
                    block.pos,
                    index.to_api(block.state)?,
                    block.nbt.cloned(),
                ))
            })
            .collect::<Result<Vec<_>, BlockStateError>>()?;
        Ok(StructureTemplateSnapshot::new(self.size, blocks))
    }
}
