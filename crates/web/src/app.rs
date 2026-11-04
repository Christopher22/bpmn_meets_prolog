//! The page: shared state, the edit → check → render pipeline and layout.

use bpmn_engine::{Outcome, Rendering};
use gloo_timers::callback::Timeout;
use leptos::html;
use leptos::prelude::*;

use crate::client::{Client, Event};
use crate::components::{Diagram, Editor, Status, Toolbar};
use crate::syntax::SyntaxReport;

/// The program shown on first load.
const EXAMPLE: &str = include_str!("example.pl");

/// The worker's loader shim, emitted by Trunk next to `index.html`.
const WORKER_SCRIPT: &str = "./worker_loader.js";

/// Quiet period after the last keystroke before Prolog runs.
const DEBOUNCE_MS: u32 = 250;

/// How long a render may take before the worker is replaced.
pub(crate) const TIMEOUT_MS: u32 = 10_000;

/// What the Prolog worker is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EngineState {
    Starting,
    Ready,
    Busy,
    Unavailable,
}

/// Something went wrong with the worker since the last render.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Notice {
    TimedOut,
    Crashed(String),
    Unavailable(String),
}

/// The most recent answer from the worker.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LastRender {
    /// The program this answers.
    pub(crate) program: String,
    pub(crate) result: Result<Rendering, String>,
    pub(crate) millis: f64,
}

impl LastRender {
    /// The BPMN 2.0 XML, if the model could be rendered.
    pub(crate) fn xml(&self) -> Option<&str> {
        match &self.result {
            Ok(Rendering {
                outcome: Outcome::Rendered { xml, .. },
                ..
            }) => Some(xml),
            _ => None,
        }
    }
}

/// Signals shared by all components (provided as context).
#[derive(Clone, Copy)]
pub(crate) struct AppState {
    pub(crate) source: RwSignal<String>,
    pub(crate) syntax: Memo<SyntaxReport>,
    pub(crate) engine: RwSignal<EngineState>,
    pub(crate) notice: RwSignal<Option<Notice>>,
    pub(crate) last: RwSignal<Option<LastRender>>,
    pub(crate) editor: NodeRef<html::Textarea>,
    /// Set when another program was opened: the next diagram is not just an
    /// edit of the current one, so its viewport is not kept.
    pub(crate) fit_diagram: StoredValue<bool>,
}

/// The whole page: a title above the editor (left) and the diagram (right).
#[component]
pub fn App() -> impl IntoView {
    let source = RwSignal::new(EXAMPLE.to_owned());
    let state = AppState {
        source,
        syntax: Memo::new(move |_| source.with(|text| SyntaxReport::of(text))),
        engine: RwSignal::new(EngineState::Starting),
        notice: RwSignal::new(None),
        last: RwSignal::new(None),
        editor: NodeRef::new(),
        fit_diagram: StoredValue::new(false),
    };
    provide_context(state);

    let client = Client::new(WORKER_SCRIPT, TIMEOUT_MS, move |event| match event {
        Event::Starting => state.engine.set(EngineState::Starting),
        Event::Ready => state.engine.set(EngineState::Ready),
        Event::Busy => state.engine.set(EngineState::Busy),
        Event::Rendered {
            program,
            result,
            millis,
        } => {
            state.notice.set(None);
            state.last.set(Some(LastRender {
                program,
                result,
                millis,
            }));
        }
        Event::TimedOut => state.notice.set(Some(Notice::TimedOut)),
        Event::Crashed(message) => state.notice.set(Some(Notice::Crashed(message))),
        Event::Unavailable(message) => {
            state.engine.set(EngineState::Unavailable);
            state.notice.set(Some(Notice::Unavailable(message)));
        }
    });

    // Lexically valid programs go to Prolog once typing pauses. Replacing the
    // stored timeout cancels the previous one.
    let debounce = StoredValue::new_local(None::<Timeout>);
    Effect::new(move |_| {
        let valid = state.syntax.with(|report| report.result.is_ok());
        let program = state.source.get();
        let client = client.clone();
        debounce
            .set_value(valid.then(|| Timeout::new(DEBOUNCE_MS, move || client.submit(program))));
    });

    view! {
        <header class="title">
            <div>
                <h1>"BPMN in Prolog"</h1>
                <p>
                    "Model BPMN as Prolog terms, reason about them with Scryer Prolog "
                    "and see them drawn by bpmn-js:"
                </p>
            </div>
            <Toolbar />
        </header>
        <main class="split">
            <section class="pane editor-pane" aria-label="Prolog program">
                <Editor initial=EXAMPLE />
                <Status />
            </section>
            <section class="pane diagram-pane" aria-label="Diagram">
                <Diagram />
            </section>
        </main>
    }
}
