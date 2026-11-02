//! The Scryer Prolog machine behind [`Engine`].

mod decode;
mod output;
mod query;

use bpmn_syntax::Program;
use scryer_prolog::{LeafAnswer, Machine, MachineBuilder, StreamConfig, Term};

use self::output::Capture;
use crate::{EngineError, Outcome, Rendering};

/// The `bpmn.pl` DSL, compiled into the engine.
pub const BPMN_LIBRARY: &str = include_str!("../../../../prolog/bpmn.pl");

/// The host driver that loads user programs (see `prolog/driver.pl`).
const DRIVER: &str = include_str!("../../prolog/driver.pl");

/// User output beyond this many bytes is dropped.
const OUTPUT_LIMIT: usize = 64 * 1024;

/// A Prolog machine with `bpmn.pl` and the host driver loaded.
///
/// Building an engine compiles the Prolog libraries, which is by far the
/// most expensive step; reuse one engine for many renders.
pub struct Engine {
    machine: Machine,
    output: Capture,
}

impl Engine {
    /// Builds a machine and loads `bpmn.pl` and the driver into it.
    #[must_use]
    pub fn new() -> Self {
        let output = Capture::new(OUTPUT_LIMIT);
        // The user input side is dropped on purpose: reads see end of file.
        let (_input, streams) =
            StreamConfig::from_callbacks(Some(output.callback()), Some(output.callback()));
        let mut machine = MachineBuilder::default().with_streams(streams).build();
        machine.load_module_string("bpmn", BPMN_LIBRARY);
        machine.load_module_string("driver", DRIVER);
        Self { machine, output }
    }

    /// Loads `program`, replacing the previously rendered program, and
    /// renders its `model/1`.
    ///
    /// Problems in the program itself (syntax errors Prolog finds, failing
    /// directives, a missing or malformed model) are reported inside the
    /// [`Rendering`], not as errors.
    ///
    /// # Errors
    ///
    /// Returns an [`EngineError`] only if the driver itself misbehaves.
    pub fn render(&mut self, program: &Program<'_>) -> Result<Rendering, EngineError> {
        self.output.clear();
        let answer = match self.run(&query::render(program)) {
            Some(Ok(LeafAnswer::LeafAnswer { mut bindings, .. })) => bindings.remove("R"),
            _ => None,
        }
        .ok_or_else(|| EngineError::UnexpectedAnswer("render/2 did not bind R".to_owned()))?;
        let mut rendering = decode::rendering(&answer)?;
        rendering.output = self.output.take();

        if let Outcome::Rendered { xml, .. } = &mut rendering.outcome {
            // The diagram is the driver's own output and must not be cut off.
            self.output.set_limit(usize::MAX);
            let answer = self.run("driver:write_diagram.");
            *xml = self.output.take();
            self.output.set_limit(OUTPUT_LIMIT);
            if !matches!(answer, Some(Ok(LeafAnswer::True))) {
                return Err(EngineError::UnexpectedAnswer(format!("{answer:?}")));
            }
        }
        Ok(rendering)
    }

    /// Runs a query that succeeds exactly once and returns its answer.
    fn run(&mut self, query: &str) -> Option<Result<LeafAnswer, Term>> {
        let mut answers = self.machine.run_query(query);
        let first = answers.next();
        // The query is deterministic, so this only lets the iterator finish
        // cleanly before it is dropped.
        while answers.next().is_some() {}
        first
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}
