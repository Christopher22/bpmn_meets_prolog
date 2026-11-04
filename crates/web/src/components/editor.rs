//! The program editor: a text area with transparent text over a
//! syntax-highlighted copy of it, with completion of DSL elements and
//! declared element ids, and whitespace-aware editing (see [`indent`]).

use std::ops::Range;

use leptos::html;
use leptos::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{
    HtmlDocument, HtmlTextAreaElement, InputEvent, KeyboardEvent, ScrollIntoViewOptions,
    ScrollLogicalPosition,
};

use super::rename_box::RenameBox;
use crate::app::AppState;
use crate::completion::{self, Completion};
use crate::highlight;
use crate::indent::{self, Edit};
use crate::rename::{self, Refusal, Target};

/// Assumed height of the completion list when deciding whether it fits below
/// the caret (matches `max-height` in `style.css`).
const LIST_HEIGHT: f64 = 220.0;

/// Assumed width of the completion list when keeping it inside the editor.
const LIST_WIDTH: f64 = 340.0;

/// An open completion list.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Popup {
    completion: Completion,
    selected: usize,
    /// CSS placing the list at the caret.
    style: String,
}

/// The highlighted program editor, bound to the program source.
#[component]
pub(crate) fn Editor(initial: &'static str) -> impl IntoView {
    let state = expect_context::<AppState>();
    let overlay = NodeRef::<html::Pre>::new();
    let measure = NodeRef::<html::Span>::new();
    let popup = RwSignal::new(None::<Popup>);
    let renaming = RwSignal::new(None::<Result<Target, Refusal>>);

    let highlighted = Memo::new(move |_| {
        let renaming = renaming.read();
        let marked = match &*renaming {
            Some(Ok(target)) => target.occurrences.as_slice(),
            _ => &[],
        };
        state.source.with(|source| {
            state
                .syntax
                .with(|report| highlight::to_html(source, report.result.as_ref().err(), marked))
        })
    });

    let sync_scroll = move || {
        if let (Some(textarea), Some(overlay)) =
            (state.editor.get_untracked(), overlay.get_untracked())
        {
            overlay.set_scroll_top(textarea.scroll_top());
            overlay.set_scroll_left(textarea.scroll_left());
        }
    };
    // New overlay content may change how far the overlay can scroll.
    Effect::new(move |_| {
        highlighted.track();
        request_animation_frame(sync_scroll);
    });

    let controller = Controller {
        state,
        measure,
        popup,
        inserting: StoredValue::new(false),
        leave_on_tab: StoredValue::new(false),
        renaming,
        rename_cursor: StoredValue::new(0),
        rename_style: RwSignal::new(String::new()),
    };
    let accept = move |index| controller.accept(index);

    // A double click in the diagram asks to rename an element.
    Effect::new(move |_| {
        if let Some(id) = state.rename_request.get() {
            state.rename_request.set(None);
            controller.rename_element(&id);
        }
    });

    view! {
        <div class="editor">
            <pre
                class="editor-code editor-overlay"
                aria-hidden="true"
                node_ref=overlay
                inner_html=move || highlighted.get()
            ></pre>
            <span class="editor-code editor-measure" aria-hidden="true" node_ref=measure>
                "0000000000"
            </span>
            <textarea
                class="editor-code editor-input"
                node_ref=state.editor
                aria-label="Prolog program. Tab indents; press Escape, then Tab, to leave. F2 renames."
                aria-autocomplete="list"
                aria-controls="completions"
                aria-expanded=move || popup.with(Option::is_some).to_string()
                aria-activedescendant=move || {
                    popup.with(|open| open.as_ref().map(|p| item_id(p.selected)))
                }
                spellcheck="false"
                autocomplete="off"
                autocapitalize="off"
                wrap="off"
                prop:value=initial
                on:input=move |event| controller.on_input(&event)
                on:keydown=move |event| controller.on_keydown(&event)
                on:scroll=move |_| {
                    sync_scroll();
                    popup.set(None);
                    controller.place_rename();
                }
                on:blur=move |_| {
                    popup.set(None);
                    controller.dismiss_refusal();
                }
                on:click=move |_| {
                    popup.set(None);
                    controller.dismiss_refusal();
                }
            ></textarea>
            {move || popup.get().map(|open| completion_list(open, accept))}
            {move || renaming.get().map(|outcome| rename_overlay(outcome, controller))}
        </div>
    }
}

/// Keyboard behaviour of the editor: completion and indentation. All fields
/// are cheap handles.
#[derive(Clone, Copy)]
struct Controller {
    state: AppState,
    measure: NodeRef<html::Span>,
    popup: RwSignal<Option<Popup>>,
    /// Set while the editor applies an edit itself, so that the resulting
    /// input event does not open the completion list.
    inserting: StoredValue<bool>,
    /// Set by Escape: the next Tab moves the focus instead of indenting, so
    /// keyboard users are not trapped in the editor.
    leave_on_tab: StoredValue<bool>,
    /// The open rename input, or why there is nothing to rename.
    renaming: RwSignal<Option<Result<Target, Refusal>>>,
    /// The byte offset the rename input belongs to.
    rename_cursor: StoredValue<usize>,
    /// CSS placing the rename input at `rename_cursor`.
    rename_style: RwSignal<String>,
}

impl Controller {
    /// Opens the rename input for the identifier at the caret.
    fn start_rename(self) {
        let Some(textarea) = self.state.editor.get_untracked() else {
            return;
        };
        let text = textarea.value();
        let caret = textarea.selection_start().ok().flatten().unwrap_or(0);
        let cursor = byte_offset(&text, caret);
        self.popup.set(None);
        let line = text[..cursor].matches('\n').count();
        keep_line_visible(&textarea, line, text.matches('\n').count() + 1);
        self.rename_cursor.set_value(cursor);
        self.place_rename();
        self.renaming.set(Some(rename::target(&text, cursor)));
    }

    /// Opens the rename input for the diagram element `id`, if the program
    /// still mentions it.
    fn rename_element(self, id: &str) {
        let text = self.state.source.get_untracked();
        let Some(offset) = rename::first_occurrence(&text, id) else {
            return;
        };
        let before = &text[..offset];
        let line = before.matches('\n').count() + 1;
        let column = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
        reveal(
            self.state.editor,
            u32::try_from(line).unwrap_or(u32::MAX),
            u32::try_from(column).unwrap_or(u32::MAX),
        );
        self.start_rename();
    }

    /// Keeps the rename input at its text, which may have scrolled.
    fn place_rename(self) {
        if let Some(textarea) = self.state.editor.get_untracked() {
            self.rename_style.set(placement(
                &textarea,
                self.measure,
                &textarea.value(),
                self.rename_cursor.get_value(),
            ));
        }
    }

    fn commit_rename(self, new_name: &str) {
        let Some(Some(Ok(target))) = self.renaming.try_get_untracked() else {
            return;
        };
        self.renaming.set(None);
        let Some(textarea) = self.state.editor.get_untracked() else {
            return;
        };
        let _ = textarea.focus();
        if new_name != target.name {
            let (top, left) = (textarea.scroll_top(), textarea.scroll_left());
            self.apply(&rename::edit(&textarea.value(), &target, new_name));
            textarea.set_scroll_top(top);
            textarea.set_scroll_left(left);
        }
    }

    fn cancel_rename(self) {
        self.renaming.set(None);
        if let Some(textarea) = self.state.editor.get_untracked() {
            let _ = textarea.focus();
        }
    }

    /// Closes the message shown when there was nothing to rename.
    fn dismiss_refusal(self) {
        if self
            .renaming
            .with_untracked(|renaming| matches!(renaming, Some(Err(_))))
        {
            self.renaming.set(None);
        }
    }

    /// Opens, updates or closes the list for the word at the caret.
    fn refresh(self, explicit: bool) {
        let Some(textarea) = self.state.editor.get_untracked() else {
            return;
        };
        let text = textarea.value();
        let caret = textarea.selection_start().ok().flatten().unwrap_or(0);
        let cursor = byte_offset(&text, caret);
        self.popup.set(
            completion::complete(&text, cursor, explicit).map(|completion| Popup {
                style: placement(&textarea, self.measure, &text, cursor),
                completion,
                selected: 0,
            }),
        );
    }

    fn accept(self, index: usize) {
        let Some(Popup { completion, .. }) = self.popup.get_untracked() else {
            return;
        };
        self.popup.set(None);
        let Some(item) = completion.items.get(index) else {
            return;
        };
        let start = completion.replace.start;
        let full = item.insert.len();
        let select = item.select.clone().unwrap_or(full..full);
        self.apply(&Edit {
            replace: completion.replace,
            insert: item.insert.clone(),
            select: start + select.start..start + select.end,
        });
    }

    /// Applies `edit` (computed from the current text and selection).
    fn edit(self, compute: impl FnOnce(&str, Range<usize>) -> Option<Edit>) -> bool {
        let Some(textarea) = self.state.editor.get_untracked() else {
            return false;
        };
        let text = textarea.value();
        let selection = selection(&textarea, &text);
        match compute(&text, selection) {
            Some(edit) => {
                self.apply(&edit);
                true
            }
            None => false,
        }
    }

    /// Replaces `edit.replace` (byte offsets) and sets the selection.
    fn apply(self, edit: &Edit) {
        let Some(textarea) = self.state.editor.get_untracked() else {
            return;
        };
        let text = textarea.value();
        let start = utf16_len(&text[..edit.replace.start]);
        let end = utf16_len(&text[..edit.replace.end]);
        let _ = textarea.focus();
        let _ = textarea.set_selection_range(start, end);

        // `insertText` and `delete` keep the browser's undo history and fire
        // `input`.
        let (command, value) = if edit.insert.is_empty() {
            ("delete", "")
        } else {
            ("insertText", edit.insert.as_str())
        };
        self.inserting.set_value(true);
        let applied = document()
            .dyn_into::<HtmlDocument>()
            .ok()
            .and_then(|document| {
                document
                    .exec_command_with_show_ui_and_value(command, false, value)
                    .ok()
            })
            .unwrap_or(false);
        self.inserting.set_value(false);
        if !applied {
            let _ = textarea.set_range_text_with_start_and_end(&edit.insert, start, end);
            self.state.source.set(textarea.value());
        }

        let text = textarea.value();
        let _ = textarea.set_selection_range(
            utf16_len(&text[..edit.select.start]),
            utf16_len(&text[..edit.select.end]),
        );
    }

    fn on_input(self, event: &web_sys::Event) {
        let Some(textarea) = self.state.editor.get_untracked() else {
            return;
        };
        self.state.source.set(textarea.value());
        if self.inserting.get_value() {
            return;
        }
        let kind = event
            .dyn_ref::<InputEvent>()
            .map(InputEvent::input_type)
            .unwrap_or_default();
        match kind.as_str() {
            "insertText" => self.refresh(false),
            "deleteContentBackward" | "deleteContentForward"
                if self.popup.with_untracked(Option::is_some) =>
            {
                self.refresh(false);
            }
            _ => self.popup.set(None),
        }
    }

    fn on_keydown(self, event: &KeyboardEvent) {
        if event.is_composing() {
            return;
        }
        self.dismiss_refusal();
        if event.key() == "F2" {
            event.prevent_default();
            self.start_rename();
            return;
        }
        let leave_on_tab = self.leave_on_tab.get_value();
        self.leave_on_tab.set_value(false);
        if event.ctrl_key() && event.code() == "Space" {
            event.prevent_default();
            self.refresh(true);
            return;
        }
        if self.popup.with_untracked(Option::is_some) {
            self.on_list_key(event);
        } else if !(event.ctrl_key() || event.alt_key() || event.meta_key()) {
            self.on_edit_key(event, leave_on_tab);
        }
    }

    /// Indentation keys while no completion list is open.
    fn on_edit_key(self, event: &KeyboardEvent, leave_on_tab: bool) {
        let handled = match event.key().as_str() {
            "Tab" if leave_on_tab => false,
            "Tab" if event.shift_key() => {
                // Nothing to outdent still must not move the focus.
                self.edit(indent::outdent);
                true
            }
            "Tab" => self.edit(|text, selection| Some(indent::indent(text, selection))),
            "Enter" => self.edit(|text, selection| Some(indent::newline(text, selection))),
            "Backspace" => self.edit(indent::backspace),
            "Escape" => {
                self.leave_on_tab.set_value(true);
                false
            }
            _ => false,
        };
        if handled {
            event.prevent_default();
        }
    }

    /// Navigation keys while the completion list is open.
    fn on_list_key(self, event: &KeyboardEvent) {
        let count = self
            .popup
            .with_untracked(|open| open.as_ref().map_or(1, |p| p.completion.items.len()));
        match event.key().as_str() {
            key @ ("ArrowDown" | "ArrowUp") => {
                event.prevent_default();
                let step = if key == "ArrowDown" { 1 } else { count - 1 };
                self.popup.update(|open| {
                    if let Some(open) = open {
                        open.selected = (open.selected + step) % count;
                        reveal_item(open.selected);
                    }
                });
            }
            "Enter" | "Tab" => {
                event.prevent_default();
                let selected = self
                    .popup
                    .with_untracked(|open| open.as_ref().map_or(0, |p| p.selected));
                self.accept(selected);
            }
            "Escape" => {
                event.prevent_default();
                self.popup.set(None);
            }
            "ArrowLeft" | "ArrowRight" | "Home" | "End" | "PageUp" | "PageDown" => {
                self.popup.set(None);
            }
            _ => {}
        }
    }
}

/// The rename input, or why there is nothing to rename.
fn rename_overlay(outcome: Result<Target, Refusal>, controller: Controller) -> impl IntoView {
    match outcome {
        Ok(target) => view! {
            <RenameBox
                target
                style=controller.rename_style
                on_commit=move |new_name: String| controller.commit_rename(&new_name)
                on_escape=move || controller.cancel_rename()
                on_blur=move || controller.renaming.set(None)
            />
        }
        .into_any(),
        Err(refusal) => view! {
            <p class="rename-box rename-error" role="status" style=controller.rename_style>
                {refusal.to_string()}
            </p>
        }
        .into_any(),
    }
}

/// The open completion list; `accept` inserts the item with the given index.
fn completion_list(popup: Popup, accept: impl Fn(usize) + Copy + 'static) -> impl IntoView {
    let Popup {
        completion,
        selected,
        style,
    } = popup;
    let items = completion
        .items
        .into_iter()
        .enumerate()
        .map(|(index, item)| {
            view! {
                <li
                    id=item_id(index)
                    role="option"
                    class:selected=index == selected
                    aria-selected=(index == selected).to_string()
                    // Keep the focus in the text area.
                    on:mousedown=move |event| {
                        event.prevent_default();
                        accept(index);
                    }
                >
                    <span class="completion-label">{item.label}</span>
                    <span class="completion-detail">{item.detail}</span>
                </li>
            }
        })
        .collect_view();
    view! {
        <ul class="completions" id="completions" role="listbox" aria-label="Completions" style=style>
            {items}
        </ul>
    }
}

fn item_id(index: usize) -> String {
    format!("completion-{index}")
}

/// Scrolls the list so that item `index` is visible once it is rendered.
fn reveal_item(index: usize) {
    request_animation_frame(move || {
        if let Some(element) = document().get_element_by_id(&item_id(index)) {
            let options = ScrollIntoViewOptions::new();
            options.set_block(ScrollLogicalPosition::Nearest);
            element.scroll_into_view_with_scroll_into_view_options(&options);
        }
    });
}

/// The selection of `textarea` as byte offsets into `text`.
fn selection(textarea: &HtmlTextAreaElement, text: &str) -> Range<usize> {
    let units = |offset: Result<Option<u32>, _>| offset.ok().flatten().unwrap_or(0);
    let start = byte_offset(text, units(textarea.selection_start()));
    let end = byte_offset(text, units(textarea.selection_end()));
    start.min(end)..start.max(end)
}

/// CSS that places the completion list below the caret at byte `cursor`, or
/// above it when there is no room below.
fn placement(
    textarea: &HtmlTextAreaElement,
    measure: NodeRef<html::Span>,
    text: &str,
    cursor: usize,
) -> String {
    let before = &text[..cursor];
    let line_start = before.rfind('\n').map_or(0, |newline| newline + 1);
    let line = before[..line_start].matches('\n').count();
    let column = indent::columns(&before[line_start..]);

    let char_width = measure
        .get_untracked()
        .map_or(8.0, |span| span.get_bounding_client_rect().width() / 10.0);
    let style = window().get_computed_style(textarea).ok().flatten();
    let pixels = |property: &str| {
        style
            .as_ref()
            .and_then(|style| style.get_property_value(property).ok())
            .and_then(|value| value.trim_end_matches("px").parse::<f64>().ok())
            .unwrap_or(0.0)
    };
    let line_height = pixels("line-height");

    let left =
        pixels("padding-left") + count(column) * char_width - f64::from(textarea.scroll_left());
    let left = left.clamp(
        0.0,
        (f64::from(textarea.client_width()) - LIST_WIDTH).max(0.0),
    );
    let below =
        pixels("padding-top") + count(line + 1) * line_height - f64::from(textarea.scroll_top());
    let height = f64::from(textarea.client_height());
    // A caret scrolled out of view leaves the list at the nearest edge.
    let below = below.min(height).max(line_height);
    if below + LIST_HEIGHT > height && below - line_height > LIST_HEIGHT {
        let bottom = height - (below - line_height);
        format!("left: {left:.0}px; bottom: {bottom:.0}px")
    } else {
        format!("left: {left:.0}px; top: {below:.0}px")
    }
}

fn count(n: usize) -> f64 {
    f64::from(u32::try_from(n).unwrap_or(u32::MAX))
}

/// The length of `text` in UTF-16 units, as the selection API counts.
fn utf16_len(text: &str) -> u32 {
    u32::try_from(text.encode_utf16().count()).unwrap_or(u32::MAX)
}

/// The byte offset of the UTF-16 offset `units` in `text`.
fn byte_offset(text: &str, units: u32) -> usize {
    let mut seen = 0;
    for (offset, c) in text.char_indices() {
        if seen >= units {
            return offset;
        }
        seen += u32::try_from(c.len_utf16()).unwrap_or(2);
    }
    text.len()
}

/// Moves the caret to `line:column` (both 1-based) and scrolls it into view.
pub(crate) fn reveal(editor: NodeRef<html::Textarea>, line: u32, column: u32) {
    let Some(textarea) = editor.get_untracked() else {
        return;
    };
    let text = textarea.value();
    let (offset, lines) = utf16_offset(&text, line, column);
    let _ = textarea.focus();
    let _ = textarea.set_selection_range(offset, offset);
    scroll_to_line(&textarea, line, lines);
}

/// The UTF-16 offset of `line:column` (as the selection API counts) and the
/// total number of lines.
fn utf16_offset(text: &str, line: u32, column: u32) -> (u32, u32) {
    let mut offset = 0;
    let mut lines = 0;
    for (number, content) in (1..).zip(text.split('\n')) {
        lines = number;
        if number < line {
            offset += content.encode_utf16().count() + 1;
        } else if number == line {
            let skip = usize::try_from(column.saturating_sub(1)).unwrap_or(usize::MAX);
            offset += content
                .chars()
                .take(skip)
                .map(char::len_utf16)
                .sum::<usize>();
        }
    }
    (u32::try_from(offset).unwrap_or(u32::MAX), lines)
}

/// Scrolls to the 0-based `line` unless it is already in view.
fn keep_line_visible(textarea: &HtmlTextAreaElement, line: usize, lines: usize) {
    let (line, lines) = (
        u32::try_from(line).unwrap_or(u32::MAX),
        u32::try_from(lines).unwrap_or(u32::MAX),
    );
    let line_height = f64::from(textarea.scroll_height()) / f64::from(lines.max(1));
    let top = f64::from(line) * line_height;
    let scrolled = f64::from(textarea.scroll_top());
    if top < scrolled || top + line_height > scrolled + f64::from(textarea.client_height()) {
        scroll_to_line(textarea, line.saturating_add(1), lines);
    }
}

fn scroll_to_line(textarea: &HtmlTextAreaElement, line: u32, lines: u32) {
    let line_height = f64::from(textarea.scroll_height()) / f64::from(lines.max(1));
    let target =
        f64::from(line.saturating_sub(1)) * line_height - f64::from(textarea.client_height()) / 3.0;
    // Whole pixels; the value is clamped to a small, non-negative range.
    #[allow(clippy::cast_possible_truncation)]
    textarea.set_scroll_top(target.max(0.0) as i32);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_utf16_units() {
        let text = "ab\nü𝄞x\nlast";
        assert_eq!(utf16_offset(text, 1, 1), (0, 3));
        assert_eq!(utf16_offset(text, 2, 1), (3, 3));
        // 'ü' is one unit, '𝄞' two.
        assert_eq!(utf16_offset(text, 2, 3), (6, 3));
        assert_eq!(utf16_offset(text, 3, 2), (9, 3));
    }

    #[test]
    fn converts_between_utf16_and_bytes() {
        let text = "aü𝄞b";
        for (units, bytes) in [(0, 0), (1, 1), (2, 3), (4, 7), (5, 8)] {
            assert_eq!(byte_offset(text, units), bytes);
            assert_eq!(utf16_len(&text[..bytes]), units);
        }
    }
}
