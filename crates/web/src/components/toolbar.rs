//! The header's buttons: open and save files, and the warning about leaving
//! the page with unsaved changes.

use leptos::ev;
use leptos::html;
use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Event, File, HtmlInputElement};

use crate::app::AppState;
use crate::files;

/// The file name offered for a program that was not opened from a file.
const DEFAULT_STEM: &str = "model";

/// The open file: what it is called and what it held when last saved or
/// opened. All fields are cheap handles.
#[derive(Clone, Copy)]
struct Document {
    state: AppState,
    stem: RwSignal<String>,
    /// The text as last saved or opened; the example counts as saved.
    saved: RwSignal<String>,
    dirty: Memo<bool>,
    error: RwSignal<Option<String>>,
}

impl Document {
    fn new(state: AppState) -> Self {
        let saved = RwSignal::new(state.source.get_untracked());
        Self {
            state,
            stem: RwSignal::new(DEFAULT_STEM.to_owned()),
            saved,
            dirty: Memo::new(move |_| state.source.with(|source| saved.with(|old| source != old))),
            error: RwSignal::new(None),
        }
    }

    /// Offers `content` as a download and marks the program as saved.
    fn save(self, extension: &str, mime: &str, content: &str) {
        let name = format!("{}.{extension}", self.stem.get_untracked());
        match files::download(&name, mime, content) {
            Ok(()) => {
                self.error.set(None);
                self.saved.set(self.state.source.get_untracked());
            }
            Err(cause) => self
                .error
                .set(Some(format!("Cannot save {name}: {cause:?}"))),
        }
    }

    /// Replaces the program with `file`, after asking if that loses changes.
    fn open(self, file: File) {
        let name = file.name();
        if self.dirty.get_untracked() && !confirm_discard(&name) {
            return;
        }
        spawn_local(async move {
            match JsFuture::from(file.text()).await {
                Ok(text) => {
                    let text = text.as_string().unwrap_or_default();
                    self.saved.set(replace_program(self.state, &text));
                    self.stem
                        .set(files::stem(&name).unwrap_or(DEFAULT_STEM).to_owned());
                    self.error.set(None);
                }
                Err(cause) => self
                    .error
                    .set(Some(format!("Cannot open {name}: {cause:?}"))),
            }
        });
    }
}

/// Open, save as Prolog and save as BPMN.
#[component]
pub(crate) fn Toolbar() -> impl IntoView {
    let state = expect_context::<AppState>();
    let document = Document::new(state);
    let picker = NodeRef::<html::Input>::new();

    let listener = window_event_listener(ev::beforeunload, move |event| {
        if document.dirty.get_untracked() {
            event.prevent_default();
            // Older browsers only look at the return value.
            event.set_return_value("");
        }
    });
    on_cleanup(move || listener.remove());

    // The XML is only offered if it was rendered from the text in the editor.
    let current_xml = Memo::new(move |_| {
        state.last.with(|last| {
            let last = last.as_ref()?;
            let current = state.source.with(|source| *source == last.program);
            current.then(|| last.xml().map(str::to_owned)).flatten()
        })
    });

    view! {
        <div class="toolbar">
            {move || document.error.get().map(|message| view! {
                <p class="toolbar-error" role="alert">{message}</p>
            })}
            <input
                type="file"
                accept=files::PROLOG_EXTENSIONS
                hidden
                node_ref=picker
                on:change=move |event: Event| {
                    let input = event_target::<HtmlInputElement>(&event);
                    let file = input.files().and_then(|files| files.get(0));
                    // Otherwise, picking the same file again would not fire `change`.
                    input.set_value("");
                    if let Some(file) = file {
                        document.open(file);
                    }
                }
            />
            <button
                class="button"
                title="Open a Prolog program"
                on:click=move |_| {
                    if let Some(input) = picker.get_untracked() {
                        input.click();
                    }
                }
            >
                "Open"
            </button>
            <button
                class="button"
                title="Download the program"
                on:click=move |_| {
                    state.source.with_untracked(|source| {
                        document.save("pl", files::PROLOG_MIME, source);
                    });
                }
            >
                "Save as Prolog"
            </button>
            <button
                class="button"
                title="Download the BPMN 2.0 XML, available once the current program has rendered"
                disabled=move || current_xml.with(Option::is_none)
                on:click=move |_| {
                    if let Some(xml) = current_xml.get_untracked() {
                        document.save("bpmn", files::BPMN_MIME, &xml);
                    }
                }
            >
                "Save as BPMN"
            </button>
        </div>
    }
}

/// Puts `text` into the editor as a new program and returns it as the editor
/// holds it: the text area normalises line endings, and the editor's caret
/// arithmetic relies on the source matching it.
fn replace_program(state: AppState, text: &str) -> String {
    let Some(textarea) = state.editor.get_untracked() else {
        return state.source.get_untracked();
    };
    textarea.set_value(text);
    let text = textarea.value();
    let _ = textarea.set_selection_range(0, 0);
    textarea.set_scroll_top(0);
    textarea.set_scroll_left(0);

    state.fit_diagram.set_value(true);
    state.source.set(text.clone());
    text
}

/// Asks whether unsaved changes may be replaced by the file called `name`.
fn confirm_discard(name: &str) -> bool {
    window()
        .confirm_with_message(&format!("Discard your unsaved changes and open {name}?"))
        .unwrap_or(false)
}
