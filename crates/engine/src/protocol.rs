//! Messages for running an [`Engine`](crate) in another thread, such as a
//! Web Worker, encoded as JSON strings.

use serde::{Deserialize, Serialize};

use crate::Rendering;

/// Host → engine thread: render this program.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    /// Identifies the matching [`Response::Rendered`].
    pub id: u64,
    /// The complete program text.
    pub program: String,
}

/// Engine thread → host.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Response {
    /// The engine is built and accepts requests from now on.
    Ready,
    /// The answer to the [`Request`] with the same `id`.
    Rendered {
        /// The request's id.
        id: u64,
        /// The rendering, or why the engine could not produce one.
        result: Result<Rendering, String>,
        /// Time spent in Prolog, in milliseconds.
        millis: f64,
    },
}

macro_rules! impl_json {
    ($($message:ty),*) => {$(
        impl $message {
            /// Encodes the message for `postMessage`.
            #[must_use]
            pub fn to_json(&self) -> String {
                serde_json::to_string(self).expect("messages serialise infallibly")
            }

            /// Decodes a message received from `postMessage`.
            ///
            /// # Errors
            ///
            /// Fails if `json` is not an encoded message of this type.
            pub fn from_json(json: &str) -> serde_json::Result<Self> {
                serde_json::from_str(json)
            }
        }
    )*};
}

impl_json!(Request, Response);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Outcome;

    #[test]
    fn round_trips() {
        let response = Response::Rendered {
            id: 7,
            result: Ok(Rendering {
                outcome: Outcome::NoModel,
                diagnostics: vec![],
                output: "hi\n".to_owned(),
            }),
            millis: 1.5,
        };
        assert_eq!(Response::from_json(&response.to_json()).unwrap(), response);

        let request = Request {
            id: 1,
            program: "model(x).".to_owned(),
        };
        assert_eq!(Request::from_json(&request.to_json()).unwrap(), request);
    }
}
