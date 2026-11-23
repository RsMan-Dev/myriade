use crate::*;


/// Menu-owned roles, never indices into the appended player inventory.
#[derive(Clone, Debug, PartialEq, Eq, )]
pub enum RecipeBookLayout {
    Crafting {
        width: u16,
        height: u16,
        slots: Vec<u16>,
        result: u16,
    },
    Furnace {
        input: u16,
        fuel: u16,
        result: u16,
    },
    Stonecutter {
        input: u16,
        result: u16,
    },
    Smithing {
        template: u16,
        base: u16,
        addition: u16,
        result: u16,
    },
}

impl RecipeBookLayout {
    /// Check role indices against the content menu's container slot count.
    /// Loading must call this with the actual count, before publishing a layout.
    pub fn validate(&self, slot_count: u16) -> Result<(), String> {
        if slot_count == 0 || slot_count > 256 {
            return Err("container slot count must be in 1..=256".into());
        }
        let slots = match self {
            Self::Crafting {
                width,
                height,
                slots,
                result,
            } => {
                let cells = u32::from(*width) * u32::from(*height);
                if *width == 0
                    || *height == 0
                    || cells > u32::from(slot_count)
                    || cells as usize != slots.len()
                {
                    return Err("grid dimensions must match its slot list and fit the menu".into());
                }
                slots
                    .iter()
                    .map(|slot| (*slot, "grid"))
                    .chain([(*result, "result")])
                    .collect::<Vec<_>>()
            }
            Self::Furnace {
                input,
                fuel,
                result,
            } => vec![(*input, "input"), (*fuel, "fuel"), (*result, "result")],
            Self::Stonecutter { input, result } => vec![(*input, "input"), (*result, "result")],
            Self::Smithing {
                template,
                base,
                addition,
                result,
            } => vec![
                (*template, "template"),
                (*base, "base"),
                (*addition, "addition"),
                (*result, "result"),
            ],
        };
        let mut seen = [false; 256];
        for (slot, role) in slots {
            if slot >= slot_count {
                return Err(format!("{role} slot {slot} is outside the menu"));
            }
            if seen[usize::from(slot)] {
                return Err(format!("{role} slot {slot} overlaps another role"));
            }
            seen[usize::from(slot)] = true;
        }
        Ok(())
    }
}
