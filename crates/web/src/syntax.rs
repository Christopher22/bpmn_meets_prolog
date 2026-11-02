//! The instant, main-thread syntax check behind the status line.

use bpmn_syntax::SyntaxError;

/// The result of checking the editor content once.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SyntaxReport {
    pub(crate) result: Result<Summary, SyntaxError>,
    /// How long the check took, in milliseconds.
    pub(crate) millis: f64,
}

/// What the check learned about a lexically valid program.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Summary {
    pub(crate) clauses: usize,
    pub(crate) defines_model: bool,
}

impl SyntaxReport {
    pub(crate) fn of(source: &str) -> Self {
        let started = now();
        let result = bpmn_syntax::check(source).map(|program| Summary {
            clauses: program.clauses().len(),
            defines_model: program.defines("model", 1),
        });
        Self {
            result,
            millis: now() - started,
        }
    }
}

fn now() -> f64 {
    web_sys::window()
        .and_then(|window| window.performance())
        .map_or(0.0, |performance| performance.now())
}
