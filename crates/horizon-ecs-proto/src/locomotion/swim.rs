//! The swim (step 2).
use crate::behaviours::{Cap, Module};

#[derive(Clone, Copy, Debug)]
pub struct Params {}
impl Params {
    pub const DEFAULT: Params = Params {};
}
pub static MODULE: Module = Module { name: "swim", requires: &[Cap::Locomotion], ..Module::DEFAULT };
