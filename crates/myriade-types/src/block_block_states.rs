use crate::*;


/// A property constraint used when matching a state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateConstraint {
    /// Property name.
    pub name: String,
    /// Required serialized value.
    pub value: String,
}

impl StateConstraint {
    /// Construct a property constraint.
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }
}
