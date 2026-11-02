use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A failure of the engine itself, as opposed to a problem in the program.
#[derive(Debug, Clone, PartialEq, Eq, Error, Serialize, Deserialize)]
pub enum EngineError {
    /// The driver raised an exception it should have caught.
    #[error("the Prolog driver raised an unexpected exception")]
    Crashed,
    /// The driver answered with a term of an unexpected shape.
    #[error("unexpected answer from the Prolog driver: {0}")]
    UnexpectedAnswer(String),
}
