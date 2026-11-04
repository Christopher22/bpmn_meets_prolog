//! The input that asks for a new name.

use leptos::html;
use leptos::prelude::*;
use web_sys::KeyboardEvent;

use crate::app::AppState;
use crate::rename::{self, Target};

/// Asks how to rename `target`. Enter commits a valid name, Escape cancels, and
/// so does leaving the input.
#[component]
pub(super) fn RenameBox(
    target: Target,
    style: RwSignal<String>,
    on_commit: impl Fn(String) + 'static,
    on_escape: impl Fn() + 'static,
    on_blur: impl Fn() + 'static,
) -> impl IntoView {
    let state = expect_context::<AppState>();
    let input = NodeRef::<html::Input>::new();
    let value = RwSignal::new(target.name.clone());
    let initial = target.name.clone();
    let count = target.occurrences.len();

    let problem = Memo::new(move |_| {
        let name = value.get();
        state
            .source
            .with(|source| rename::validate(source, &target, &name))
            .err()
    });

    Effect::new(move |_| {
        if let Some(input) = input.get() {
            let _ = input.focus();
            input.select();
        }
    });

    let on_keydown = move |event: KeyboardEvent| match event.key().as_str() {
        "Enter" => {
            event.prevent_default();
            if problem.with_untracked(Option::is_none) {
                on_commit(value.get_untracked());
            }
        }
        "Escape" => {
            event.prevent_default();
            on_escape();
        }
        _ => {}
    };

    view! {
        <div class="rename-box" style=style>
            <input
                type="text"
                node_ref=input
                aria-label="New name"
                aria-invalid=move || problem.with(Option::is_some).to_string()
                spellcheck="false"
                autocomplete="off"
                autocapitalize="off"
                prop:value=initial
                on:input=move |event| value.set(event_target_value(&event))
                on:keydown=on_keydown
                on:blur=move |_| on_blur()
            />
            {move || match problem.get() {
                Some(problem) => view! {
                    <span class="rename-error" role="status">{problem.to_string()}</span>
                }
                .into_any(),
                None => view! {
                    <span class="rename-hint" role="status">
                        {match count {
                            1 => "1 occurrence".to_owned(),
                            n => format!("{n} occurrences"),
                        }}
                        " · Enter renames, Escape cancels"
                    </span>
                }
                .into_any(),
            }}
        </div>
    }
}
