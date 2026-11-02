//! Runs BPMN models written with `bpmn.pl` on an embedded Scryer Prolog
//! machine.
//!
//! A user program must define `model/1`, whose solution is a
//! `bpmn(Id, Items)` term in the `bpmn.pl` DSL. [`Engine::render`] loads the
//! program clause by clause, lints the model with `bpmn_check/2` and turns
//! it into BPMN 2.0 XML with `bpmn_chars/2`.
//!
//! The result types and the [`protocol`] for running an engine in another
//! thread are always available. The [`Engine`] itself needs the `machine`
//! feature (on by default), which pulls in Scryer Prolog.
//!
//! ```
//! let program = bpmn_syntax::check(r#"
//!     :- use_module(bpmn).
//!     model(bpmn(hello, [process(p, "Hello", [start(s), end(e), s => e])])).
//! "#).unwrap();
//!
//! # #[cfg(feature = "machine")] {
//! let mut engine = bpmn_engine::Engine::new();
//! let rendering = engine.render(&program).unwrap();
//! let bpmn_engine::Outcome::Rendered { xml, problems } = rendering.outcome else {
//!     panic!("expected a diagram");
//! };
//! assert!(xml.contains("<bpmn:startEvent id=\"s\""));
//! assert!(problems.is_empty());
//! # }
//! ```

mod error;
#[cfg(feature = "machine")]
mod machine;
pub mod protocol;
mod rendering;

pub use error::EngineError;
#[cfg(feature = "machine")]
pub use machine::{BPMN_LIBRARY, Engine};
pub use rendering::{Diagnostic, Outcome, Problem, Rendering};
