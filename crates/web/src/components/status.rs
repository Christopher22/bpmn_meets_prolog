//! The panel below the editor: syntax, Prolog status and findings.

use bpmn_engine::{Outcome, Rendering};
use leptos::prelude::*;

use super::editor::reveal;
use crate::app::{AppState, EngineState, LastRender, Notice, TIMEOUT_MS};

#[component]
pub(crate) fn Status() -> impl IntoView {
    view! {
        <div class="status" aria-live="polite">
            <SyntaxLine />
            <EngineLine />
            <Findings />
        </div>
    }
}

#[component]
fn SyntaxLine() -> impl IntoView {
    let state = expect_context::<AppState>();
    move || {
        state.syntax.with(|report| match &report.result {
            Ok(summary) => {
                let model = (!summary.defines_model).then(|| {
                    view! {
                        <p class="line warn">"model/1 is not defined, so there is nothing to draw."</p>
                    }
                });
                let clauses = match summary.clauses {
                    1 => "1 clause".to_owned(),
                    n => format!("{n} clauses"),
                };
                view! {
                    <p class="line ok">
                        "✓ Syntax OK · " {clauses} " · checked in " {millis(report.millis)}
                    </p>
                    {model}
                }
                .into_any()
            }
            Err(error) => {
                let position = error.position;
                view! {
                    <button
                        class="line error jump"
                        title="Show in editor"
                        on:click=move |_| reveal(state.editor, position.line, position.column)
                    >
                        "✗ " {error.to_string()}
                    </button>
                }
                .into_any()
            }
        })
    }
}

#[component]
fn EngineLine() -> impl IntoView {
    let state = expect_context::<AppState>();
    move || {
        let notice = state.notice.get();
        let stale = (state.syntax.with(|report| report.result.is_err()) || notice.is_some())
            && state.last.with(Option::is_some);
        let stale = stale.then(|| {
            view! {
                <p class="line muted">"Diagram and findings are from the last successful run."</p>
            }
        });
        let engine = match state.engine.get() {
            EngineState::Starting => "⏳ Starting Prolog…".to_owned(),
            EngineState::Busy => "⏳ Prolog is running…".to_owned(),
            EngineState::Unavailable => "✗ Prolog is unavailable".to_owned(),
            EngineState::Ready => {
                let took = state
                    .last
                    .with(|last| last.as_ref().map(|last| last.millis));
                match (&notice, took) {
                    (None, Some(ms)) => format!("✓ Prolog finished in {}", millis(ms)),
                    _ => "✓ Prolog is ready".to_owned(),
                }
            }
        };
        let notice = notice.map(|notice| {
            let text = match notice {
                Notice::TimedOut => format!(
                    "⚠ Prolog did not answer within {} s and was restarted.",
                    TIMEOUT_MS / 1000
                ),
                Notice::Crashed(message) => {
                    format!("⚠ Prolog crashed and was restarted: {message}")
                }
                Notice::Unavailable(message) => {
                    format!("✗ Prolog could not start (reload to retry): {message}")
                }
            };
            view! { <p class="line error">{text}</p> }
        });
        view! {
            <p class="line">{engine}</p>
            {notice}
            {stale}
        }
    }
}

#[component]
fn Findings() -> impl IntoView {
    let state = expect_context::<AppState>();
    move || {
        state.last.with(|last| {
            let Some(LastRender { result, .. }) = last else {
                return ().into_any();
            };
            let rendering = match result {
                Ok(rendering) => rendering,
                Err(message) => {
                    return view! { <p class="line error">"Engine error: " {message.clone()}</p> }
                        .into_any();
                }
            };
            let Rendering {
                outcome,
                diagnostics,
                output,
            } = rendering;

            let outcome = match outcome {
                Outcome::Rendered { problems, .. } if problems.is_empty() => {
                    view! { <p class="line ok">"✓ bpmn_check/2 found no problems"</p> }.into_any()
                }
                Outcome::Rendered { problems, .. } => view! {
                    <p class="line warn">"bpmn_check/2 found " {problems.len()} " problem(s):"</p>
                    <ul class="findings">
                        {problems.iter().map(|problem| view! { <li>{problem.to_string()}</li> }).collect_view()}
                    </ul>
                }
                .into_any(),
                Outcome::NoModel => view! { <p class="line warn">"No model/1 to render."</p> }.into_any(),
                Outcome::Failed { message } => {
                    view! { <p class="line error">"model/1: " {message.clone()}</p> }.into_any()
                }
            };

            let diagnostics = (!diagnostics.is_empty()).then(|| {
                let items = diagnostics
                    .iter()
                    .map(|diagnostic| {
                        let line = diagnostic.line;
                        view! {
                            <li>
                                <button
                                    class="jump"
                                    title="Show in editor"
                                    on:click=move |_| reveal(state.editor, line, 1)
                                >
                                    {diagnostic.to_string()}
                                </button>
                            </li>
                        }
                    })
                    .collect_view();
                view! {
                    <p class="line warn">"Not loaded:"</p>
                    <ul class="findings">{items}</ul>
                }
            });

            let output = (!output.is_empty()).then(|| {
                view! {
                    <details class="output" open>
                        <summary>"Output"</summary>
                        <pre>{output.clone()}</pre>
                    </details>
                }
            });

            view! { {outcome} {diagnostics} {output} }.into_any()
        })
    }
}

fn millis(ms: f64) -> String {
    if ms < 0.1 {
        "< 0.1 ms".to_owned()
    } else if ms < 10.0 {
        format!("{ms:.1} ms")
    } else {
        format!("{ms:.0} ms")
    }
}
