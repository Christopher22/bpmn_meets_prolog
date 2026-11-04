//! The bpmn-js diagram pane.

use bpmn_engine::{Outcome, Problem, Rendering};
use leptos::html;
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::app::{AppState, LastRender};
use crate::bpmn_js::Viewer;

/// CSS class bpmn-js puts on elements that `bpmn_check/2` complains about.
const PROBLEM_MARKER: &str = "bpmn-problem";

/// What the viewer needs from a successful render.
#[derive(Debug, Clone, PartialEq)]
struct Drawing {
    xml: String,
    flagged: Vec<String>,
}

impl Drawing {
    fn of(last: Option<&LastRender>) -> Option<Self> {
        match last?.result.as_ref().ok()? {
            Rendering {
                outcome: Outcome::Rendered { xml, problems },
                ..
            } => Some(Self {
                xml: xml.clone(),
                flagged: problems
                    .iter()
                    .filter_map(Problem::element)
                    .map(str::to_owned)
                    .collect(),
            }),
            _ => None,
        }
    }
}

/// Shows the latest successfully rendered model. Failed renders leave the
/// previous diagram in place, and the viewport survives re-imports.
#[component]
pub(crate) fn Diagram() -> impl IntoView {
    let state = expect_context::<AppState>();
    let container = NodeRef::<html::Div>::new();
    let viewer = StoredValue::new_local(None::<Viewer>);
    let error = RwSignal::new(None::<String>);
    let shown = RwSignal::new(false);
    let generation = StoredValue::new(0_u64);

    // Equal XML (say, after editing a comment) is not imported again.
    let drawing = Memo::new(move |_| state.last.with(|last| Drawing::of(last.as_ref())));

    Effect::new(move |_| {
        let Some(Drawing { xml, flagged }) = drawing.get() else {
            return;
        };
        let Some(element) = container.get() else {
            return;
        };
        let current = match viewer.get_value() {
            Some(current) => current,
            None => match Viewer::new(&element) {
                Ok(created) => {
                    viewer.set_value(Some(created.clone()));
                    created
                }
                Err(cause) => {
                    error.set(Some(format!("bpmn-js is not available: {cause:?}")));
                    return;
                }
            },
        };

        generation.update_value(|count| *count += 1);
        let this_import = generation.get_value();
        let fit = !shown.get_untracked() || state.fit_diagram.get_value();
        state.fit_diagram.set_value(false);
        spawn_local(async move {
            let result = current.show(&xml, &flagged, PROBLEM_MARKER, fit).await;
            // A newer import supersedes this one (or the pane is gone).
            if generation.try_get_value() != Some(this_import) {
                return;
            }
            match result {
                Ok(()) => {
                    shown.set(true);
                    error.set(None);
                }
                Err(message) => error.set(Some(message)),
            }
        });
    });

    on_cleanup(move || {
        if let Some(Some(current)) = viewer.try_get_value() {
            current.dispose();
        }
    });

    view! {
        <div class="canvas" node_ref=container></div>
        <Show when=move || !shown.get() && error.with(Option::is_none)>
            <p class="overlay">"Waiting for the first model…"</p>
        </Show>
        {move || error.get().map(|message| view! {
            <p class="overlay error">"Cannot show the diagram: " {message}</p>
        })}
    }
}
