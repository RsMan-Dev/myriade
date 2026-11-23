use crate::*;


/// Why a custom argument's parser refused the text a player wrote.
#[derive(Clone, Debug, PartialEq)]
pub enum CommandArgumentError {
    /// The message shown to the player.
    Message(Box<Text>),
}

impl CommandArgumentError {
    /// A refusal with the message the player sees.
    pub fn new(message: Text) -> Self {
        Self::Message(Box::new(message))
    }

    /// The message the player sees.
    pub fn message(&self) -> &Text {
        let CommandArgumentError::Message(message) = self;
        message
    }
}

impl From<(crate::Identifier, String)> for CommandArgumentError {
    fn from((_, message): (crate::Identifier, String)) -> Self {
        Self::new(Text::literal(message))
    }
}

impl From<CommandArgumentError> for Text {
    fn from(error: CommandArgumentError) -> Self {
        let CommandArgumentError::Message(message) = error;
        *message
    }
}
