use crate::*;


/// `InteractionHand`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hand {
    Main,
    Off,
}

impl Hand {
    pub const BOTH: [Hand; 2] = [Hand::Main, Hand::Off];
}
