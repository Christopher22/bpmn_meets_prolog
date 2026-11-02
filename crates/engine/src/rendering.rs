//! The result of rendering a program.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Everything a render produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rendering {
    /// What became of `model/1`.
    pub outcome: Outcome,
    /// Clauses or directives that could not be loaded.
    pub diagnostics: Vec<Diagnostic>,
    /// Text the program wrote to `user_output` or `user_error`.
    pub output: String,
}

/// What became of the program's `model/1`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    /// The model was turned into BPMN 2.0 XML.
    Rendered {
        /// The BPMN 2.0 XML document, including diagram interchange.
        xml: String,
        /// Findings of `bpmn_check/2`.
        problems: Vec<Problem>,
    },
    /// The program does not define `model/1`.
    NoModel,
    /// `model/1` failed, threw, or did not yield a `bpmn/2` term.
    Failed {
        /// A description of the failure.
        message: String,
    },
}

/// A clause or directive that could not be loaded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    /// The line the clause starts on.
    pub line: u32,
    /// What went wrong.
    pub message: String,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

/// A finding of `bpmn_check/2`, such as `no_incoming(task_a)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Problem {
    /// The problem's functor, e.g. `no_incoming`.
    pub code: String,
    /// The problem's arguments: element ids, or other terms in `~q` form.
    pub subjects: Vec<String>,
}

impl Problem {
    /// The id of the diagram element the problem is primarily about.
    #[must_use]
    pub fn element(&self) -> Option<&str> {
        self.subjects.first().map(String::as_str)
    }
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [a, b] = [0, 1].map(|i| self.subjects.get(i).map_or("?", String::as_str));
        match self.code.as_str() {
            "duplicate_id" => write!(f, "id {a} is used more than once"),
            "invalid_id" => write!(f, "{a} is not a valid XML id"),
            "unknown_flow_end" => write!(f, "sequence flow {a} refers to unknown node {b}"),
            "flow_crosses_container" => {
                write!(
                    f,
                    "sequence flow {a} crosses a pool or sub-process boundary"
                )
            }
            "start_has_incoming" => write!(f, "start event {a} has an incoming flow"),
            "end_has_outgoing" => write!(f, "end event {a} has an outgoing flow"),
            "boundary_has_incoming" => write!(f, "boundary event {a} has an incoming flow"),
            "no_incoming" => write!(f, "{a} has no incoming sequence flow"),
            "no_outgoing" => write!(f, "{a} has no outgoing sequence flow"),
            "bad_attachment" => write!(
                f,
                "boundary event {a} is attached to {b}, which is not an activity next to it"
            ),
            "bad_trigger" => write!(f, "event {a} cannot have the trigger {b}"),
            "event_gateway_target" => write!(
                f,
                "event-based gateway {a} leads to {b}, which is no catch event or receive task"
            ),
            "default_not_allowed" => write!(f, "{a} cannot be a default flow from its source"),
            "several_defaults" => write!(f, "{a} has more than one default flow"),
            "unknown_message_end" => write!(f, "message flow {a} refers to unknown element {b}"),
            "message_flow_inside_pool" => write!(f, "message flow {a} stays inside one pool"),
            "unknown_lane_member" => write!(f, "lane {a} lists unknown node {b}"),
            "not_in_any_lane" => write!(f, "{a} is not in any lane of its pool"),
            code => write!(f, "{code}({})", self.subjects.join(", ")),
        }
    }
}
