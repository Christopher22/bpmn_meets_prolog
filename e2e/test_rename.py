"""Renaming from the editor and from the diagram."""

import pytest
from helpers import PROCESS, App
from playwright.sync_api import expect

CLAUSES = """\
double(X, Y) :- Y is X * 2.
twice(X, Y) :- Y is X + X.
"""


def start_rename(app: App, program: str, needle: str) -> None:
    app.set_program(program)
    app.caret_on(needle, 1)
    app.page.keyboard.press("F2")


def test_f2_renames_everywhere_except_strings_and_comments(app: App) -> None:
    start_rename(app, PROCESS, "work,")
    expect(app.rename_input).to_have_value("work")
    expect(app.rename_message).to_contain_text("2 occurrences · Enter renames, Escape cancels")

    app.rename_input.fill("labour")
    app.page.keyboard.press("Enter")

    expect(app.rename_input).to_have_count(0)
    expect(app.editor).to_be_focused()
    source = app.source()
    assert "task(labour, " in source
    assert "begin => labour => finish" in source
    assert '"work hard"' in source
    assert "% work is mentioned" in source

    app.wait_rendered()
    expect(app.element("labour")).to_contain_text("work hard")


def test_a_dsl_element_name_is_free_as_an_id(app: App) -> None:
    start_rename(app, PROCESS, "work,")
    app.rename_input.fill("task")
    app.page.keyboard.press("Enter")
    assert "task(task, " in app.source()


def test_a_variable_is_renamed_within_its_clause(app: App) -> None:
    start_rename(app, CLAUSES, "X")
    expect(app.rename_message).to_contain_text("2 occurrences")
    app.rename_input.fill("Value")
    app.page.keyboard.press("Enter")
    assert app.source() == (
        "double(Value, Y) :- Y is Value * 2.\ntwice(X, Y) :- Y is X + X.\n"
    )


@pytest.mark.parametrize(
    ("program", "needle", "new_name", "message"),
    [
        (PROCESS, "work,", "Upper", "Must start with a lowercase letter"),
        (PROCESS, "work,", "begin", "This name is already in use."),
        (PROCESS, "work,", "send", "This name is reserved by the BPMN DSL."),
        (PROCESS, "work,", "model", "This name is reserved by the BPMN DSL."),
        (CLAUSES, "X", "lower", "Must start with an uppercase letter or _"),
    ],
)
def test_invalid_names_cannot_be_committed(
    app: App, program: str, needle: str, new_name: str, message: str
) -> None:
    start_rename(app, program, needle)
    app.rename_input.fill(new_name)
    expect(app.rename_message).to_contain_text(message)
    expect(app.rename_input).to_have_attribute("aria-invalid", "true")

    app.page.keyboard.press("Enter")
    expect(app.rename_input).to_be_visible()
    assert app.source() == program


@pytest.mark.parametrize(
    ("needle", "message"),
    [
        ("model(", "model is needed for rendering and cannot be renamed."),
        ("bpmn)", "bpmn is needed for rendering and cannot be renamed."),
        ("task(", "task is part of the BPMN DSL and cannot be renamed."),
        ("comment", "Put the caret on an id, atom or variable to rename it."),
    ],
)
def test_refuses_what_cannot_be_renamed(app: App, needle: str, message: str) -> None:
    start_rename(app, PROCESS, needle)
    expect(app.rename_message).to_have_text(message)
    expect(app.rename_input).to_have_count(0)


def test_refuses_the_anonymous_variable(app: App) -> None:
    app.set_program("a(_).\n")
    app.caret_on("_", 1)
    app.page.keyboard.press("F2")
    expect(app.rename_message).to_have_text("The anonymous variable _ cannot be renamed.")


def test_escape_cancels(app: App) -> None:
    start_rename(app, PROCESS, "work,")
    app.rename_input.fill("labour")
    app.page.keyboard.press("Escape")
    expect(app.rename_input).to_have_count(0)
    expect(app.editor).to_be_focused()
    assert app.source() == PROCESS


def test_leaving_the_input_cancels(app: App) -> None:
    start_rename(app, PROCESS, "work,")
    app.rename_input.fill("labour")
    app.rename_input.blur()
    expect(app.rename_input).to_have_count(0)
    assert app.source() == PROCESS


def test_double_clicking_an_element_renames_it(app: App) -> None:
    app.show_program(PROCESS)
    app.element("work").dblclick()
    expect(app.rename_input).to_have_value("work")

    app.rename_input.fill("labour")
    app.page.keyboard.press("Enter")
    expect(app.element("labour")).to_be_visible()
    assert "task(labour, " in app.source()


def test_double_clicking_a_label_renames_its_element(app: App) -> None:
    app.show_program(PROCESS)
    app.element("begin_label").dblclick()
    expect(app.rename_input).to_have_value("begin")
