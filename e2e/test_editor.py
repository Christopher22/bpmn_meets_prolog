"""Indentation, focus and highlighting in the editor."""

from helpers import App
from playwright.sync_api import expect


def press_at(app: App, text: str, caret: int, key: str) -> None:
    app.set_program(text)
    app.select(caret)
    app.page.keyboard.press(key)


def test_enter_keeps_the_indentation(app: App) -> None:
    press_at(app, "    a,", 6, "Enter")
    assert app.source() == "    a,\n    "


def test_enter_goes_deeper_after_openers(app: App) -> None:
    for opener in "([{":
        press_at(app, f"a{opener}", 2, "Enter")
        assert app.source() == f"a{opener}\n    "


def test_enter_between_brackets_puts_the_closer_on_its_own_line(app: App) -> None:
    press_at(app, "a()", 2, "Enter")
    assert app.source() == "a(\n    \n)"
    assert app.selection() == (7, 7)


def test_tab_indents_the_current_line(app: App) -> None:
    press_at(app, "abc", 0, "Tab")
    assert app.source() == "    abc"
    expect(app.editor).to_be_focused()


def test_tab_and_shift_tab_change_selected_lines(app: App) -> None:
    app.set_program("a.\nb.\nc.\n")
    app.select(0, 5)
    app.page.keyboard.press("Tab")
    assert app.source() == "    a.\n    b.\nc.\n"

    app.page.keyboard.press("Shift+Tab")
    assert app.source() == "a.\nb.\nc.\n"


def test_shift_tab_with_nothing_to_outdent_keeps_the_focus(app: App) -> None:
    press_at(app, "abc", 0, "Shift+Tab")
    assert app.source() == "abc"
    expect(app.editor).to_be_focused()


def test_backspace_removes_a_level_of_indentation(app: App) -> None:
    press_at(app, "        z", 8, "Backspace")
    assert app.source() == "    z"


def test_backspace_returns_to_the_previous_tab_stop(app: App) -> None:
    press_at(app, "      z", 6, "Backspace")
    assert app.source() == "    z"


def test_backspace_after_text_removes_one_character(app: App) -> None:
    press_at(app, "    abc", 7, "Backspace")
    assert app.source() == "    ab"


def test_escape_then_tab_leaves_the_editor(app: App) -> None:
    app.editor.focus()
    app.page.keyboard.press("Escape")
    app.page.keyboard.press("Tab")
    expect(app.editor).not_to_be_focused()


def test_escape_only_frees_the_next_key(app: App) -> None:
    press_at(app, "abc", 0, "Escape")
    app.page.keyboard.press("z")
    app.page.keyboard.press("Home")
    app.page.keyboard.press("Tab")
    assert app.source() == "    zabc"
    expect(app.editor).to_be_focused()


def test_overlay_highlights_the_program(app: App) -> None:
    program = 'model(bpmn(m, [start(begin, "Begin")])). % note\n'
    app.set_program(program)
    overlay = app.page.locator(".editor-overlay")
    assert overlay.evaluate("node => node.textContent").startswith(program)
    expect(overlay.locator("span", has_text="start")).not_to_have_count(0)
    expect(overlay.locator("span", has_text="% note")).not_to_have_count(0)
